use thiserror::Error;

use crate::mysql_const;
use crate::pg_const;

const ER_QUERY_INTERRUPTED: u16 = 1317;
const SQLSTATE_QUERY_INTERRUPTED: [u8; 5] = *b"70100";
const PG_QUERY_CANCELED_SQLSTATE: &str = "57014";

/// The unified error type for the proxy.
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct ProxyError {
    pub kind: ErrorKind,
    pub fatal: bool,
    pub mysql_error_code: u16,
    pub sqlstate: [u8; 5],
}

/// Classifies the error's origin.
#[derive(Debug, Error)]
pub enum ErrorKind {
    #[error("{0}")]
    Auth(String),
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("translation error: {0}")]
    Translation(#[from] TranslationError),
    #[error("execution error: {0}")]
    Execution(#[from] ExecutionError),
    #[error("middleware error: {0}")]
    Middleware(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("client disconnected")]
    ClientDisconnect,
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("invalid packet: {0}")]
    InvalidPacket(String),
    #[error("auth failure: {0}")]
    AuthFailure(String),
    #[error("unsupported capability: {0}")]
    UnsupportedCapability(String),
    #[error("connection closed")]
    ConnectionClosed,
    #[error("packet too large: {size} bytes")]
    PacketTooLarge { size: usize },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum TranslationError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("unsupported syntax [{syntax_id}]: {message}")]
    UnsupportedSyntax {
        syntax_id: &'static str,
        message: String,
    },
    #[error("rewrite error: {0}")]
    Rewrite(String),
    #[error("emit error: {0}")]
    Emit(String),
}

impl TranslationError {
    pub fn unsupported_syntax(
        syntax_id: &'static str,
        message: impl Into<String>,
    ) -> TranslationError {
        TranslationError::UnsupportedSyntax {
            syntax_id,
            message: message.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("pg error: {0}")]
    Postgres(String),
    #[error("type coercion error: {0}")]
    TypeCoercion(String),
    #[error("pool exhausted")]
    PoolExhausted,
    #[error("E_RESULT_SET_LIMIT_EXCEEDED: {0}")]
    ResultSetLimitExceeded(String),
    #[error("unknown system variable '{0}'")]
    UnknownSystemVariable(String),
    #[error("variable '{variable}' can't be set to the value of '{value}': {message}")]
    InvalidSystemVariableValue {
        variable: String,
        value: String,
        message: String,
    },
    #[error("transaction error: {0}")]
    Transaction(String),
}

impl ProxyError {
    fn execution_with_metadata(
        err: impl Into<ExecutionError>,
        mysql_error_code: u16,
        sqlstate: [u8; 5],
        fatal: bool,
    ) -> Self {
        Self {
            kind: ErrorKind::Execution(err.into()),
            fatal,
            mysql_error_code,
            sqlstate,
        }
    }

    /// Create a protocol-level error.
    pub fn protocol(err: impl Into<ProtocolError>) -> Self {
        Self {
            kind: ErrorKind::Protocol(err.into()),
            fatal: false,
            mysql_error_code: mysql_const::error_code::ER_UNKNOWN_ERROR,
            sqlstate: mysql_const::sqlstate::HY000,
        }
    }

    /// Create an authentication / authorization error with MySQL access-denied metadata.
    pub fn auth_denied(message: impl Into<String>, fatal: bool) -> Self {
        Self {
            kind: ErrorKind::Auth(message.into()),
            fatal,
            mysql_error_code: mysql_const::error_code::ER_ACCESS_DENIED,
            sqlstate: mysql_const::sqlstate::AUTH_ERROR,
        }
    }

    /// Create a SQL translation error.
    pub fn translation(err: impl Into<TranslationError>) -> Self {
        let err = err.into();
        let (mysql_error_code, sqlstate) = match &err {
            TranslationError::UnsupportedSyntax { .. } => (
                mysql_const::error_code::ER_NOT_SUPPORTED_YET,
                mysql_const::sqlstate::SYNTAX_ERROR,
            ),
            _ => (
                mysql_const::error_code::ER_SYNTAX_ERROR,
                mysql_const::sqlstate::SYNTAX_ERROR,
            ),
        };

        Self {
            kind: ErrorKind::Translation(err),
            fatal: false,
            mysql_error_code,
            sqlstate,
        }
    }

