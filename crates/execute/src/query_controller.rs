//! Source admission and response ownership for the selected COM_QUERY subset.
//! The parser uses the settled session's current mode on every command.

use crate::select_controller::{SelectAdmissionError, SelectPlan};
use crate::set_controller::SetAdmissionError;
use crate::{
    NativeBackend, NativeBackendError, NativeBackendState, SelectSqlError, ServerSetValues,
    SetSqlError, TransactionSqlError,
};
use bytes::{Bytes, BytesMut};
use darmok_protocol::{
    CapabilityFlags, Command, EofPacket, ErrPacket, OkPacket, RawPacket, StatusFlags,
    encode_result_set_header, encode_text_row,
};
use darmok_session::{
    AutocommitSetting, CommandSettingsSnapshot, FrontendTransactionAccess, SessionState, SqlMode,
    TransactionSettingsError,
};
use darmok_types::error::ProxyError;
use sqlparser::{
    ast::Statement,
    source::{MySqlSourceParseError, SourceProvenanceError, parse_mysql_source},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_postgres::TransactionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySqlError {
    Parse,
    UnsupportedStatement,
    UnsupportedExecutableComment,
    Set(SetSqlError),
    Select(SelectSqlError),
    Transaction(TransactionSqlError),
}

impl QuerySqlError {
    pub fn code(self) -> u16 {
        match self {
            Self::Parse => 1064,
            Self::UnsupportedStatement | Self::UnsupportedExecutableComment => 1235,
            Self::Set(error) => error.code(),
            Self::Select(error) => error.code(),
            Self::Transaction(error) => error.code(),
        }
    }

    pub fn sql_state(self) -> [u8; 5] {
        match self {
            Self::Parse | Self::UnsupportedStatement | Self::UnsupportedExecutableComment => {
                *b"42000"
            }
            Self::Set(error) => error.sql_state(),
            Self::Select(error) => error.sql_state(),
            Self::Transaction(error) => error.sql_state(),
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::Parse => "The MySQL query could not be parsed",
            Self::UnsupportedStatement => "This statement or query batch is not implemented",
            Self::UnsupportedExecutableComment => "MySQL executable comments are not implemented",
            Self::Set(error) => error.message(),
            Self::Select(error) => error.message(),
            Self::Transaction(error) => error.message(),
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
    #[error("query output requires protocol 4.1 without session tracking or optional metadata")]
    OutputContract,
    #[error(transparent)]
    Source(#[from] SourceProvenanceError),
    #[error(transparent)]
    Native(#[from] NativeBackendError),
    #[error(transparent)]
    Encode(#[from] ProxyError),
    #[error("query response write failed: {0}")]
    Output(#[from] std::io::Error),
}

enum QueryPlan {
    Set(crate::set_controller::SetPlan),
    Select(SelectPlan),
    Transaction(crate::transaction_controller::TransactionPlan),
}

/// Executes one selected SET, transaction control or local SELECT from an original decoded query.
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
        || capabilities.intersects(
            CapabilityFlags::CLIENT_SESSION_TRACK
                | CapabilityFlags::CLIENT_OPTIONAL_RESULTSET_METADATA,
        )
    {
        return Err(QueryExecutionError::OutputContract);
    }

    // Retain the source-derived AST until admission finishes. Neither callers
    // nor a previous command's parser mode can substitute a constructed AST.
    let parsed = parse_mysql_source(sql, state.sql_modes()?.parser_flags());
    let planned = match &parsed {
        Ok(parsed) => match parsed.statements() {
            [Statement::Set(_)] => {
                let source = parsed.single_set().ok_or(SourceProvenanceError)?;
                match crate::set_controller::admit_set(state, globals, source) {
                    Ok(plan) => Ok(QueryPlan::Set(plan)),
                    Err(SetAdmissionError::Sql(error)) => Err(QuerySqlError::Set(error)),
                    Err(SetAdmissionError::Source(error)) => return Err(error.into()),
                }
            }
            [
                input @ (Statement::StartTransaction { .. }
                | Statement::Commit { .. }
                | Statement::Rollback { .. }),
            ] => match crate::transaction_controller::admit_transaction(state, input) {
                Ok(plan) => Ok(QueryPlan::Transaction(plan)),
                Err(crate::transaction_controller::TransactionAdmissionError::Sql(error)) => {
                    Err(QuerySqlError::Transaction(error))
                }
                Err(crate::transaction_controller::TransactionAdmissionError::Settings(error)) => {
                    return Err(error.into());
                }
            },
            [Statement::Query(_)] => {
                if let Some(source) = parsed.single_select()? {
                    match crate::select_controller::admit_select(state, globals, source) {
                        Ok(plan) => Ok(QueryPlan::Select(plan)),
                        Err(SelectAdmissionError::Sql(error)) => Err(QuerySqlError::Select(error)),
                        Err(SelectAdmissionError::Source(error)) => return Err(error.into()),
                        Err(SelectAdmissionError::Settings(error)) => return Err(error.into()),
                    }
                } else {
                    Err(QuerySqlError::UnsupportedStatement)
                }
            }
            _ => Err(QuerySqlError::UnsupportedStatement),
        },
        Err(MySqlSourceParseError::Parse(_)) => Err(QuerySqlError::Parse),
        Err(MySqlSourceParseError::UnsupportedExecutableComment) => {
            Err(QuerySqlError::UnsupportedExecutableComment)
        }
        Err(MySqlSourceParseError::Source(error)) => return Err((*error).into()),
    };
    let mut stage = state.stage_command()?;
    stage.clear_diagnostics();
    let mut select = None;
    let error = match planned {
        Err(error) => Some(error),
        Ok(QueryPlan::Set(plan)) => crate::set_controller::apply_set(&mut stage, backend, plan)
            .await?
            .err()
            .map(QuerySqlError::Set),
        Ok(QueryPlan::Select(plan)) => {
            stage.record_select_success(1);
            select = Some(plan);
            None
        }
        Ok(QueryPlan::Transaction(plan)) => {
            crate::transaction_controller::apply_transaction(&mut stage, backend, plan).await?;
            None
        }
    };
    let mut payload = BytesMut::new();
    let mut encoded = BytesMut::new();
    let mut sequence = response_sequence;
    if let Some(error) = error {
        stage.record_sql_error(error.code(), error.message());
        ErrPacket {
            error_code: error.code(),
            sql_state: error.sql_state(),
            message: Bytes::from_static(error.message().as_bytes()),
        }
        .encode(&mut payload);
        append_packet(&mut payload, &mut sequence, &mut encoded)?;
    } else {
        if select.is_none() {
            stage.record_sql_success();
        }
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
        let ok = OkPacket {
            affected_rows: 0,
            last_insert_id: 0,
            status_flags: flags,
            warnings: 0,
            info: Bytes::new(),
            session_state_changes: None,
        };
        if let Some(select) = select {
            encode_result_set_header(select.columns.len() as u64, &mut payload);
            append_packet(&mut payload, &mut sequence, &mut encoded)?;
            for column in &select.columns {
                column.encode(&mut payload);
                append_packet(&mut payload, &mut sequence, &mut encoded)?;
            }
            if !capabilities.contains(CapabilityFlags::CLIENT_DEPRECATE_EOF) {
                EofPacket {
                    warnings: 0,
                    status_flags: flags,
                }
                .encode(&mut payload);
                append_packet(&mut payload, &mut sequence, &mut encoded)?;
            }
            let values: Vec<_> = select.row.iter().map(|cell| cell.as_deref()).collect();
            encode_text_row(&values, &mut payload);
            append_packet(&mut payload, &mut sequence, &mut encoded)?;
            if capabilities.contains(CapabilityFlags::CLIENT_DEPRECATE_EOF) {
                ok.encode_ok_as_eof(&mut payload, capabilities)?;
            } else {
                EofPacket {
                    warnings: 0,
                    status_flags: flags,
                }
                .encode(&mut payload);
            }
        } else {
            ok.encode(&mut payload, capabilities)?;
        }
        append_packet(&mut payload, &mut sequence, &mut encoded)?;
    }
    output.write_all(&encoded).await?;
    output.flush().await?;
    let settings = stage.finish_with_sent_output()?;
    Ok(match error {
        Some(error) => QueryOutcome::SqlError { error, settings },
        None => QueryOutcome::Success(settings),
    })
}

fn append_packet(
    payload: &mut BytesMut,
    sequence: &mut u8,
    encoded: &mut BytesMut,
) -> Result<(), ProxyError> {
    let packet = RawPacket::new(*sequence, std::mem::take(payload).freeze())?;
    packet.encode_into(encoded)?;
    *sequence = packet.next_sequence_id();
    Ok(())
}
