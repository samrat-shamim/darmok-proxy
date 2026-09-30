use darmok_types::error::{ExecutionError, ProxyError, Result};
use darmok_types::value::Value;

use crate::state::SessionState;

/// Trait for reading and writing MySQL session-level system variables.
pub trait SessionVariableStore {
    /// Retrieve a system variable by name (case-insensitive lookup recommended).
    fn get_system_var(&self, name: &str) -> Option<Value>;

    /// Set a system variable. Returns an error if the variable is read-only or
    /// the value is invalid.
    fn set_system_var(&mut self, name: &str, value: Value) -> Result<()>;
}

impl SessionVariableStore for SessionState {
    fn get_system_var(&self, name: &str) -> Option<Value> {
        self.system_variables
            .get(&normalize_system_var_name(name))
            .cloned()
    }

    fn set_system_var(&mut self, name: &str, value: Value) -> Result<()> {
        let normalized = normalize_system_var_name(name);
        if !is_declared_system_var(&normalized) {
            return Err(ProxyError::execution(
                ExecutionError::UnknownSystemVariable(normalized),
            ));
        }

        self.system_variables.insert(normalized, value);
        Ok(())
    }
}

fn normalize_system_var_name(name: &str) -> String {
    let mut normalized = name
        .trim()
        .trim_matches('`')
        .to_ascii_lowercase()
        .replace('-', "_");

    for prefix in [
        "@@session.",
        "@@local.",
        "@@global.",
        "@@",
        "session.",
        "local.",
        "global.",
    ] {
        if let Some(stripped) = normalized.strip_prefix(prefix) {
            normalized = stripped.to_owned();
            break;
        }
    }

    normalized
}

fn is_declared_system_var(name: &str) -> bool {
    matches!(
        name,
        "autocommit"
            | "sql_mode"
            | "time_zone"
            | "transaction_isolation"
            | "tx_isolation"
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
    fn session_state_gets_and_sets_system_variables() {
        let mut state = SessionState::new(11);

        // The store is name-agnostic; these cover the session variable names
        // referenced elsewhere in the Darmok MySQL-to-PostgreSQL proxy.
        let cases = vec![
            ("sql_mode", Value::String("STRICT_TRANS_TABLES".into())),
            ("autocommit", Value::Bool(true)),
            (
                "transaction_isolation",
                Value::String("READ-COMMITTED".into()),
            ),
            ("tx_isolation", Value::String("READ-COMMITTED".into())),
            ("time_zone", Value::String("UTC".into())),
            ("character_set_client", Value::String("utf8mb4".into())),
            ("character_set_connection", Value::String("utf8mb4".into())),
            ("character_set_results", Value::String("utf8mb4".into())),
            (
                "collation_connection",
                Value::String("utf8mb4_general_ci".into()),
            ),
            (
                "collation_server",
                Value::String("utf8mb4_general_ci".into()),
            ),
            (
                "collation_database",
                Value::String("utf8mb4_general_ci".into()),
            ),
            ("version", Value::String("8.4.0-darmok-0.1.0".into())),
            (
                "version_comment",
                Value::String("Darmok MySQL-to-PostgreSQL proxy".into()),
            ),
        ];

        for (name, value) in &cases {
            assert_eq!(state.get_system_var(name), None);
            state.set_system_var(name, value.clone()).unwrap();
            assert_eq!(state.get_system_var(name), Some(value.clone()));
        }

        assert_eq!(state.system_variables.len(), cases.len());
    }

    #[test]
    fn system_variable_store_normalizes_declared_names() {
        let mut state = SessionState::new(13);

        state
            .set_system_var(
                "@@SESSION.Transaction-Isolation",
                Value::String("SERIALIZABLE".into()),
            )
            .unwrap();

        assert_eq!(
            state.get_system_var("transaction_isolation"),
            Some(Value::String("SERIALIZABLE".into()))
        );
        assert!(state.system_variables.contains_key("transaction_isolation"));
    }

    #[test]
    fn system_variable_store_rejects_undeclared_names() {
        let mut state = SessionState::new(14);

        let error = state
            .set_system_var("unknown_system_var", Value::Int(1))
            .unwrap_err();

        assert_eq!(
            error.mysql_error_code,
            darmok_types::mysql_const::error_code::ER_UNKNOWN_SYSTEM_VARIABLE
        );
        assert_eq!(state.system_variables.len(), 0);
    }

    #[test]
    fn setting_a_system_variable_overwrites_the_previous_value() {
        let mut state = SessionState::new(12);

        state
            .set_system_var("sql_mode", Value::String("STRICT_TRANS_TABLES".into()))
            .unwrap();
        state
            .set_system_var("sql_mode", Value::String("ANSI_QUOTES".into()))
            .unwrap();

        assert_eq!(
            state.get_system_var("sql_mode"),
            Some(Value::String("ANSI_QUOTES".into()))
        );
        assert_eq!(state.system_variables.len(), 1);
    }
}