    /// Create an execution error (from the PostgreSQL backend).
    pub fn execution(err: impl Into<ExecutionError>) -> Self {
        let err = err.into();
        let (mysql_error_code, sqlstate, fatal) = match &err {
            ExecutionError::PoolExhausted => (
                mysql_const::error_code::ER_CON_COUNT_ERROR,
                mysql_const::sqlstate::TOO_MANY_CONNECTIONS,
                false,
            ),
            ExecutionError::ResultSetLimitExceeded(_) => (
                mysql_const::error_code::ER_UNKNOWN_ERROR,
                mysql_const::sqlstate::HY000,
                false,
            ),
            ExecutionError::UnknownSystemVariable(_) => (
                mysql_const::error_code::ER_UNKNOWN_SYSTEM_VARIABLE,
                mysql_const::sqlstate::HY000,
                false,
            ),
            ExecutionError::InvalidSystemVariableValue { .. } => (
                mysql_const::error_code::ER_WRONG_VALUE_FOR_VAR,
                mysql_const::sqlstate::SYNTAX_ERROR,
                false,
            ),
            _ => (
                mysql_const::error_code::ER_UNKNOWN_ERROR,
                mysql_const::sqlstate::HY000,
                false,
            ),
        };
        Self::execution_with_metadata(err, mysql_error_code, sqlstate, fatal)
    }

    /// Create an execution error with explicit MySQL wire metadata.
    pub fn execution_with_mysql_metadata(
        err: impl Into<ExecutionError>,
        mysql_error_code: u16,
        sqlstate: [u8; 5],
        fatal: bool,
    ) -> Self {
        Self::execution_with_metadata(err, mysql_error_code, sqlstate, fatal)
    }

