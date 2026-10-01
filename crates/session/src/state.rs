use darmok_types::value::Value;

use crate::compatibility::{CharsetInfo, MysqlCompatibilityProfile};
use crate::sql_mode::SqlModes;
use crate::transaction::{
    AutocommitSetting, FrontendTransactionAccess, FrontendTransactionCommand,
    TransactionCommandStage, TransactionSettings, TransactionSettingsError,
    TransactionSettingsSnapshot, UnconfirmedTransactionCommand,
};
use crate::variables::{SessionVariable, SessionVariableError, SessionVariableReader};

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
    pub sql_modes: SqlModes,
    pub charset_id: u16,
    pub collation: String,
    pub character_set_client: String,
    pub character_set_connection: String,
    pub character_set_results: String,
    pub timezone: String,
    pub transactions: TransactionSettingsSnapshot,
}

/// Per-connection session state that mirrors what a MySQL client expects.
/// Canonical variable state cannot be overwritten through an arbitrary map.
///
/// ```compile_fail
/// use darmok_session::*;
/// use darmok_types::value::Value;
/// let mut state = SessionState::new(1);
/// state.set_system_var("version", Value::String("other".into()));
/// ```
///
/// ```compile_fail
/// use darmok_session::SessionState;
/// let mut state = SessionState::new(1);
/// state.sql_mode = "ANSI_QUOTES".to_owned();
/// ```
///
/// ```compile_fail
/// use darmok_session::SessionState;
/// let mut state = SessionState::new(1);
/// state.warnings = 42;
/// ```
#[derive(Debug, PartialEq)]
pub struct SessionState {
    pub connection_id: u32,
    pub database: Option<String>,
    /// Immutable declared text settings until codecs/comparison semantics and
    /// their setting effects are implemented. Numeric/name identity is shared.
    text_collation: &'static CharsetInfo,
    time_zone: &'static str,
    sql_modes: SqlModes,
    transactions: TransactionSettings,
    pub client_capabilities: u32,
    pub last_insert_id: u64,
    pub affected_rows: u64,
    pub row_count: i64,
    pub found_rows: u64,
    warning_stack: Vec<SessionWarning>,
}

