//! Pure admission of local SELECT values and their declared MySQL metadata.
//! Table execution, general expressions and diagnostic variables remain gates.

use bytes::Bytes;
use darmok_protocol::ColumnDefinition;
use darmok_session::{
    CharsetInfo, SessionInputError, SessionState, SessionVariable, SessionVariableReader,
    SystemVariableForm, TransactionSettingsError, classify_mysql_system_variable_read,
};
use darmok_types::{
    mysql_const::{charset, column_flag as flag, field_type as ty},
    value::Value,
};
use sqlparser::{
    ast::{
        ContextModifier, Expr, GroupByExpr, Query, Select, SelectFlavor, SelectItem, UnaryOperator,
        Value as Literal,
    },
    source::{SourceProvenanceError, SourceSelect},
};

use crate::ServerSetValues;
use crate::exact_number::ExactNumber;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectSqlError {
    Unsupported,
    UnknownVariable,
    GlobalVariable,
}

impl SelectSqlError {
    pub fn code(self) -> u16 {
        match self {
            Self::Unsupported => 1235,
            Self::UnknownVariable => 1193,
            Self::GlobalVariable => 1238,
        }
    }
    pub fn sql_state(self) -> [u8; 5] {
        match self {
            Self::Unsupported => *b"42000",
            _ => *b"HY000",
        }
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::Unsupported => {
                "This SELECT expression, clause or variable value is not implemented"
            }
            Self::UnknownVariable => "Unknown system variable",
            Self::GlobalVariable => "Variable is a GLOBAL variable",
        }
    }
}

pub(crate) enum SelectAdmissionError {
    Sql(SelectSqlError),
    Source(SourceProvenanceError),
    Settings(TransactionSettingsError),
}
impl From<SelectSqlError> for SelectAdmissionError {
    fn from(error: SelectSqlError) -> Self {
        Self::Sql(error)
    }
}
impl From<SourceProvenanceError> for SelectAdmissionError {
    fn from(error: SourceProvenanceError) -> Self {
        Self::Source(error)
    }
}
impl From<TransactionSettingsError> for SelectAdmissionError {
    fn from(error: TransactionSettingsError) -> Self {
        Self::Settings(error)
    }
}

pub(crate) struct SelectPlan {
    pub(crate) columns: Vec<ColumnDefinition>,
    pub(crate) row: Vec<Option<Bytes>>,
}

struct Cell {
    value: Option<Bytes>,
    name: Bytes,
    mysql_type: u8,
    flags: u16,
    charset: u16,
    width: u32,
    decimals: u8,
}

pub(crate) fn admit_select(
    state: &SessionState,
    globals: &ServerSetValues,
    source: SourceSelect<'_>,
) -> Result<SelectPlan, SelectAdmissionError> {
    check_clauses(source.query(), source.select())?;
    let text = state.text_collation()?;
    let mut columns = Vec::with_capacity(source.select().projection.len());
    let mut row = Vec::with_capacity(columns.capacity());
    for (index, item) in source.select().projection.iter().enumerate() {
        let (expr, alias) = match item {
            SelectItem::UnnamedExpr(expr) => (expr, None),
            SelectItem::ExprWithAlias { expr, alias } => (expr, Some(alias.value.as_str())),
            _ => return Err(SelectSqlError::Unsupported.into()),
        };
        let mut cell = evaluate(
            state,
            globals,
            text,
            source,
            expr,
            source.item_source(index)?,
        )?;
        if let Some(alias) = alias {
            // MySQL reports warnings for alias trimming. That outcome is not
            // implemented; it cannot be silently replaced by a clean SELECT.
            if alias.starts_with(|ch: char| ch.is_ascii_control() || ch == ' ')
                || alias.len() > 256
                || alias.chars().any(|ch| ch as u32 > 0xffff)
            {
                return Err(SelectSqlError::Unsupported.into());
            }
            cell.name = Bytes::copy_from_slice(alias.as_bytes());
        } else {
            cell.name = generated_name(cell.name)?;
        }
        columns.push(ColumnDefinition {
            catalog: Bytes::from_static(b"def"),
            schema: Bytes::new(),
            table: Bytes::new(),
            org_table: Bytes::new(),
            name: cell.name,
            org_name: Bytes::new(),
            character_set: cell.charset,
            column_length: cell.width,
            column_type: cell.mysql_type,
            flags: cell.flags,
            decimals: cell.decimals,
        });
        row.push(cell.value);
    }
    Ok(SelectPlan { columns, row })
}

