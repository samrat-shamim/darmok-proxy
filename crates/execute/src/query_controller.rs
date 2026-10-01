//! Source admission and response ownership for the selected COM_QUERY subset.
//! The parser uses the settled session's current mode on every command.

use crate::{NativeBackend, NativeBackendError, NativeBackendState, ServerSetValues, SetSqlError};
use bytes::{Bytes, BytesMut};
use darmok_protocol::{CapabilityFlags, Command, ErrPacket, OkPacket, RawPacket, StatusFlags};
use darmok_session::{
    AutocommitSetting, CommandSettingsSnapshot, FrontendTransactionAccess, SessionState, SqlMode,
    TransactionSettingsError,
};
use darmok_types::error::ProxyError;
use sqlparser::{ast::Statement, mysql_mode::parse_mysql_with_mode};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_postgres::TransactionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySqlError {
    Parse,
    UnsupportedStatement,
    Set(SetSqlError),
}

impl QuerySqlError {
    pub fn code(self) -> u16 {
        match self {
            Self::Parse => 1064,
            Self::UnsupportedStatement => 1235,
            Self::Set(error) => error.code(),
        }
    }

    pub fn sql_state(self) -> [u8; 5] {
        match self {
            Self::Parse | Self::UnsupportedStatement => *b"42000",
            Self::Set(error) => error.sql_state(),
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::Parse => "The MySQL query could not be parsed",
            Self::UnsupportedStatement => "This statement or query batch is not implemented",
            Self::Set(error) => error.message(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryOutcome {
    Success(CommandSettingsSnapshot),
    SqlError {
        error: QuerySqlError,
        settings: CommandSettingsSnapshot,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum QueryExecutionError {
    #[error("the query controller requires a decoded COM_QUERY command")]
    NotQuery,
    #[error(transparent)]
    Settings(#[from] TransactionSettingsError),
    #[error("frontend/native transaction ownership differs: {0:?}")]
    NativeState(NativeBackendState),
    #[error("query output requires protocol 4.1 without session tracking")]
    OutputContract,
    #[error(transparent)]
    Native(#[from] NativeBackendError),
    #[error(transparent)]
    Encode(#[from] ProxyError),
    #[error("query response write failed: {0}")]
    Output(#[from] std::io::Error),
}

/// Executes one selected SET statement from an original decoded query.
/// Other statements and batches return an explicit SQL error before effects.
/// A transport/native error requires caller disposal: no response or recovery
/// is invented. The caller supplies the response sequence from its wire phase.
pub async fn execute_query_command<W: AsyncWrite + Unpin>(
    state: &mut SessionState,
    backend: &mut NativeBackend,
    globals: &ServerSetValues,
    command: &Command,
    response_sequence: u8,
    output: &mut W,
) -> Result<QueryOutcome, QueryExecutionError> {
    let Command::Query(sql) = command else {
        return Err(QueryExecutionError::NotQuery);
    };
    let before = state.transaction_settings()?;
    let expected = if before.active.is_some() {
        TransactionState::Transaction
    } else {
        TransactionState::Idle
    };
    if backend.state() != NativeBackendState::Ready(expected) {
        return Err(QueryExecutionError::NativeState(backend.state()));
    }
    let capabilities = CapabilityFlags::from_bits_retain(state.client_capabilities);
    if !capabilities.contains(CapabilityFlags::CLIENT_PROTOCOL_41)
        || capabilities.contains(CapabilityFlags::CLIENT_SESSION_TRACK)
    {
        return Err(QueryExecutionError::OutputContract);
    }

    // Retain the source-derived AST until admission finishes. Neither callers
    // nor a previous command's parser mode can substitute a constructed AST.
    let parsed = parse_mysql_with_mode(sql, state.sql_modes()?.parser_flags());
    let planned = match &parsed {
        Ok(statements) => match statements.as_slice() {
            [Statement::Set(input)] => {
                crate::set_controller::admit_set(state, globals, input).map_err(QuerySqlError::Set)
            }
            _ => Err(QuerySqlError::UnsupportedStatement),
        },
        Err(_) => Err(QuerySqlError::Parse),
    };
    let mut stage = state.stage_command()?;
    stage.clear_diagnostics();
    let error = match planned {
        Err(error) => Some(error),
        Ok(plan) => crate::set_controller::apply_set(&mut stage, backend, plan)
            .await?
            .err()
            .map(QuerySqlError::Set),
    };
    let mut payload = BytesMut::new();
    if let Some(error) = error {
        stage.record_sql_error(error.code(), error.message());
        ErrPacket {
            error_code: error.code(),
            sql_state: error.sql_state(),
            message: Bytes::from_static(error.message().as_bytes()),
        }
        .encode(&mut payload);
    } else {
        stage.record_sql_success();
        let settings = stage.settings()?;
        let mut flags = StatusFlags::empty();
        flags.set(
            StatusFlags::SERVER_STATUS_AUTOCOMMIT,
            settings.transactions.autocommit == AutocommitSetting::Enabled,
        );
        flags.set(
            StatusFlags::SERVER_STATUS_IN_TRANS,
            settings.transactions.active.is_some(),
        );
        flags.set(
            StatusFlags::SERVER_STATUS_IN_TRANS_READONLY,
            settings
                .transactions
                .active
                .is_some_and(|active| active.access == FrontendTransactionAccess::ReadOnly),
        );
        flags.set(
            StatusFlags::SERVER_STATUS_NO_BACKSLASH_ESCAPES,
            settings.sql_modes.contains(SqlMode::NoBackslashEscapes),
        );
        OkPacket {
            affected_rows: 0,
            last_insert_id: 0,
            status_flags: flags,
            warnings: 0,
            info: Bytes::new(),
            session_state_changes: None,
        }
        .encode(&mut payload, capabilities)?;
    }
    let packet = RawPacket::new(response_sequence, payload.freeze())?;
    let mut encoded = BytesMut::with_capacity(packet.encoded_len()?);
    packet.encode_into(&mut encoded)?;
    output.write_all(&encoded).await?;
    output.flush().await?;
    let settings = stage.finish_with_sent_output()?;
    Ok(match error {
        Some(error) => QueryOutcome::SqlError { error, settings },
        None => QueryOutcome::Success(settings),
    })
}