impl Default for SessionState {
    fn default() -> Self {
        let profile = MysqlCompatibilityProfile::default_mysql8();
        let default_collation = profile.default_collation_info();
        Self {
            connection_id: 0,
            database: None,
            text_collation: default_collation,
            time_zone: profile.default_time_zone,
            sql_modes: profile.default_sql_modes,
            transactions: TransactionSettings::new(profile.default_transaction_characteristics),
            client_capabilities: 0,
            last_insert_id: 0,
            affected_rows: 0,
            row_count: 0,
            found_rows: 0,
            warning_stack: Vec::new(),
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
        Ok(TranslationFingerprint {
            database: self.database.clone(),
            sql_modes: self.sql_modes,
            charset_id: self.text_collation.id,
            collation: self.text_collation.collation.to_owned(),
            character_set_client: self.text_collation.name.to_owned(),
            character_set_connection: self.text_collation.name.to_owned(),
            character_set_results: self.text_collation.name.to_owned(),
            timezone: self.time_zone.to_owned(),
            transactions,
        })
    }

    pub fn sql_modes(&self) -> Result<SqlModes, TransactionSettingsError> {
        self.transactions.snapshot()?;
        Ok(self.sql_modes)
    }

    /// Change the locally owned typed model value. This does not evaluate a
    /// SET expression, coerce SQL values, generate warnings, classify scope,
    /// mutate PostgreSQL, or acknowledge frontend output. An integrated caller
    /// must establish those obligations; it must not use a value update as a
    /// SQL compatibility or execution receipt.
    pub fn set_sql_modes(&mut self, modes: SqlModes) -> Result<(), TransactionSettingsError> {
        self.transactions.snapshot()?;
        self.sql_modes = modes;
        Ok(())
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
    }

    pub fn push_warning(&mut self, level: WarningLevel, code: u16, message: impl Into<String>) {
        self.warning_stack.push(SessionWarning {
            level,
            code,
            message: message.into(),
        });
    }

    pub fn warning_stack(&self) -> &[SessionWarning] {
        &self.warning_stack
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

impl SessionVariableReader for SessionState {
    fn read_variable(&self, variable: SessionVariable) -> Result<Value, SessionVariableError> {
        let state = self.transactions.snapshot()?;
        let profile = MysqlCompatibilityProfile::default_mysql8();
        use SessionVariable as Var;
        Ok(match variable {
            Var::TransactionIsolation => {
                Value::String(state.defaults.isolation.variable_label().into())
            }
            Var::TransactionReadOnly => Value::UInt(u64::from(
                state.defaults.access == FrontendTransactionAccess::ReadOnly,
            )),
            Var::Autocommit => {
                Value::UInt(u64::from(state.autocommit == AutocommitSetting::Enabled))
            }
            Var::SqlMode => Value::String(self.sql_modes.canonical_names().into()),
            Var::TimeZone => Value::String(self.time_zone.into()),
            Var::CharacterSetClient | Var::CharacterSetConnection | Var::CharacterSetResults => {
                Value::String(self.text_collation.name.into())
            }
            Var::CollationConnection => Value::String(self.text_collation.collation.into()),
            Var::Version => Value::String(profile.server_version.into()),
            Var::VersionComment => Value::String(profile.version_comment.into()),
            Var::VersionCompileOs => Value::String(profile.version_compile_os.into()),
            Var::WarningCount => Value::UInt(self.warning_count() as u64),
            Var::ErrorCount => Value::UInt(self.error_count() as u64),
            _ => return Err(SessionVariableError::ValueNotImplemented(variable)),
        })
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
        let fingerprint = state.translation_fingerprint().unwrap();
        assert_eq!(fingerprint.charset_id, profile.default_collation_info().id);
        assert_eq!(fingerprint.collation, profile.default_collation);
        assert_eq!(fingerprint.timezone, profile.default_time_zone);
        assert_eq!(state.sql_modes().unwrap(), profile.default_sql_modes);
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
        assert_eq!(state.warning_count_u16(), 0);
        assert_eq!(state.found_rows, 0);
        assert!(state.warning_stack.is_empty());
    }

    #[test]
    fn translation_fingerprint_is_stable_for_identical_state() {
        let mut left = SessionState::new(7);
        left.database = Some("analytics".to_owned());
        left.set_sql_modes(SqlModes::parse_names("ANSI_QUOTES,STRICT_TRANS_TABLES").unwrap())
            .unwrap();
        let mut right = SessionState::new(7);
        right.database = left.database.clone();
        right
            .set_sql_modes(
                SqlModes::parse_names("strict_trans_tables,ansi_quotes,ANSI_QUOTES").unwrap(),
            )
            .unwrap();

        let left_fingerprint = left.translation_fingerprint().unwrap();
        let right_fingerprint = right.translation_fingerprint().unwrap();

        assert_eq!(left_fingerprint, right_fingerprint);
        assert_eq!(
            fingerprint_hash(&left_fingerprint),
            fingerprint_hash(&right_fingerprint)
        );
    }

    #[test]
    fn validation_modes_are_not_lost_in_the_parser_projection_or_fingerprint() {
        let mut left = SessionState::new(7);
        let mut right = SessionState::new(7);

        left.set_sql_modes(SqlModes::parse_names("ANSI_QUOTES,STRICT_TRANS_TABLES").unwrap())
            .unwrap();
        right
            .set_sql_modes(SqlModes::parse_names("ANSI_QUOTES,STRICT_ALL_TABLES").unwrap())
            .unwrap();
        assert_eq!(
            left.sql_modes().unwrap().parser_flags(),
            right.sql_modes().unwrap().parser_flags()
        );

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
        assert_eq!(
            state.get_system_var("warning_count").unwrap(),
            Value::UInt(2)
        );
        assert_eq!(state.get_system_var("error_count").unwrap(), Value::UInt(1));
        assert_eq!(state.warning_stack[0].level.as_str(), "Warning");
        assert_eq!(state.warning_stack[1].level.as_str(), "Error");

        state.clear_warning_stack();
        assert_eq!(state.warning_count(), 0);
        assert_eq!(state.error_count(), 0);
        assert_eq!(
            state.get_system_var("warning_count").unwrap(),
            Value::UInt(0)
        );
        assert_eq!(state.get_system_var("error_count").unwrap(), Value::UInt(0));
        assert!(state.warning_stack.is_empty());
    }
}