fn generated_name(bytes: Bytes) -> Result<Bytes, SelectSqlError> {
    let original = std::str::from_utf8(&bytes).map_err(|_| SelectSqlError::Unsupported)?;
    let name = original.trim_start_matches(|ch: char| ch.is_ascii_control() || ch == ' ');
    if !name.chars().any(|ch| ch as u32 > 0xffff) {
        if name.len() > 256 {
            return Err(SelectSqlError::Unsupported);
        }
        return Ok(bytes.slice(original.len() - name.len()..));
    }
    // MySQL names use the three-byte system charset even when row text uses
    // utf8mb4. Supplementary characters become '?' in autogenerated names.
    let converted: String = name
        .chars()
        .map(|ch| if ch as u32 > 0xffff { '?' } else { ch })
        .collect();
    if converted.len() > 256 {
        return Err(SelectSqlError::Unsupported);
    }
    Ok(Bytes::from(converted))
}

fn check_clauses(query: &Query, select: &Select) -> Result<(), SelectSqlError> {
    // Exhaustive destructuring makes additions to the AST an admission decision.
    let Query {
        with,
        body: _,
        order_by,
        limit_clause,
        fetch,
        locks,
        for_clause,
        settings,
        format_clause,
        pipe_operators,
    } = query;
    let Select {
        select_token: _,
        optimizer_hints,
        distinct,
        select_modifiers,
        top,
        top_before_distinct,
        projection,
        exclude,
        into,
        from,
        lateral_views,
        prewhere,
        selection,
        connect_by,
        group_by,
        cluster_by,
        distribute_by,
        sort_by,
        having,
        named_window,
        qualify,
        window_before_qualify,
        value_table_mode,
        flavor,
    } = select;
    if with.is_some()
        || order_by.is_some()
        || limit_clause.is_some()
        || fetch.is_some()
        || !locks.is_empty()
        || for_clause.is_some()
        || settings.is_some()
        || format_clause.is_some()
        || !pipe_operators.is_empty()
        || !optimizer_hints.is_empty()
        || distinct.is_some()
        || select_modifiers.is_some()
        || top.is_some()
        || *top_before_distinct
        || projection.is_empty()
        || exclude.is_some()
        || into.is_some()
        || !from.is_empty()
        || !lateral_views.is_empty()
        || prewhere.is_some()
        || selection.is_some()
        || !connect_by.is_empty()
        || !matches!(group_by, GroupByExpr::Expressions(exprs, modifiers) if exprs.is_empty() && modifiers.is_empty())
        || !cluster_by.is_empty()
        || !distribute_by.is_empty()
        || !sort_by.is_empty()
        || having.is_some()
        || !named_window.is_empty()
        || qualify.is_some()
        || *window_before_qualify
        || value_table_mode.is_some()
        || *flavor != SelectFlavor::Standard
    {
        return Err(SelectSqlError::Unsupported);
    }
    Ok(())
}

