use std::collections::HashMap;

use darmok_types::value::Value;

use crate::compatibility::MysqlCompatibilityProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarningLevel {
    Note,
    Warning,
    Error,
}

impl WarningLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Warning => "Warning",
            Self::Error => "Error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionWarning {
    pub level: WarningLevel,
    pub code: u16,
    pub message: String,
}

/// Tracks the lifecycle of a transaction within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransactionState {
    /// No active transaction.
    #[default]
    Idle,
    /// A transaction is in progress.
    Active,
    /// The transaction has encountered an error and must be rolled back.
    Failed,
}

/// Fields from session state relevant for translation cache fingerprinting.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TranslationFingerprint {
    pub database: Option<String>,
    pub sql_mode: String,
    pub charset_id: u16,
    pub collation: String,
    pub character_set_client: String,
    pub character_set_connection: String,
    pub character_set_results: String,
    pub timezone: String,
    pub transaction_isolation: String,
}

/// Per-connection session state that mirrors what a MySQL client expects.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionState {
    pub connection_id: u32,
    pub database: Option<String>,
    /// MySQL charset collation ID from the compatibility profile.
    pub charset_id: u16,
    /// MySQL collation name from the compatibility profile.
    pub collation: String,
    pub timezone: String,
    pub sql_mode: String,
    /// MySQL transaction isolation level (default "READ-COMMITTED").
    pub transaction_isolation: String,
    pub autocommit: bool,
    pub transaction_state: TransactionState,
    pub client_capabilities: u32,
    pub last_insert_id: u64,
    pub affected_rows: u64,
    pub row_count: i64,
    pub warnings: u16,
    pub found_rows: u64,
    pub warning_stack: Vec<SessionWarning>,
    /// Storage for session-level system variables (e.g. `@@sql_mode`).
    pub system_variables: HashMap<String, Value>,
}

impl Default for SessionState {
    fn default() -> Self {
        let profile = MysqlCompatibilityProfile::default_mysql8();
        let default_collation = profile.default_collation_info();
        Self {
            connection_id: 0,
            database: None,
            charset_id: default_collation.id,
            collation: default_collation.collation.to_owned(),
            timezone: profile.default_time_zone.to_owned(),
            sql_mode: profile.default_sql_mode.to_owned(),
            transaction_isolation: profile.default_transaction_isolation.to_owned(),
            autocommit: true,
            transaction_state: TransactionState::default(),
            client_capabilities: 0,
            last_insert_id: 0,
            affected_rows: 0,
            row_count: 0,
            warnings: 0,
            found_rows: 0,
            warning_stack: Vec::new(),
            system_variables: HashMap::new(),
        }
    }
}

impl SessionState {
    /// Create a new session state for the given connection id.
    pub fn new(connection_id: u32) -> Self {
        Self {
            connection_id,
            ..Default::default()
        }
    }

    /// Extract session fields relevant for translation cache fingerprinting.
    pub fn translation_fingerprint(&self) -> TranslationFingerprint {
        let profile = MysqlCompatibilityProfile::default_mysql8();
        TranslationFingerprint {
            database: self.database.clone(),
            sql_mode: self.sql_mode.clone(),
            charset_id: self.charset_id,
            collation: self.collation.clone(),
            character_set_client: self
                .system_variable_string_or_default("character_set_client", profile.default_charset),
            character_set_connection: self.system_variable_string_or_default(
                "character_set_connection",
                profile.default_charset,
            ),
            character_set_results: self.system_variable_string_or_default(
                "character_set_results",
                profile.default_charset,
            ),
            timezone: self.timezone.clone(),
            transaction_isolation: self.transaction_isolation.clone(),
        }
    }

    pub fn system_variable_string_or_default(&self, name: &str, default: &str) -> String {
        match self.system_variables.get(name) {
            Some(Value::String(value)) => value.to_string(),
            Some(Value::Bool(value)) => {
                if *value {
                    "1".to_owned()
                } else {
                    "0".to_owned()
                }
            }
            Some(Value::Int(value)) => value.to_string(),
            Some(Value::UInt(value)) => value.to_string(),
            Some(Value::Float(value)) => value.to_string(),
            Some(Value::Decimal(value)) => value.to_string(),
            Some(Value::Bytes(value)) => String::from_utf8_lossy(value).into_owned(),
            Some(Value::Null) | None => default.to_owned(),
            Some(Value::Date { .. } | Value::Time { .. } | Value::DateTime { .. }) => {
                default.to_owned()
            }
        }
    }

