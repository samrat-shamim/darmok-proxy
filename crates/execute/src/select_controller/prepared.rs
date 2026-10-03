//! A source-admitted local SELECT template. Literal parsing happens once;
//! variable values and parameter representations resolve for each execution.

use super::*;
use darmok_types::value::PreparedStatementParamType;

pub(crate) struct PreparedSelect {
    columns: Vec<ColumnDefinition>,
    cells: Vec<PreparedCell>,
    parameters: Vec<ColumnDefinition>,
}

enum PreparedCell {
    Constant(Value),
    Variable { expression: Expr, label: String },
    Parameter(usize),
}

pub(crate) struct BinarySelect {
    pub(crate) columns: Vec<ColumnDefinition>,
    pub(crate) row: Vec<Value>,
}

impl PreparedSelect {
    pub(crate) fn admit(
        state: &SessionState,
        globals: &ServerSetValues,
        source: SourceSelect<'_>,
    ) -> Result<Self, SelectAdmissionError> {
        check_clauses(source.query(), source.select())?;
        let text = state.text_collation()?;
        let mut columns = Vec::with_capacity(source.select().projection.len());
        let mut cells = Vec::with_capacity(columns.capacity());
        let mut parameters = Vec::new();
        for (index, item) in source.select().projection.iter().enumerate() {
            let (expr, alias) = match item {
                SelectItem::UnnamedExpr(expr) => (expr, None),
                SelectItem::ExprWithAlias { expr, alias } => (expr, Some(alias.value.as_str())),
                _ => return Err(SelectSqlError::Unsupported.into()),
            };
            let spelling = source.item_source(index)?;
            let (mut cell, template) = if matches!(expr, Expr::Value(value) if matches!(&value.value, Literal::Placeholder(value) if value == "?"))
            {
                let cell = parameter_cell(text);
                let parameter = parameters.len();
                parameters.push(column(parameter_cell(text)));
                (cell, PreparedCell::Parameter(parameter))
            } else {
                let cell = evaluate(state, globals, text, source, expr, spelling)?;
                let template = if let Some(expression) = variable_leaf(expr) {
                    PreparedCell::Variable {
                        expression: expression.clone(),
                        label: spelling.to_owned(),
                    }
                } else {
                    PreparedCell::Constant(binary_value(&cell)?)
                };
                (cell, template)
            };
            finish_name(&mut cell, alias)?;
            columns.push(column(cell));
            cells.push(template);
        }
        if u16::try_from(columns.len()).is_err() || u16::try_from(parameters.len()).is_err() {
            return Err(SelectSqlError::Unsupported.into());
        }
        Ok(Self {
            columns,
            cells,
            parameters,
        })
    }

    pub(crate) fn columns(&self) -> &[ColumnDefinition] {
        &self.columns
    }
    pub(crate) fn parameters(&self) -> &[ColumnDefinition] {
        &self.parameters
    }

    pub(crate) fn resolve(
        &mut self,
        state: &SessionState,
        globals: &ServerSetValues,
        values: &[Value],
        types: &[PreparedStatementParamType],
    ) -> Result<BinarySelect, SelectAdmissionError> {
        if values.len() != self.parameters.len() || types.len() != values.len() {
            return Err(SourceProvenanceError.into());
        }
        // A semantic error cannot partially replace derived result metadata.
        let mut columns = self.columns.clone();
        let mut row = Vec::with_capacity(self.cells.len());
        for (index, cell) in self.cells.iter().enumerate() {
            row.push(match cell {
                PreparedCell::Constant(value) => value.clone(),
                PreparedCell::Variable { expression, label } => {
                    let cell =
                        variable(state, globals, state.text_collation()?, expression, label)?;
                    binary_value(&cell)?
                }
                PreparedCell::Parameter(parameter) => {
                    let value = &values[*parameter];
                    let metadata = types[*parameter];
                    if !matches!(
                        metadata.field_type,
                        ty::TINY
                            | ty::SHORT
                            | ty::LONG
                            | ty::LONGLONG
                            | ty::INT24
                            | ty::VAR_STRING
                            | ty::NULL
                    ) {
                        return Err(SelectSqlError::Unsupported.into());
                    }
                    match value {
                        Value::Null => {} // NULL preserves the derived type, including before its first non-NULL binding.
                        Value::Int(_) | Value::UInt(_) => {
                            columns[index].column_type = ty::LONGLONG;
                            columns[index].column_length = 21;
                            columns[index].character_set = charset::BINARY;
                            columns[index].flags =
                                flag::BINARY | if metadata.unsigned { flag::UNSIGNED } else { 0 };
                            columns[index].decimals = 0;
                        }
                        Value::Bytes(bytes) if columns[index].column_type == ty::VAR_STRING => {
                            std::str::from_utf8(bytes).map_err(|_| SelectSqlError::Unsupported)?;
                        }
                        // Numeric-to-text input can require MySQL coercion and
                        // warnings. It must not reset the sticky derived type.
                        _ => return Err(SelectSqlError::Unsupported.into()),
                    }
                    value.clone()
                }
            });
        }
        self.columns.clone_from(&columns);
        Ok(BinarySelect { columns, row })
    }
}

fn parameter_cell(text: &CharsetInfo) -> Cell {
    Cell {
        value: None,
        name: Bytes::from_static(b"?"),
        mysql_type: ty::VAR_STRING,
        charset: text.id,
        width: 65535 / u32::from(text.max_len) * u32::from(text.max_len),
        flags: 0,
        decimals: 31,
    }
}

fn variable_leaf(mut expression: &Expr) -> Option<&Expr> {
    loop {
        match expression {
            Expr::Nested(inner)
            | Expr::UnaryOp {
                op: UnaryOperator::Plus,
                expr: inner,
            } => expression = inner,
            Expr::MySqlSystemVariable(_) => return Some(expression),
            _ => return None,
        }
    }
}

fn binary_value(cell: &Cell) -> Result<Value, SourceProvenanceError> {
    let Some(value) = &cell.value else {
        return Ok(Value::Null);
    };
    match cell.mysql_type {
        ty::LONGLONG => {
            let text = std::str::from_utf8(value).map_err(|_| SourceProvenanceError)?;
            if cell.flags & flag::UNSIGNED != 0 {
                text.parse()
                    .map(Value::UInt)
                    .map_err(|_| SourceProvenanceError)
            } else {
                text.parse()
                    .map(Value::Int)
                    .map_err(|_| SourceProvenanceError)
            }
        }
        ty::NEWDECIMAL => Ok(Value::Decimal(
            std::str::from_utf8(value)
                .map_err(|_| SourceProvenanceError)?
                .into(),
        )),
        ty::VAR_STRING => Ok(Value::Bytes(value.clone())),
        _ => Err(SourceProvenanceError),
    }
}