fn evaluate(
    state: &SessionState,
    globals: &ServerSetValues,
    text: &CharsetInfo,
    source: SourceSelect<'_>,
    expr: &Expr,
    spelling: &str,
) -> Result<Cell, SelectAdmissionError> {
    match expr {
        Expr::Nested(expr) => evaluate(state, globals, text, source, expr, spelling),
        Expr::UnaryOp {
            op: UnaryOperator::Plus,
            expr,
        } => {
            // MySQL unary plus preserves these selected values and types.
            match unwrap_nested(expr) {
                Expr::Value(value)
                    if matches!(value.value, Literal::Boolean(_) | Literal::Null) =>
                {
                    evaluate(state, globals, text, source, expr, spelling)
                }
                Expr::Value(value) if matches!(value.value, Literal::Number(_, false)) => {
                    number_cell(source, expr, spelling)
                }
                Expr::Value(value)
                    if matches!(
                        value.value,
                        Literal::HexStringLiteral(_) | Literal::BitStringLiteral(_)
                    ) =>
                {
                    evaluate(state, globals, text, source, expr, spelling)
                }
                Expr::Prefixed { .. } => evaluate(state, globals, text, source, expr, spelling),
                Expr::UnaryOp {
                    op: UnaryOperator::Plus | UnaryOperator::Minus,
                    ..
                } => evaluate(state, globals, text, source, expr, spelling),
                Expr::MySqlSystemVariable(_) => {
                    evaluate(state, globals, text, source, expr, spelling)
                }
                _ => Err(SelectSqlError::Unsupported.into()),
            }
        }
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            ..
        } => number_cell(source, expr, spelling),
        Expr::Value(value) => match &value.value {
            Literal::Number(_, false) => number_cell(source, expr, spelling),
            Literal::Boolean(value) => Ok(numeric_cell(
                Bytes::from_static(if *value { b"1" } else { b"0" }),
                spelling,
                1,
                flag::NOT_NULL,
            )),
            Literal::Null => Ok(Cell {
                value: None,
                name: Bytes::from_static(b"NULL"),
                mysql_type: ty::NULL,
                charset: charset::BINARY,
                width: 0,
                flags: flag::BINARY,
                decimals: 0,
            }),
            Literal::SingleQuotedString(value) | Literal::DoubleQuotedString(value) => {
                let width = u32::try_from(value.chars().count())
                    .ok()
                    .and_then(|chars| chars.checked_mul(u32::from(text.max_len)))
                    .ok_or(SelectSqlError::Unsupported)?;
                Ok(text_cell(
                    value.as_bytes(),
                    value,
                    text,
                    width,
                    flag::NOT_NULL,
                ))
            }
            Literal::HexStringLiteral(_) | Literal::BitStringLiteral(_) => {
                binary_cell(&value.value, spelling, false)
            }
            _ => Err(SelectSqlError::Unsupported.into()),
        },
        Expr::Prefixed { prefix, value }
            if prefix.quote_style.is_none() && prefix.value.eq_ignore_ascii_case("_binary") =>
        {
            let Expr::Value(value) = value.as_ref() else {
                return Err(SelectSqlError::Unsupported.into());
            };
            binary_cell(&value.value, spelling, true)
        }
        Expr::MySqlSystemVariable(_) => variable(state, globals, text, expr, spelling),
        _ => Err(SelectSqlError::Unsupported.into()),
    }
}

fn binary_cell(
    literal: &Literal,
    spelling: &str,
    introduced: bool,
) -> Result<Cell, SelectAdmissionError> {
    let (value, unsigned) = match literal {
        Literal::HexStringLiteral(digits) => (crate::binary_literal::hex(digits)?, true),
        Literal::BitStringLiteral(digits) => (crate::binary_literal::bits(digits)?, false),
        _ => return Err(SelectSqlError::Unsupported.into()),
    };
    Ok(Cell {
        width: u32::try_from(value.len()).map_err(|_| SelectSqlError::Unsupported)?,
        value: Some(value),
        name: Bytes::copy_from_slice(spelling.as_bytes()),
        mysql_type: ty::VAR_STRING,
        flags: flag::NOT_NULL
            | flag::BINARY
            | if unsigned && !introduced {
                flag::UNSIGNED
            } else {
                0
            },
        charset: charset::BINARY,
        decimals: if introduced { 31 } else { 0 },
    })
}

