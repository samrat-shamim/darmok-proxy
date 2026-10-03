use darmok_types::value::PreparedStatementParamType;
use std::collections::HashMap;
use std::fmt;
use std::num::NonZeroU32;

/// A prepared statement registered by the client. Mutation stays in the
/// registry so callers cannot bypass accounting or parameter validation.
#[derive(Clone, PartialEq)]
pub struct PreparedStatement<P> {
    id: u32,
    original_sql: String,
    param_count: u16,
    last_param_types: Vec<PreparedStatementParamType>,
    plan: P,
}

impl<P> PreparedStatement<P> {
    pub fn id(&self) -> u32 {
        self.id
    }
    pub fn original_sql(&self) -> &str {
        &self.original_sql
    }
    pub fn param_count(&self) -> u16 {
        self.param_count
    }
    pub fn last_param_types(&self) -> &[PreparedStatementParamType] {
        &self.last_param_types
    }
    pub fn plan(&self) -> &P {
        &self.plan
    }
    pub fn plan_mut(&mut self) -> &mut P {
        &mut self.plan
    }
}

impl<P> fmt::Debug for PreparedStatement<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedStatement")
            .field("id", &self.id)
            .field("sql_bytes", &self.original_sql.len())
            .field("param_count", &self.param_count)
            .field("last_param_types", &self.last_param_types)
            .finish()
    }
}

/// Per-session limits. A zero count disables prepared statements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedStatementLimits {
    pub max_statements: usize,
    pub max_sql_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreparedStatementError {
    #[error("prepared statement count limit reached ({limit})")]
    CountLimit { limit: usize },
    #[error("prepared statement SQL byte limit exceeded ({limit})")]
    SqlByteLimit { limit: usize },
    #[error("prepared statement IDs exhausted; reset the connection")]
    IdsExhausted,
    #[error("unknown prepared statement {id}")]
    UnknownStatement { id: u32 },
    #[error("prepared statement expects {expected} parameter types, received {actual}")]
    ParameterTypeCount { expected: u16, actual: usize },
}

/// Bounded prepared-statement storage owned by one authenticated session.
/// Statement IDs are not reused until the connection is reset. Exhaustion
/// fails explicitly instead of wrapping and overwriting a live statement.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedStatementRegistry<P> {
    stmts: HashMap<u32, PreparedStatement<P>>,
    next_id: Option<NonZeroU32>,
    sql_bytes: usize,
    limits: PreparedStatementLimits,
}

impl<P> PreparedStatementRegistry<P> {
    pub fn new(limits: PreparedStatementLimits) -> Self {
        Self {
            stmts: HashMap::new(),
            next_id: NonZeroU32::new(1),
            sql_bytes: 0,
            limits,
        }
    }

    pub fn register(
        &mut self,
        sql: String,
        param_count: u16,
        plan: P,
    ) -> Result<u32, PreparedStatementError> {
        if self.stmts.len() >= self.limits.max_statements {
            return Err(PreparedStatementError::CountLimit {
                limit: self.limits.max_statements,
            });
        }
        let sql_bytes = self
            .sql_bytes
            .checked_add(sql.len())
            .filter(|bytes| *bytes <= self.limits.max_sql_bytes)
            .ok_or(PreparedStatementError::SqlByteLimit {
                limit: self.limits.max_sql_bytes,
            })?;
        let id = self
            .next_id
            .ok_or(PreparedStatementError::IdsExhausted)?
            .get();
        self.next_id = id.checked_add(1).and_then(NonZeroU32::new);
        self.stmts.insert(
            id,
            PreparedStatement {
                id,
                original_sql: sql,
                param_count,
                last_param_types: Vec::new(),
                plan,
            },
        );
        self.sql_bytes = sql_bytes;
        Ok(id)
    }