    /// Create an internal / unexpected error.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal(msg.into()),
            fatal: true,
            mysql_error_code: mysql_const::error_code::ER_UNKNOWN_ERROR,
            sqlstate: mysql_const::sqlstate::HY000,
        }
    }

    /// Create a middleware error.
    pub fn middleware(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Middleware(msg.into()),
            fatal: false,
            mysql_error_code: mysql_const::error_code::ER_UNKNOWN_ERROR,
            sqlstate: mysql_const::sqlstate::HY000,
        }
    }

    /// Create a config error.
    pub fn config(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Config(msg.into()),
            fatal: false,
            mysql_error_code: mysql_const::error_code::ER_UNKNOWN_ERROR,
            sqlstate: mysql_const::sqlstate::HY000,
        }
    }

    /// Create a client-disconnect error (always fatal for that session).
    pub fn client_disconnect() -> Self {
        Self {
            kind: ErrorKind::ClientDisconnect,
            fatal: true,
            mysql_error_code: mysql_const::error_code::ER_UNKNOWN_ERROR,
            sqlstate: mysql_const::sqlstate::HY000,
        }
    }

    pub fn is_pg_query_canceled(code: &str) -> bool {
        code == PG_QUERY_CANCELED_SQLSTATE
    }

    pub fn query_interrupted(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Execution(ExecutionError::Postgres(message.into())),
            fatal: false,
            mysql_error_code: ER_QUERY_INTERRUPTED,
            sqlstate: SQLSTATE_QUERY_INTERRUPTED,
        }
    }

    pub fn aborted_transaction(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Execution(ExecutionError::Transaction(message.into())),
            fatal: false,
            mysql_error_code: mysql_const::error_code::ER_UNKNOWN_ERROR,
            sqlstate: mysql_const::sqlstate::INVALID_TRANSACTION_STATE,
        }
    }

    /// Map a PostgreSQL SQLSTATE code to the appropriate MySQL error code and
    /// construct a typed execution error.
    pub fn from_pg_error(code: &str, message: String) -> Self {
        if Self::is_pg_query_canceled(code) {
            return Self::query_interrupted(message);
        }

        let code_bytes: [u8; 5] = code
            .as_bytes()
            .try_into()
            .unwrap_or(mysql_const::sqlstate::HY000);

        if code_bytes == pg_const::sqlstate::IN_FAILED_SQL_TRANSACTION {
            return Self::aborted_transaction(
                "transaction is aborted and needs ROLLBACK before more statements can execute",
            );
        }

        let (mysql_code, fatal) = match code_bytes {
            pg_const::sqlstate::UNIQUE_VIOLATION => (mysql_const::error_code::ER_DUP_ENTRY, false),
            pg_const::sqlstate::FOREIGN_KEY_VIOLATION => {
                (mysql_const::error_code::ER_NO_REFERENCED_ROW, false)
            }
            pg_const::sqlstate::NOT_NULL_VIOLATION => {
                (mysql_const::error_code::ER_BAD_NULL_ERROR, false)
            }
            pg_const::sqlstate::UNDEFINED_TABLE => {
                (mysql_const::error_code::ER_NO_SUCH_TABLE, false)
            }
            pg_const::sqlstate::UNDEFINED_COLUMN => {
                (mysql_const::error_code::ER_BAD_FIELD_ERROR, false)
            }
            pg_const::sqlstate::SYNTAX_ERROR => (mysql_const::error_code::ER_SYNTAX_ERROR, false),
            pg_const::sqlstate::INSUFFICIENT_PRIVILEGE => {
                (mysql_const::error_code::ER_ACCESS_DENIED, false)
            }
            pg_const::sqlstate::DEADLOCK_DETECTED => {
                (mysql_const::error_code::ER_LOCK_DEADLOCK, false)
            }
            pg_const::sqlstate::SERIALIZATION_FAILURE => {
                (mysql_const::error_code::ER_LOCK_DEADLOCK, false)
            }
            pg_const::sqlstate::TOO_MANY_CONNECTIONS => {
                (mysql_const::error_code::ER_TOO_MANY_CONNECTIONS, true)
            }
            pg_const::sqlstate::DISK_FULL => (mysql_const::error_code::ER_DISK_FULL, true),
            pg_const::sqlstate::DUPLICATE_TABLE => {
                (mysql_const::error_code::ER_TABLE_EXISTS, false)
            }
            _ => (mysql_const::error_code::ER_UNKNOWN_ERROR, false),
        };

        let sqlstate = match code_bytes {
            pg_const::sqlstate::UNIQUE_VIOLATION => *b"23000",
            pg_const::sqlstate::UNDEFINED_TABLE => mysql_const::sqlstate::NO_SUCH_TABLE,
            pg_const::sqlstate::SYNTAX_ERROR => mysql_const::sqlstate::SYNTAX_ERROR,
            pg_const::sqlstate::INSUFFICIENT_PRIVILEGE => mysql_const::sqlstate::AUTH_ERROR,
            pg_const::sqlstate::UNDEFINED_COLUMN => *b"42S22",
            pg_const::sqlstate::FOREIGN_KEY_VIOLATION => *b"23000",
            pg_const::sqlstate::NOT_NULL_VIOLATION => *b"23000",
            pg_const::sqlstate::SERIALIZATION_FAILURE => *b"40001",
            pg_const::sqlstate::DEADLOCK_DETECTED => *b"40001",
            pg_const::sqlstate::DUPLICATE_TABLE => *b"42S01",
            pg_const::sqlstate::TOO_MANY_CONNECTIONS => *b"08004",
            _ => mysql_const::sqlstate::HY000,
        };

        Self {
            kind: ErrorKind::Execution(ExecutionError::Postgres(message)),
            fatal,
            mysql_error_code: mysql_code,
            sqlstate,
        }
    }
}

impl From<std::io::Error> for ProxyError {
    fn from(err: std::io::Error) -> Self {
        Self {
            kind: ErrorKind::Protocol(ProtocolError::Io(err)),
            fatal: false,
            mysql_error_code: mysql_const::error_code::ER_UNKNOWN_ERROR,
            sqlstate: mysql_const::sqlstate::HY000,
        }
    }
}