fn unwrap_nested(mut expr: &Expr) -> &Expr {
    while let Expr::Nested(inner) = expr {
        expr = inner;
    }
    expr
}

fn number_expression<'a>(
    source: SourceSelect<'a>,
    expr: &Expr,
) -> Result<(ExactNumber<'a>, &'a str, bool), SelectAdmissionError> {
    match expr {
        Expr::Nested(inner)
        | Expr::UnaryOp {
            op: UnaryOperator::Plus,
            expr: inner,
        } => number_expression(source, inner),
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            expr: inner,
        } => {
            let (number, token, _) = number_expression(source, inner)?;
            Ok((
                number.negate().map_err(|_| SelectSqlError::Unsupported)?,
                token,
                true,
            ))
        }
        Expr::Value(value) if matches!(value.value, Literal::Number(_, false)) => {
            let token = source.number_source(value)?;
            Ok((
                ExactNumber::from_token(token).map_err(|_| SelectSqlError::Unsupported)?,
                token,
                false,
            ))
        }
        _ => Err(SelectSqlError::Unsupported.into()),
    }
}

fn number_cell(
    source: SourceSelect<'_>,
    expr: &Expr,
    spelling: &str,
) -> Result<Cell, SelectAdmissionError> {
    let (number, token, negation) = number_expression(source, expr)?;
    let declaration = number.metadata();
    Ok(Cell {
        value: Some(number.into_text()),
        name: Bytes::copy_from_slice(if negation { spelling } else { token }.as_bytes()),
        mysql_type: declaration.mysql_type,
        flags: declaration.flags,
        charset: charset::BINARY,
        width: declaration.width,
        decimals: declaration.scale,
    })
}

fn numeric_cell(value: Bytes, label: &str, width: u32, flags: u16) -> Cell {
    Cell {
        value: Some(value),
        name: Bytes::copy_from_slice(label.as_bytes()),
        mysql_type: ty::LONGLONG,
        flags: flags | flag::BINARY,
        charset: charset::BINARY,
        width,
        decimals: 0,
    }
}

fn text_cell(value: &[u8], label: &str, text: &CharsetInfo, width: u32, flags: u16) -> Cell {
    Cell {
        value: Some(Bytes::copy_from_slice(value)),
        name: Bytes::copy_from_slice(label.as_bytes()),
        mysql_type: ty::VAR_STRING,
        flags,
        charset: text.id,
        width,
        decimals: 31,
    }
}

