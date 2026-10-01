use std::collections::HashMap;

use darmok_types::value::Value;

use crate::compatibility::MysqlCompatibilityProfile;
use crate::transaction::{
    AutocommitSetting, FrontendTransactionAccess, FrontendTransactionCommand,
    TransactionCommandStage, TransactionSettings, TransactionSettingsError,
    TransactionSettingsSnapshot, UnconfirmedTransactionCommand,
};
use crate::variables::{
    SessionVariableError, SessionVariableStore, canonical_name, is_transaction_variable,
};

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
    pub transactions: TransactionSettingsSnapshot,
}

/// Per-connection session state that mirrors what a MySQL client expects.
#[derive(Debug, PartialEq)]
pub struct SessionState {
    pub connection_id: u32,
    pub database: Option<String>,
    /// MySQL charset collation ID from the compatibility profile.
    pub charset_id: u16,
    /// MySQL collation name from the compatibility profile.
    pub collation: String,
    pub timezone: String,
    pub sql_mode: String,
    transactions: TransactionSettings,
    pub client_capabilities: u32,
    pub last_insert_id: u64,
    pub affected_rows: u64,
    pub row_count: i64,
    pub warnings: u16,
    pub found_rows: u64,
    pub warning_stack: Vec<SessionWarning>,
    /// Canonical nontransaction variable values only. Transaction variables
    /// cannot be independently inserted or changed through the generic store.
    system_variables: HashMap<String, Value>,
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
            transactions: TransactionSettings::new(profile.default_transaction_characteristics),
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
    pub fn translation_fingerprint(&self) -> Result<TranslationFingerprint, SessionVariableError> {
        let transactions = self.transactions.snapshot()?;
        let profile = MysqlCompatibilityProfile::default_mysql8();
        Ok(TranslationFingerprint {
            database: self.database.clone(),
            sql_mode: self.sql_mode.clone(),
            charset_id: self.charset_id,
            collation: self.collation.clone(),
            character_set_client: self.system_variable_string_or_default(
                "character_set_client",
                profile.default_charset,
            )?,
            character_set_connection: self.system_variable_string_or_default(
                "character_set_connection",
                profile.default_charset,
            )?,
            character_set_results: self.system_variable_string_or_default(
                "character_set_results",
                profile.default_charset,
            )?,
            timezone: self.timezone.clone(),
            transactions,
        })
    }

    fn system_variable_string_or_default(
        &self,
        name: &str,
        default: &str,
    ) -> Result<String, SessionVariableError> {
        match self.system_variables.get(name) {
            Some(Value::String(value)) => Ok(value.to_string()),
            None => Ok(default.to_owned()),
            Some(_) => Err(SessionVariableError::NonStringTranslationValue(
                name.to_owned(),
            )),
        }
    }

    pub fn transaction_settings(
        &self,
    ) -> Result<TransactionSettingsSnapshot, TransactionSettingsError> {
        self.transactions.snapshot()
    }

    pub fn stage_transaction_command(
        &mut self,
        command: FrontendTransactionCommand,
    ) -> Result<TransactionCommandStage<'_>, TransactionSettingsError> {
        self.transactions.stage(command)
    }

    pub fn unconfirmed_transaction_command(&self) -> Option<UnconfirmedTransactionCommand> {
        self.transactions.unconfirmed()
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

impl SessionVariableStore for SessionState {
    fn get_system_var(&self, name: &str) -> Result<Option<Value>, SessionVariableError> {
        let canonical = canonical_name(name)?;
        let state = self.transactions.snapshot()?;
        if is_transaction_variable(&canonical) {
            return Ok(Some(match canonical.as_str() {
                "transaction_isolation" => {
                    Value::String(state.defaults.isolation.variable_label().into())
                }
                "transaction_read_only" => Value::UInt(u64::from(
                    state.defaults.access == FrontendTransactionAccess::ReadOnly,
                )),
                "autocommit" => {
                    Value::UInt(u64::from(state.autocommit == AutocommitSetting::Enabled))
                }
                _ => unreachable!("canonical transaction variable checked above"),
            }));
        }
        Ok(self.system_variables.get(&canonical).cloned())
    }

    fn set_system_var(&mut self, name: &str, value: Value) -> Result<(), SessionVariableError> {
        let canonical = canonical_name(name)?;
        self.transactions.snapshot()?;
        if is_transaction_variable(&canonical) {
            return Err(SessionVariableError::TransactionCommandRequired);
        }
        self.system_variables.insert(canonical, value);
        Ok(())
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
        let transactions = state.transaction_settings().unwrap();
        assert_eq!(
            transactions.defaults,
            profile.default_transaction_characteristics
        );
        assert_eq!(transactions.autocommit, AutocommitSetting::Enabled);
        assert_eq!(transactions.active, None);
        assert_eq!(
            transactions.next,
            crate::transaction::NextTransactionCharacteristics::default()
        );
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
        let mut right = SessionState::new(7);
        right.database = left.database.clone();
        right.charset_id = left.charset_id;
        right.collation = left.collation.clone();
        right.timezone = left.timezone.clone();
        right.sql_mode = left.sql_mode.clone();

        let left_fingerprint = left.translation_fingerprint().unwrap();
        let right_fingerprint = right.translation_fingerprint().unwrap();

        assert_eq!(left_fingerprint, right_fingerprint);
        assert_eq!(
            fingerprint_hash(&left_fingerprint),
            fingerprint_hash(&right_fingerprint)
        );
    }

    #[test]
    fn translation_fingerprint_includes_rewritten_charset_variables() {
        let mut left = SessionState::new(7);
        let mut right = SessionState::new(7);

        left.set_system_var("character_set_results", Value::String("utf8mb4".into()))
            .unwrap();
        right
            .set_system_var("character_set_results", Value::String("binary".into()))
            .unwrap();

        let left_fingerprint = left.translation_fingerprint().unwrap();
        let right_fingerprint = right.translation_fingerprint().unwrap();

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