    pub fn clear_warning_stack(&mut self) {
        self.warning_stack.clear();
        self.warnings = 0;
    }

    pub fn push_warning(&mut self, level: WarningLevel, code: u16, message: impl Into<String>) {
        self.warning_stack.push(SessionWarning {
            level,
            code,
            message: message.into(),
        });
        self.warnings = self.warning_count_u16();
    }

    pub fn warning_count(&self) -> usize {
        self.warning_stack.len()
    }

    pub fn warning_count_u16(&self) -> u16 {
        self.warning_count().min(u16::MAX as usize) as u16
    }

    pub fn error_count(&self) -> usize {
        self.warning_stack
            .iter()
            .filter(|warning| warning.level == WarningLevel::Error)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn fingerprint_hash(fingerprint: &TranslationFingerprint) -> u64 {
        let mut hasher = DefaultHasher::new();
        fingerprint.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn session_state_new_uses_expected_defaults() {
        let state = SessionState::new(42);

        assert_eq!(state.connection_id, 42);
        assert_eq!(state.database, None);
        let profile = MysqlCompatibilityProfile::default_mysql8();
        assert_eq!(state.charset_id, profile.default_collation_info().id);
        assert_eq!(state.collation, profile.default_collation);
        assert_eq!(state.timezone, profile.default_time_zone);
        assert_eq!(state.sql_mode, profile.default_sql_mode);
        assert_eq!(
            state.transaction_isolation,
            profile.default_transaction_isolation
        );
        assert!(state.autocommit);
        assert_eq!(state.transaction_state, TransactionState::Idle);
        assert_eq!(state.client_capabilities, 0);
        assert_eq!(state.last_insert_id, 0);
        assert_eq!(state.affected_rows, 0);
        assert_eq!(state.row_count, 0);
        assert_eq!(state.warnings, 0);
        assert_eq!(state.found_rows, 0);
        assert!(state.warning_stack.is_empty());
        assert!(state.system_variables.is_empty());
    }

    #[test]
    fn translation_fingerprint_is_stable_for_identical_state() {
        let mut left = SessionState::new(7);
        left.database = Some("analytics".to_owned());
        left.charset_id = 224;
        left.collation = "utf8mb4_unicode_ci".to_owned();
        left.timezone = "+00:00".to_owned();
        left.sql_mode = "ANSI_QUOTES,STRICT_TRANS_TABLES".to_owned();
        left.transaction_isolation = "READ-COMMITTED".to_owned();

        let right = left.clone();

        let left_fingerprint = left.translation_fingerprint();
        let right_fingerprint = right.translation_fingerprint();

        assert_eq!(left_fingerprint, right_fingerprint);
        assert_eq!(
            fingerprint_hash(&left_fingerprint),
            fingerprint_hash(&right_fingerprint)
        );
    }

    #[test]
    fn translation_fingerprint_includes_rewritten_charset_variables() {
        let mut left = SessionState::new(7);
        let mut right = left.clone();

        left.system_variables.insert(
            "character_set_results".to_owned(),
            Value::String("utf8mb4".into()),
        );
        right.system_variables.insert(
            "character_set_results".to_owned(),
            Value::String("binary".into()),
        );

        let left_fingerprint = left.translation_fingerprint();
        let right_fingerprint = right.translation_fingerprint();

        assert_ne!(left_fingerprint, right_fingerprint);
        assert_ne!(
            fingerprint_hash(&left_fingerprint),
            fingerprint_hash(&right_fingerprint)
        );
    }

    #[test]
    fn warning_stack_tracks_counts() {
        let mut state = SessionState::new(7);
        state.push_warning(WarningLevel::Warning, 1287, "deprecated");
        state.push_warning(WarningLevel::Error, 1064, "syntax error");

        assert_eq!(state.warning_count(), 2);
        assert_eq!(state.warning_count_u16(), 2);
        assert_eq!(state.error_count(), 1);
        assert_eq!(state.warnings, 2);
        assert_eq!(state.warning_stack[0].level.as_str(), "Warning");
        assert_eq!(state.warning_stack[1].level.as_str(), "Error");

        state.clear_warning_stack();
        assert_eq!(state.warning_count(), 0);
        assert_eq!(state.error_count(), 0);
        assert_eq!(state.warnings, 0);
        assert!(state.warning_stack.is_empty());
    }
}
