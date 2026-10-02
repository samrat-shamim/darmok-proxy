use darmok_types::value::Value;
use thiserror::Error;

use crate::transaction::TransactionSettingsError;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum SessionVariableError {
    #[error("unknown canonical variable '{0}'")]
    UnknownName(String),
    #[error("scoped SQL references require semantic classification")]
    ScopedReference,
    #[error("variable '{0}' is read-only")]
    ReadOnly(SessionVariable),
    #[error("value for variable '{0}' is not implemented")]
    ValueNotImplemented(SessionVariable),
    #[error("setting variable '{0}' is not implemented")]
    SettingNotImplemented(SessionVariable),
    #[error(transparent)]
    TransactionOutcome(#[from] TransactionSettingsError),
}

/// Canonical names, not SQL expressions. This registry deliberately does not
/// erase @@, GLOBAL/SESSION, or assignment-form information. A SQL classifier
/// must preserve that information before selecting a value or command API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionVariable {
    Autocommit,
    CompletionType,
    TransactionIsolation,
    TransactionReadOnly,
    SqlMode,
    TimeZone,
    DefaultStorageEngine,
    ForeignKeyChecks,
    CharacterSetClient,
    CharacterSetConnection,
    CharacterSetDatabase,
    CharacterSetFilesystem,
    CharacterSetResults,
    CharacterSetServer,
    CharacterSetSystem,
    CollationConnection,
    CollationDatabase,
    CollationServer,
    SessionTrackGtids,
    SessionTrackSchema,
    SessionTrackStateChange,
    SessionTrackSystemVariables,
    SessionTrackTransactionInfo,
    Version,
    VersionComment,
    VersionCompileOs,
    MaxAllowedPacket,
    AutoIncrementIncrement,
    WarningCount,
    ErrorCount,
    WaitTimeout,
    InteractiveTimeout,
}

/// The only implemented mutation entry points. These are internal model APIs;
/// selecting a path does not classify SQL scopes, coerce values, acknowledge
/// output, implement backend effects, or approve a statement for execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableWritePath {
    SqlModes,
    TransactionCommand,
}

impl SessionVariable {
    pub const ALL: [Self; 32] = [
        Self::Autocommit,
        Self::CompletionType,
        Self::TransactionIsolation,
        Self::TransactionReadOnly,
        Self::SqlMode,
        Self::TimeZone,
        Self::DefaultStorageEngine,
        Self::ForeignKeyChecks,
        Self::CharacterSetClient,
        Self::CharacterSetConnection,
        Self::CharacterSetDatabase,
        Self::CharacterSetFilesystem,
        Self::CharacterSetResults,
        Self::CharacterSetServer,
        Self::CharacterSetSystem,
        Self::CollationConnection,
        Self::CollationDatabase,
        Self::CollationServer,
        Self::SessionTrackGtids,
        Self::SessionTrackSchema,
        Self::SessionTrackStateChange,
        Self::SessionTrackSystemVariables,
        Self::SessionTrackTransactionInfo,
        Self::Version,
        Self::VersionComment,
        Self::VersionCompileOs,
        Self::MaxAllowedPacket,
        Self::AutoIncrementIncrement,
        Self::WarningCount,
        Self::ErrorCount,
        Self::WaitTimeout,
        Self::InteractiveTimeout,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Autocommit => "autocommit",
            Self::CompletionType => "completion_type",
            Self::TransactionIsolation => "transaction_isolation",
            Self::TransactionReadOnly => "transaction_read_only",
            Self::SqlMode => "sql_mode",
            Self::TimeZone => "time_zone",
            Self::DefaultStorageEngine => "default_storage_engine",
            Self::ForeignKeyChecks => "foreign_key_checks",
            Self::CharacterSetClient => "character_set_client",
            Self::CharacterSetConnection => "character_set_connection",
            Self::CharacterSetDatabase => "character_set_database",
            Self::CharacterSetFilesystem => "character_set_filesystem",
            Self::CharacterSetResults => "character_set_results",
            Self::CharacterSetServer => "character_set_server",
            Self::CharacterSetSystem => "character_set_system",
            Self::CollationConnection => "collation_connection",
            Self::CollationDatabase => "collation_database",
            Self::CollationServer => "collation_server",
            Self::SessionTrackGtids => "session_track_gtids",
            Self::SessionTrackSchema => "session_track_schema",
            Self::SessionTrackStateChange => "session_track_state_change",
            Self::SessionTrackSystemVariables => "session_track_system_variables",
            Self::SessionTrackTransactionInfo => "session_track_transaction_info",
            Self::Version => "version",
            Self::VersionComment => "version_comment",
            Self::VersionCompileOs => "version_compile_os",
            Self::MaxAllowedPacket => "max_allowed_packet",
            Self::AutoIncrementIncrement => "auto_increment_increment",
            Self::WarningCount => "warning_count",
            Self::ErrorCount => "error_count",
            Self::WaitTimeout => "wait_timeout",
            Self::InteractiveTimeout => "interactive_timeout",
        }
    }

    pub fn from_canonical_name(name: &str) -> Result<Self, SessionVariableError> {
        let name = name.trim();
        if name.starts_with('@') || name.contains('.') {
            return Err(SessionVariableError::ScopedReference);
        }
        Self::ALL
            .into_iter()
            .find(|variable| variable.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| SessionVariableError::UnknownName(name.to_ascii_lowercase()))
    }

    pub fn write_path(self) -> Result<VariableWritePath, SessionVariableError> {
        match self {
            Self::SqlMode => Ok(VariableWritePath::SqlModes),
            Self::Autocommit
            | Self::CompletionType
            | Self::TransactionIsolation
            | Self::TransactionReadOnly => Ok(VariableWritePath::TransactionCommand),
            Self::Version
            | Self::VersionComment
            | Self::VersionCompileOs
            | Self::WarningCount
            | Self::ErrorCount
            | Self::CharacterSetSystem => Err(SessionVariableError::ReadOnly(self)),
            _ => Err(SessionVariableError::SettingNotImplemented(self)),
        }
    }
}

impl std::fmt::Display for SessionVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Reads derive from one authority; there is no generic value insertion API.
/// Unimplemented values are explicit errors, not absent map entries or guessed
/// defaults. Successful reads are model values, not SQL scope/wire evidence.
pub trait SessionVariableReader {
    fn read_variable(&self, variable: SessionVariable) -> Result<Value, SessionVariableError>;

    fn get_system_var(&self, name: &str) -> Result<Value, SessionVariableError> {
        self.read_variable(SessionVariable::from_canonical_name(name)?)
    }
}