fn variable(
    state: &SessionState,
    globals: &ServerSetValues,
    text: &CharsetInfo,
    expr: &Expr,
    label: &str,
) -> Result<Cell, SelectAdmissionError> {
    let read = classify_mysql_system_variable_read(expr).map_err(|error| match error {
        SessionInputError::UnknownVariable => SelectSqlError::UnknownVariable,
        _ => SelectSqlError::Unsupported,
    })?;
    use SessionVariable as Var;
    let global = read.form == SystemVariableForm::Qualified(ContextModifier::Global);
    let value = match read.variable {
        Var::Version | Var::VersionComment | Var::VersionCompileOs => {
            if matches!(
                read.form,
                SystemVariableForm::Qualified(ContextModifier::Session | ContextModifier::Local)
            ) {
                return Err(SelectSqlError::GlobalVariable.into());
            }
            state
                .read_variable(read.variable)
                .map_err(|_| SelectSqlError::Unsupported)?
        }
        Var::Autocommit
        | Var::CompletionType
        | Var::TransactionIsolation
        | Var::TransactionReadOnly
        | Var::SqlMode => {
            if global {
                crate::set_controller::global_value(globals, read.variable)
                    .map_err(|_| SelectSqlError::Unsupported)?
            } else {
                state
                    .read_variable(read.variable)
                    .map_err(|_| SelectSqlError::Unsupported)?
            }
        }
        Var::TimeZone
        | Var::CharacterSetClient
        | Var::CharacterSetConnection
        | Var::CharacterSetResults
        | Var::CollationConnection
            if !global =>
        {
            state
                .read_variable(read.variable)
                .map_err(|_| SelectSqlError::Unsupported)?
        }
        _ => return Err(SelectSqlError::Unsupported.into()),
    };
    match value {
        Value::UInt(value) => Ok(numeric_cell(Bytes::from(value.to_string()), label, 1, 0)),
        // The MySQL 8.4.11 system-variable string declaration is 21845
        // characters; the declared byte width is independent of current value.
        Value::String(value) => Ok(text_cell(
            value.as_bytes(),
            label,
            text,
            21845 * u32::from(text.max_len),
            0,
        )),
        _ => Err(SelectSqlError::Unsupported.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darmok_session::{AutocommitSetting, MysqlCompatibilityProfile, SqlModes};
    use sqlparser::source::parse_mysql_source;

    fn fixture() -> (SessionState, ServerSetValues) {
        (
            SessionState::new(1),
            ServerSetValues {
                sql_modes: SqlModes::MYSQL84_DEFAULT,
                transactions: MysqlCompatibilityProfile::default_mysql8()
                    .default_transaction_characteristics,
                autocommit: AutocommitSetting::Enabled,
                completion_type: darmok_session::FrontendCompletionType::NoChain,
            },
        )
    }

    #[test]
    fn local_cells_keep_declared_metadata_separate_from_normalized_values() {
        let (state, globals) = fixture();
        let parsed = parse_mysql_source(
            "SELECT 001, -2 AS negative, 9223372036854775808, 'hé', '', null, (True)",
            state.sql_modes().unwrap().parser_flags(),
        )
        .unwrap();
        let plan = admit_select(&state, &globals, parsed.single_select().unwrap().unwrap())
            .ok()
            .unwrap();
        let observed: Vec<_> = plan
            .columns
            .iter()
            .map(|c| {
                (
                    c.name.as_ref(),
                    c.column_type,
                    c.column_length,
                    c.flags,
                    c.character_set,
                    c.decimals,
                )
            })
            .collect();
        assert_eq!(
            observed,
            vec![
                (
                    b"001".as_slice(),
                    ty::LONGLONG,
                    4,
                    flag::NOT_NULL | flag::BINARY,
                    63,
                    0
                ),
                (
                    b"negative".as_slice(),
                    ty::LONGLONG,
                    2,
                    flag::NOT_NULL | flag::BINARY,
                    63,
                    0
                ),
                (
                    b"9223372036854775808".as_slice(),
                    ty::LONGLONG,
                    19,
                    flag::NOT_NULL | flag::UNSIGNED | flag::BINARY,
                    63,
                    0
                ),
                ("hé".as_bytes(), ty::VAR_STRING, 8, flag::NOT_NULL, 45, 31),
                (b"".as_slice(), ty::VAR_STRING, 0, flag::NOT_NULL, 45, 31),
                (b"NULL".as_slice(), ty::NULL, 0, flag::BINARY, 63, 0),
                (
                    b"(True)".as_slice(),
                    ty::LONGLONG,
                    1,
                    flag::NOT_NULL | flag::BINARY,
                    63,
                    0
                ),
            ]
        );
        assert_eq!(
            plan.row,
            vec![
                Some(Bytes::from_static(b"1")),
                Some(Bytes::from_static(b"-2")),
                Some(Bytes::from_static(b"9223372036854775808")),
                Some(Bytes::from("hé")),
                Some(Bytes::new()),
                None,
                Some(Bytes::from_static(b"1"))
            ]
        );
        assert!(plan.columns.iter().all(|c| c.catalog == b"def".as_slice()
            && c.schema.is_empty()
            && c.table.is_empty()
            && c.org_table.is_empty()
            && c.org_name.is_empty()));
    }

    #[test]
    fn source_labels_and_explicit_global_authority_follow_selected_scope_contract() {
        let (mut state, mut globals) = fixture();
        state.set_sql_modes(SqlModes::empty()).unwrap();
        globals.autocommit = AutocommitSetting::Disabled;
        let parsed = parse_mysql_source("SELECT ( @@SESSION . AUTOCOMMIT ), @@global.autocommit AS global_auto, @@session.sql_mode, @@GLOBAL.version AS version", state.sql_modes().unwrap().parser_flags()).unwrap();
        let plan = admit_select(&state, &globals, parsed.single_select().unwrap().unwrap())
            .ok()
            .unwrap();
        assert_eq!(
            plan.columns[0].name,
            b"( @@SESSION . AUTOCOMMIT )".as_slice()
        );
        assert_eq!(plan.columns[0].flags, flag::BINARY);
        assert_eq!(plan.row[0].as_deref(), Some(b"1".as_slice()));
        assert_eq!(plan.row[1].as_deref(), Some(b"0".as_slice()));
        assert_eq!(plan.columns[2].column_length, 87380);
        assert_eq!(plan.columns[2].flags, 0);
        assert_eq!(plan.row[2].as_deref(), Some(b"".as_slice()));
        assert_eq!(
            plan.row[3].as_deref(),
            Some(
                MysqlCompatibilityProfile::default_mysql8()
                    .server_version
                    .as_bytes()
            )
        );
        for sql in ["SELECT @@SESSION.version", "SELECT @@LOCAL.version_comment"] {
            let parsed =
                parse_mysql_source(sql, state.sql_modes().unwrap().parser_flags()).unwrap();
            assert!(matches!(
                admit_select(&state, &globals, parsed.single_select().unwrap().unwrap()),
                Err(SelectAdmissionError::Sql(SelectSqlError::GlobalVariable))
            ));
        }
    }

    #[test]
    fn generated_labels_use_system_names_while_rows_keep_utf8mb4_text() {
        let (state, globals) = fixture();
        let parsed = parse_mysql_source(
            "SELECT 'é', '😀', ' hi ', ' x', ' hi ' AS untrimmed_value",
            state.sql_modes().unwrap().parser_flags(),
        )
        .unwrap();
        let plan = admit_select(&state, &globals, parsed.single_select().unwrap().unwrap())
            .ok()
            .unwrap();
        for (index, (name, value, width)) in [
            ("é", "é", 4),
            ("?", "😀", 4),
            ("hi ", " hi ", 16),
            (" x", " x", 8),
            ("untrimmed_value", " hi ", 16),
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(plan.columns[index].name, name.as_bytes());
            assert_eq!(plan.columns[index].column_length, *width);
            assert_eq!(plan.row[index].as_deref(), Some(value.as_bytes()));
        }
    }

    #[test]
    fn unimplemented_valid_select_semantics_return_explicit_errors() {
        let (state, globals) = fixture();
        for sql in [
            "SELECT 1 WHERE FALSE",
            "SELECT 1 LIMIT 0",
            "SELECT DISTINCT 1",
            "SELECT 1 ORDER BY 1",
            "SELECT 1 + 2",
            "SELECT CAST(1 AS DECIMAL(3, 2))",
            "SELECT 1e2",
            "SELECT @@global.time_zone",
            "SELECT @@warning_count",
        ] {
            let parsed =
                parse_mysql_source(sql, state.sql_modes().unwrap().parser_flags()).unwrap();
            assert!(
                matches!(
                    admit_select(&state, &globals, parsed.single_select().unwrap().unwrap()),
                    Err(SelectAdmissionError::Sql(SelectSqlError::Unsupported))
                ),
                "{sql}"
            );
        }
    }

    #[test]
    fn exact_decimal_values_keep_source_names_scale_and_declared_widths() {
        let (state, globals) = fixture();
        let parsed = parse_mysql_source(
            "SELECT 12.50, 001.230, .50, 1., 0.000, -0.00, +001.230, (001.230), (-001.230), -(001.230) AS via_neg",
            state.sql_modes().unwrap().parser_flags(),
        ).unwrap();
        let plan = admit_select(&state, &globals, parsed.single_select().unwrap().unwrap())
            .ok()
            .unwrap();
        for (index, (name, text, width, scale)) in [
            ("12.50", "12.50", 6, 2),
            ("001.230", "1.230", 7, 3),
            (".50", "0.50", 4, 2),
            ("1.", "1", 2, 0),
            ("0.000", "0.000", 6, 3),
            ("-0.00", "0.00", 5, 2),
            ("001.230", "1.230", 7, 3),
            ("001.230", "1.230", 7, 3),
            ("(-001.230)", "-1.230", 7, 3),
            ("via_neg", "-1.230", 7, 3),
        ]
        .into_iter()
        .enumerate()
        {
            let c = &plan.columns[index];
            assert_eq!(c.name, name.as_bytes());
            assert_eq!(c.column_type, ty::NEWDECIMAL);
            assert_eq!(c.column_length, width);
            assert_eq!(c.decimals, scale);
            assert_eq!(c.flags, flag::BINARY | flag::NOT_NULL);
            assert_eq!(c.character_set, charset::BINARY);
            assert_eq!(plan.row[index].as_deref(), Some(text.as_bytes()));
        }
    }

    #[test]
    fn exact_integer_promotions_and_nested_negation_follow_declared_types() {
        let (state, globals) = fixture();
        let parsed = parse_mysql_source(
            "SELECT 18446744073709551615 AS unsigned_value, 18446744073709551616 AS decimal_value, -9223372036854775808 AS signed_minimum, -9223372036854775809 AS below_signed, -18446744073709551615 AS negative_unsigned, -(-9223372036854775808), +(-1.00), -(-1.00), -0018446744073709551615, 0018446744073709551616",
            state.sql_modes().unwrap().parser_flags(),
        ).unwrap();
        let plan = admit_select(&state, &globals, parsed.single_select().unwrap().unwrap())
            .ok()
            .unwrap();
        for (index, (text, kind, width, scale, unsigned)) in [
            ("18446744073709551615", ty::LONGLONG, 20, 0, true),
            ("18446744073709551616", ty::NEWDECIMAL, 21, 0, false),
            ("-9223372036854775808", ty::LONGLONG, 20, 0, false),
            ("-9223372036854775809", ty::NEWDECIMAL, 20, 0, false),
            ("-18446744073709551615", ty::NEWDECIMAL, 21, 0, false),
            ("9223372036854775808", ty::NEWDECIMAL, 20, 0, false),
            ("-1.00", ty::NEWDECIMAL, 5, 2, false),
            ("1.00", ty::NEWDECIMAL, 5, 2, false),
            ("-18446744073709551615", ty::NEWDECIMAL, 23, 0, false),
            ("18446744073709551616", ty::NEWDECIMAL, 22, 0, false),
        ]
        .into_iter()
        .enumerate()
        {
            let c = &plan.columns[index];
            assert_eq!(c.column_type, kind);
            assert_eq!(c.column_length, width);
            assert_eq!(c.decimals, scale);
            assert_eq!(
                c.flags,
                flag::BINARY | flag::NOT_NULL | if unsigned { flag::UNSIGNED } else { 0 }
            );
            assert_eq!(plan.row[index].as_deref(), Some(text.as_bytes()));
        }
        assert_eq!(plan.columns[5].name, b"-(-9223372036854775808)".as_slice());
        assert_eq!(plan.columns[6].name, b"+(-1.00)".as_slice());
        assert_eq!(plan.columns[7].name, b"-(-1.00)".as_slice());
    }

    #[test]
    fn literal_precision_is_separate_from_decimal_column_scale() {
        let (state, globals) = fixture();
        let sql = "SELECT 99999999999999999999999999999999999999999999999999999999999999999 AS precision65, .1234567890123456789012345678901 AS scale31";
        let parsed = parse_mysql_source(sql, state.sql_modes().unwrap().parser_flags()).unwrap();
        let plan = admit_select(&state, &globals, parsed.single_select().unwrap().unwrap())
            .ok()
            .unwrap();
        assert_eq!(plan.columns[0].column_type, ty::NEWDECIMAL);
        assert_eq!(plan.columns[0].column_length, 66);
        assert_eq!(plan.columns[0].decimals, 0);
        assert_eq!(
            plan.row[0].as_deref(),
            Some(b"99999999999999999999999999999999999999999999999999999999999999999".as_slice())
        );
        assert_eq!(plan.columns[1].column_type, ty::NEWDECIMAL);
        assert_eq!(plan.columns[1].column_length, 33);
        assert_eq!(plan.columns[1].decimals, 31);
        assert_eq!(
            plan.row[1].as_deref(),
            Some(b"0.1234567890123456789012345678901".as_slice())
        );
    }

    #[test]
    fn nested_plus_preserves_previously_admitted_boolean_null_and_variable_values() {
        let (state, globals) = fixture();
        let parsed = parse_mysql_source(
            "SELECT +(+TRUE), +(+(NULL)), +(+@@autocommit)",
            state.sql_modes().unwrap().parser_flags(),
        )
        .unwrap();
        let plan = admit_select(&state, &globals, parsed.single_select().unwrap().unwrap())
            .ok()
            .unwrap();
        assert_eq!(plan.columns[0].name.as_ref(), b"+(+TRUE)");
        assert_eq!(plan.columns[0].column_type, ty::LONGLONG);
        assert_eq!(plan.row[0].as_deref(), Some(b"1".as_slice()));
        assert_eq!(plan.columns[1].name.as_ref(), b"NULL");
        assert_eq!(plan.columns[1].column_type, ty::NULL);
        assert!(plan.row[1].is_none());
        assert_eq!(plan.columns[2].name.as_ref(), b"+(+@@autocommit)");
        assert_eq!(plan.row[2].as_deref(), Some(b"1".as_slice()));
    }

    #[test]
    fn negating_signed_negative_operands_uses_mysql_declared_decimal_promotion() {
        let (state, globals) = fixture();
        let parsed = parse_mysql_source(
            "SELECT -(-1), -(-0), -(-(001)), -(-9223372036854775807), -(-(-1)), +(-(-1)), -(+(-1)), -(-18446744073709551615)",
            state.sql_modes().unwrap().parser_flags(),
        ).unwrap();
        let plan = admit_select(&state, &globals, parsed.single_select().unwrap().unwrap())
            .ok()
            .unwrap();
        for (index, (name, value, kind, width)) in [
            ("-(-1)", "1", ty::NEWDECIMAL, 2),
            ("-(-0)", "0", ty::LONGLONG, 2),
            ("-(-(001))", "1", ty::NEWDECIMAL, 4),
            (
                "-(-9223372036854775807)",
                "9223372036854775807",
                ty::NEWDECIMAL,
                20,
            ),
            ("-(-(-1))", "-1", ty::NEWDECIMAL, 2),
            ("+(-(-1))", "1", ty::NEWDECIMAL, 2),
            ("-(+(-1))", "1", ty::NEWDECIMAL, 2),
            (
                "-(-18446744073709551615)",
                "18446744073709551615",
                ty::NEWDECIMAL,
                21,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(plan.columns[index].name.as_ref(), name.as_bytes());
            assert_eq!(plan.columns[index].column_type, kind);
            assert_eq!(plan.columns[index].column_length, width);
            assert_eq!(plan.columns[index].decimals, 0);
            assert_eq!(plan.columns[index].flags, flag::BINARY | flag::NOT_NULL);
            assert_eq!(plan.row[index].as_deref(), Some(value.as_bytes()));
        }
    }
}

#[cfg(test)]
pub(crate) mod binary_tests;