/// Convenience alias used throughout the proxy.
pub type Result<T> = std::result::Result<T, ProxyError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_pg_error_maps_sqlstates_to_mysql_equivalents() {
        let cases = [
            (
                pg_const::sqlstate::UNIQUE_VIOLATION,
                mysql_const::error_code::ER_DUP_ENTRY,
                *b"23000",
                false,
            ),
            (
                pg_const::sqlstate::UNDEFINED_TABLE,
                mysql_const::error_code::ER_NO_SUCH_TABLE,
                mysql_const::sqlstate::NO_SUCH_TABLE,
                false,
            ),
            (
                pg_const::sqlstate::UNDEFINED_COLUMN,
                mysql_const::error_code::ER_BAD_FIELD_ERROR,
                *b"42S22",
                false,
            ),
            (
                pg_const::sqlstate::SYNTAX_ERROR,
                mysql_const::error_code::ER_SYNTAX_ERROR,
                mysql_const::sqlstate::SYNTAX_ERROR,
                false,
            ),
            (
                pg_const::sqlstate::INSUFFICIENT_PRIVILEGE,
                mysql_const::error_code::ER_ACCESS_DENIED,
                mysql_const::sqlstate::AUTH_ERROR,
                false,
            ),
            (
                pg_const::sqlstate::FOREIGN_KEY_VIOLATION,
                mysql_const::error_code::ER_NO_REFERENCED_ROW,
                *b"23000",
                false,
            ),
            (
                pg_const::sqlstate::NOT_NULL_VIOLATION,
                mysql_const::error_code::ER_BAD_NULL_ERROR,
                *b"23000",
                false,
            ),
            (
                pg_const::sqlstate::SERIALIZATION_FAILURE,
                mysql_const::error_code::ER_LOCK_DEADLOCK,
                *b"40001",
                false,
            ),
            (
                pg_const::sqlstate::DEADLOCK_DETECTED,
                mysql_const::error_code::ER_LOCK_DEADLOCK,
                *b"40001",
                false,
            ),
            (
                pg_const::sqlstate::DUPLICATE_TABLE,
                mysql_const::error_code::ER_TABLE_EXISTS,
                *b"42S01",
                false,
            ),
            (
                pg_const::sqlstate::TOO_MANY_CONNECTIONS,
                mysql_const::error_code::ER_TOO_MANY_CONNECTIONS,
                *b"08004",
                true,
            ),
            (
                pg_const::sqlstate::DISK_FULL,
                mysql_const::error_code::ER_DISK_FULL,
                mysql_const::sqlstate::HY000,
                true,
            ),
            (
                *b"57014",
                ER_QUERY_INTERRUPTED,
                SQLSTATE_QUERY_INTERRUPTED,
                false,
            ),
        ];

        for (pg_sqlstate, expected_code, expected_sqlstate, expected_fatal) in cases {
            let error = ProxyError::from_pg_error(
                std::str::from_utf8(&pg_sqlstate).expect("SQLSTATEs are ASCII"),
                "backend error".to_string(),
            );

            assert_eq!(error.mysql_error_code, expected_code);
            assert_eq!(error.sqlstate, expected_sqlstate);
            assert_eq!(error.fatal, expected_fatal);
            assert!(matches!(
                error.kind,
                ErrorKind::Execution(ExecutionError::Postgres(ref message))
                    if message == "backend error"
            ));
        }
    }

    #[test]
    fn convenience_constructors_set_expected_fields() {
        let protocol = ProxyError::protocol(ProtocolError::InvalidPacket("bad packet".into()));
        match protocol.kind {
            ErrorKind::Protocol(ProtocolError::InvalidPacket(message)) => {
                assert_eq!(message, "bad packet");
            }
            other => panic!("expected invalid-packet protocol error, got {other:?}"),
        }
        assert!(!protocol.fatal);
        assert_eq!(
            protocol.mysql_error_code,
            mysql_const::error_code::ER_UNKNOWN_ERROR
        );
        assert_eq!(protocol.sqlstate, mysql_const::sqlstate::HY000);

        let auth = ProxyError::auth_denied("Access denied (no authorized database)", true);
        match auth.kind {
            ErrorKind::Auth(message) => {
                assert_eq!(message, "Access denied (no authorized database)");
            }
            other => panic!("expected auth error, got {other:?}"),
        }
        assert!(auth.fatal);
        assert_eq!(
            auth.mysql_error_code,
            mysql_const::error_code::ER_ACCESS_DENIED
        );
        assert_eq!(auth.sqlstate, mysql_const::sqlstate::AUTH_ERROR);

        let translation = ProxyError::translation(TranslationError::Parse("bad sql".into()));
        match translation.kind {
            ErrorKind::Translation(TranslationError::Parse(message)) => {
                assert_eq!(message, "bad sql");
            }
            other => panic!("expected parse translation error, got {other:?}"),
        }
        assert!(!translation.fatal);
        assert_eq!(
            translation.mysql_error_code,
            mysql_const::error_code::ER_SYNTAX_ERROR
        );
        assert_eq!(translation.sqlstate, mysql_const::sqlstate::SYNTAX_ERROR);

        let unsupported = ProxyError::translation(TranslationError::unsupported_syntax(
            "mysql.function.json_table",
            "json_table",
        ));
        match unsupported.kind {
            ErrorKind::Translation(TranslationError::UnsupportedSyntax { syntax_id, message }) => {
                assert_eq!(syntax_id, "mysql.function.json_table");
                assert_eq!(message, "json_table");
            }
            other => panic!("expected unsupported-syntax translation error, got {other:?}"),
        }
        assert!(!unsupported.fatal);
        assert_eq!(
            unsupported.mysql_error_code,
            mysql_const::error_code::ER_NOT_SUPPORTED_YET
        );
        assert_eq!(unsupported.sqlstate, mysql_const::sqlstate::SYNTAX_ERROR);

        let execution = ProxyError::execution(ExecutionError::Transaction("rollback".into()));
        match execution.kind {
            ErrorKind::Execution(ExecutionError::Transaction(message)) => {
                assert_eq!(message, "rollback");
            }
            other => panic!("expected execution transaction error, got {other:?}"),
        }
        assert!(!execution.fatal);
        assert_eq!(
            execution.mysql_error_code,
            mysql_const::error_code::ER_UNKNOWN_ERROR
        );
        assert_eq!(execution.sqlstate, mysql_const::sqlstate::HY000);

        let pool_exhausted = ProxyError::execution(ExecutionError::PoolExhausted);
        assert!(!pool_exhausted.fatal);
        assert_eq!(
            pool_exhausted.mysql_error_code,
            mysql_const::error_code::ER_CON_COUNT_ERROR
        );
        assert_eq!(
            pool_exhausted.sqlstate,
            mysql_const::sqlstate::TOO_MANY_CONNECTIONS
        );

        let middleware = ProxyError::middleware("auth middleware");
        match middleware.kind {
            ErrorKind::Middleware(message) => assert_eq!(message, "auth middleware"),
            other => panic!("expected middleware error, got {other:?}"),
        }
        assert!(!middleware.fatal);
        assert_eq!(
            middleware.mysql_error_code,
            mysql_const::error_code::ER_UNKNOWN_ERROR
        );
        assert_eq!(middleware.sqlstate, mysql_const::sqlstate::HY000);

        let config = ProxyError::config("missing url");
        match config.kind {
            ErrorKind::Config(message) => assert_eq!(message, "missing url"),
            other => panic!("expected config error, got {other:?}"),
        }
        assert!(!config.fatal);
        assert_eq!(
            config.mysql_error_code,
            mysql_const::error_code::ER_UNKNOWN_ERROR
        );
        assert_eq!(config.sqlstate, mysql_const::sqlstate::HY000);

        let disconnect = ProxyError::client_disconnect();
        assert!(matches!(disconnect.kind, ErrorKind::ClientDisconnect));
        assert!(disconnect.fatal);
        assert_eq!(
            disconnect.mysql_error_code,
            mysql_const::error_code::ER_UNKNOWN_ERROR
        );
        assert_eq!(disconnect.sqlstate, mysql_const::sqlstate::HY000);

        let interrupted = ProxyError::query_interrupted("cancelled by backend");
        match interrupted.kind {
            ErrorKind::Execution(ExecutionError::Postgres(message)) => {
                assert_eq!(message, "cancelled by backend");
            }
            other => panic!("expected query-interrupted execution error, got {other:?}"),
        }
        assert!(!interrupted.fatal);
        assert_eq!(interrupted.mysql_error_code, ER_QUERY_INTERRUPTED);
        assert_eq!(interrupted.sqlstate, SQLSTATE_QUERY_INTERRUPTED);
        assert!(ProxyError::is_pg_query_canceled("57014"));
        assert!(!ProxyError::is_pg_query_canceled("23505"));
    }

    #[test]
    fn failed_sql_transaction_maps_to_typed_transaction_error() {
        let error = ProxyError::from_pg_error(
            "25P02",
            "current transaction is aborted, commands ignored until end of transaction block"
                .to_owned(),
        );

        assert_eq!(
            error.mysql_error_code,
            mysql_const::error_code::ER_UNKNOWN_ERROR
        );
        assert_eq!(
            error.sqlstate,
            mysql_const::sqlstate::INVALID_TRANSACTION_STATE
        );
        assert!(!error.fatal);
        assert!(matches!(
            error.kind,
            ErrorKind::Execution(ExecutionError::Transaction(ref message))
                if message == "transaction is aborted and needs ROLLBACK before more statements can execute"
        ));
    }
}
