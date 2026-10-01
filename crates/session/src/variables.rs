use darmok_types::value::Value;
use thiserror::Error;

use crate::transaction::TransactionSettingsError;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum SessionVariableError {
    #[error("unknown canonical session variable '{0}'")]
    UnknownName(String),
    #[error("scoped SQL references require semantic classification")]
    ScopedReference,
    #[error("transaction settings require the staged command API")]
    TransactionCommandRequired,
    #[error(transparent)]
    TransactionOutcome(#[from] TransactionSettingsError),
    #[error("session variable '{0}' requires a string for translation identity")]
    NonStringTranslationValue(String),
}

/// Internal canonical session names, not SQL variable expressions. SQL scopes
/// and transaction SET forms must be classified before this interface is used.
/// Transaction reads derive from typed state; mutations use staged commands.
pub trait SessionVariableStore {
    fn get_system_var(&self, name: &str) -> Result<Option<Value>, SessionVariableError>;
    fn set_system_var(&mut self, name: &str, value: Value) -> Result<(), SessionVariableError>;
}

pub(crate) fn canonical_name(name: &str) -> Result<String, SessionVariableError> {
    let canonical = name.trim().to_ascii_lowercase();
    if canonical.starts_with('@') || canonical.contains('.') {
        return Err(SessionVariableError::ScopedReference);
    }
    if !is_declared_system_var(&canonical) {
        return Err(SessionVariableError::UnknownName(canonical));
    }
    Ok(canonical)
}

pub(crate) fn is_transaction_variable(name: &str) -> bool {
    matches!(
        name,
        "autocommit" | "transaction_isolation" | "transaction_read_only"
    )
}

fn is_declared_system_var(name: &str) -> bool {
    matches!(
        name,
        "autocommit"
            | "transaction_isolation"
            | "transaction_read_only"
            | "sql_mode"
            | "time_zone"
            | "default_storage_engine"
            | "storage_engine"
            | "foreign_key_checks"
            | "character_set_client"
            | "character_set_connection"
            | "character_set_database"
            | "character_set_filesystem"
            | "character_set_results"
            | "character_set_server"
            | "character_set_system"
            | "collation_connection"
            | "collation_database"
            | "collation_server"
            | "session_track_gtids"
            | "session_track_schema"
            | "session_track_state_change"
            | "session_track_system_variables"
            | "session_track_transaction_info"
            | "version"
            | "version_comment"
            | "version_compile_os"
            | "max_allowed_packet"
            | "auto_increment_increment"
            | "warning_count"
            | "error_count"
            | "wait_timeout"
            | "interactive_wait_timeout"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SessionState;

    #[test]
    fn canonical_nontransaction_values_remain_available() {
        let mut state = SessionState::new(11);
        state
            .set_system_var("SQL_MODE", Value::String("STRICT_TRANS_TABLES".into()))
            .unwrap();
        state
            .set_system_var("sql_mode", Value::String("ANSI_QUOTES".into()))
            .unwrap();
        assert_eq!(
            state.get_system_var("sql_mode").unwrap(),
            Some(Value::String("ANSI_QUOTES".into()))
        );
    }

    #[test]
    fn transaction_values_cannot_create_map_duplicates() {
        let mut state = SessionState::new(12);
        for name in [
            "autocommit",
            "transaction_isolation",
            "transaction_read_only",
        ] {
            assert_eq!(
                state.set_system_var(name, Value::Int(0)),
                Err(SessionVariableError::TransactionCommandRequired)
            );
        }
        assert_eq!(
            state.get_system_var("autocommit").unwrap(),
            Some(Value::UInt(1))
        );
        assert_eq!(
            state.get_system_var("transaction_isolation").unwrap(),
            Some(Value::String("REPEATABLE-READ".into()))
        );
        assert_eq!(
            state.get_system_var("transaction_read_only").unwrap(),
            Some(Value::UInt(0))
        );
    }

    #[test]
    fn canonical_store_does_not_erase_sql_scope_or_accept_aliases() {
        let mut state = SessionState::new(13);
        for name in [
            "@@SESSION.transaction_isolation",
            "@@transaction_isolation",
            "global.transaction_isolation",
        ] {
            assert_eq!(
                state.set_system_var(name, Value::String("SERIALIZABLE".into())),
                Err(SessionVariableError::ScopedReference)
            );
        }
        assert!(matches!(
            state.get_system_var("tx_isolation"),
            Err(SessionVariableError::UnknownName(_))
        ));
        assert!(matches!(
            state.set_system_var("unknown_system_var", Value::Int(1)),
            Err(SessionVariableError::UnknownName(_))
        ));
        assert_eq!(
            state.get_system_var("transaction_isolation").unwrap(),
            Some(Value::String("REPEATABLE-READ".into()))
        );
    }
}