    pub fn get(&self, id: u32) -> Option<&PreparedStatement<P>> {
        self.stmts.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut PreparedStatement<P>> {
        self.stmts.get_mut(&id)
    }

    pub fn remove(&mut self, id: u32) -> Option<PreparedStatement<P>> {
        let statement = self.stmts.remove(&id)?;
        self.sql_bytes -= statement.original_sql.len();
        Some(statement)
    }

    pub fn set_param_types(
        &mut self,
        id: u32,
        param_types: Vec<PreparedStatementParamType>,
    ) -> Result<(), PreparedStatementError> {
        let statement = self
            .stmts
            .get_mut(&id)
            .ok_or(PreparedStatementError::UnknownStatement { id })?;
        if param_types.len() != usize::from(statement.param_count) {
            return Err(PreparedStatementError::ParameterTypeCount {
                expected: statement.param_count,
                actual: param_types.len(),
            });
        }
        statement.last_param_types = param_types;
        Ok(())
    }

    /// Invalidate all statements during connection reset/change-user.
    pub fn clear(&mut self) {
        self.stmts.clear();
        self.sql_bytes = 0;
        self.next_id = NonZeroU32::new(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(count: usize, bytes: usize) -> PreparedStatementRegistry<()> {
        PreparedStatementRegistry::new(PreparedStatementLimits {
            max_statements: count,
            max_sql_bytes: bytes,
        })
    }

    #[test]
    fn registration_tracks_metadata_and_removal_releases_capacity() {
        let mut reg = registry(1, 8);
        let id = reg.register("SELECT ?".into(), 1, ()).unwrap();
        let statement = reg.get(id).unwrap();
        assert_eq!(
            (
                statement.id(),
                statement.original_sql(),
                statement.param_count()
            ),
            (1, "SELECT ?", 1)
        );
        assert!(statement.last_param_types().is_empty());
        assert_eq!(
            reg.register("SELECT 2".into(), 0, ()),
            Err(PreparedStatementError::CountLimit { limit: 1 })
        );
        assert!(reg.remove(id).is_some());
        assert!(reg.get(id).is_none());
        assert!(reg.remove(id).is_none());
        assert_eq!(reg.register("SELECT 2".into(), 0, ()).unwrap(), 2);
    }

    #[test]
    fn byte_limit_counts_utf8_and_rejections_preserve_accounting_and_ids() {
        let mut reg = registry(4, 3);
        let first = reg.register("é".into(), 0, ()).unwrap();
        assert_eq!(
            reg.register("é".into(), 0, ()),
            Err(PreparedStatementError::SqlByteLimit { limit: 3 })
        );
        assert_eq!(reg.register("x".into(), 0, ()).unwrap(), 2);
        reg.remove(first).unwrap();
        assert_eq!(reg.register("é".into(), 0, ()).unwrap(), 3);
        assert!(matches!(
            reg.register("x".into(), 0, ()),
            Err(PreparedStatementError::SqlByteLimit { .. })
        ));
    }

    #[test]
    fn exhausted_ids_do_not_wrap_or_overwrite_existing_statements() {
        let mut reg = registry(4, 100);
        let first = reg.register("SELECT 1".into(), 0, ()).unwrap();
        reg.next_id = NonZeroU32::new(u32::MAX);
        assert_eq!(reg.register("SELECT 2".into(), 0, ()).unwrap(), u32::MAX);
        assert_eq!(
            reg.register("SELECT 3".into(), 0, ()),
            Err(PreparedStatementError::IdsExhausted)
        );
        assert_eq!(reg.get(first).unwrap().original_sql(), "SELECT 1");
        assert!(reg.get(0).is_none());
        reg.clear();
        assert!(reg.get(u32::MAX).is_none());
        assert_eq!(reg.register("SELECT 4".into(), 0, ()).unwrap(), 1);
    }

    #[test]
    fn invalid_parameter_metadata_does_not_replace_the_previous_types() {
        let mut reg = registry(2, 100);
        let id = reg.register("SELECT ?".into(), 1, ()).unwrap();
        let types = vec![PreparedStatementParamType {
            field_type: 0x03,
            unsigned: true,
        }];
        reg.set_param_types(id, types.clone()).unwrap();
        assert_eq!(
            reg.set_param_types(id, vec![]),
            Err(PreparedStatementError::ParameterTypeCount {
                expected: 1,
                actual: 0
            })
        );
        assert_eq!(reg.get(id).unwrap().last_param_types(), types);
        assert_eq!(
            reg.set_param_types(42, vec![]),
            Err(PreparedStatementError::UnknownStatement { id: 42 })
        );
    }

    #[test]
    fn clear_releases_byte_capacity_and_keeps_configured_limits() {
        let mut reg = registry(1, 8);
        reg.register("SELECT 1".into(), 0, ()).unwrap();
        reg.clear();
        assert_eq!(reg.register("SELECT 2".into(), 0, ()).unwrap(), 1);
        assert!(matches!(
            reg.register("SELECT 3".into(), 0, ()),
            Err(PreparedStatementError::CountLimit { .. })
        ));
        assert!(matches!(
            registry(0, 100).register("SELECT 1".into(), 0, ()),
            Err(PreparedStatementError::CountLimit { limit: 0 })
        ));
    }

    #[test]
    fn debug_output_does_not_reveal_sql_literals() {
        let mut reg = registry(1, 100);
        reg.register("SELECT 'confidential-value'".into(), 0, ())
            .unwrap();
        assert!(!format!("{reg:?}").contains("confidential-value"));
    }
}
