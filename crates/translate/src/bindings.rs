use std::collections::HashMap;
use std::fmt;
use std::ops::ControlFlow;

use darmok_types::{Result, Value as ParameterValue};
use sqlparser::ast::{Value, VisitMut, VisitorMut};
use sqlparser::emitter::{EmitOptions, PgEmitter};

use crate::frontend::{ParsedStatement, rejected};

/// Emitted syntax and an explicit backend-to-client binding map. This still
/// requires semantic approval and catalog binding before execution.
#[derive(Clone)]
pub struct EmittedStatement {
    sql: String,
    client_parameter_count: u16,
    /// Zero-based source client slots, in dense PostgreSQL parameter order.
    parameter_order: Vec<u16>,
}

impl fmt::Debug for EmittedStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmittedStatement")
            .field("sql_bytes", &self.sql.len())
            .field("client_parameter_count", &self.client_parameter_count)
            .field("parameter_order", &self.parameter_order)
            .finish()
    }
}

impl EmittedStatement {
    pub fn sql(&self) -> &str {
        &self.sql
    }
    pub fn parameter_count(&self) -> u16 {
        self.parameter_order.len() as u16
    }
    pub fn parameter_order(&self) -> &[u16] {
        &self.parameter_order
    }

    /// Validate the full frontend binding arity even if a rewrite eliminated a
    /// parameter. Do not clone values or parse SQL text to rediscover positions.
    pub fn bind<'a>(&self, client_values: &'a [ParameterValue]) -> Result<Vec<&'a ParameterValue>> {
        if client_values.len() != usize::from(self.client_parameter_count) {
            return Err(rejected(
                "binding_count",
                "client parameter count differs from prepared metadata",
            ));
        }
        Ok(self
            .parameter_order
            .iter()
            .map(|slot| &client_values[usize::from(*slot)])
            .collect())
    }
}

impl ParsedStatement {
    /// Emit a rewritten AST, densely numbering backend parameters. Duplicating
    /// a placeholder reuses its binding; eliminating one removes the backend
    /// slot without changing the required client arity.
    pub fn emit_postgres(&self) -> Result<EmittedStatement> {
        self.check_limits()?;
        let mut ast = self.ast().clone();
        let mut bindings = Bindings {
            client_count: self.client_parameter_count,
            backend_slots: HashMap::new(),
            parameter_order: Vec::new(),
        };
        if let ControlFlow::Break(error) = ast.visit(&mut bindings) {
            return Err(error);
        }
        let mut sql = String::new();
        PgEmitter::new(EmitOptions::postgres())
            .emit_statement_owned(ast, &mut sql)
            .map_err(|_| rejected("emit", "AST cannot be emitted as PostgreSQL syntax"))?;
        Ok(EmittedStatement {
            sql,
            client_parameter_count: self.client_parameter_count,
            parameter_order: bindings.parameter_order,
        })
    }
}

struct Bindings {
    client_count: u16,
    backend_slots: HashMap<u16, u16>,
    parameter_order: Vec<u16>,
}

impl VisitorMut for Bindings {
    type Break = darmok_types::ProxyError;

    fn pre_visit_value(&mut self, value: &mut Value) -> ControlFlow<Self::Break> {
        let Value::Placeholder(placeholder) = value else {
            return ControlFlow::Continue(());
        };
        let Some(index) = placeholder
            .strip_prefix('$')
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|index| *index > 0 && *index <= self.client_count)
        else {
            return ControlFlow::Break(rejected(
                "binding_identity",
                "rewriter introduced an unknown parameter identity",
            ));
        };
        if *placeholder != format!("${index}") {
            return ControlFlow::Break(rejected(
                "binding_identity",
                "noncanonical parameter identity in rewritten AST",
            ));
        }
        let slot = index - 1;
        let backend = *self.backend_slots.entry(slot).or_insert_with(|| {
            self.parameter_order.push(slot);
            self.parameter_order.len() as u16
        });
        *placeholder = format!("${backend}");
        ControlFlow::Continue(())
    }
}
