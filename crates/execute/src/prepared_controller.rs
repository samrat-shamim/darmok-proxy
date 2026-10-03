//! Private prepared-command execution beneath the owning TCP connection.
//! Only source-admitted local SELECT templates can enter this registry.

use bytes::{Bytes, BytesMut};
use darmok_protocol::command::decode_stmt_execute_parameters;
use darmok_protocol::resultset::encode_stmt_prepare_ok;
use darmok_protocol::{
    CapabilityFlags, Command, EofPacket, ErrPacket, OkPacket, encode_binary_row,
    encode_result_set_header,
};
use darmok_session::{
    PreparedStatementError, PreparedStatementLimits, PreparedStatementRegistry, SessionState,
};
use sqlparser::{
    ast::Statement,
    source::{MySqlSourceParseError, SourceProvenanceError, parse_mysql_source},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::query_controller::{append_packet, session_status_flags};
use crate::select_controller::{BinarySelect, PreparedSelect, SelectAdmissionError};
use crate::{QueryExecutionError, QuerySqlError, ServerSetValues};

pub(crate) struct PreparedCommands {
    statements: PreparedStatementRegistry<PreparedSelect>,
}

enum PreparedError {
    Query(QuerySqlError),
    Unknown,
    Registry(PreparedStatementError),
}

impl PreparedError {
    fn code(&self) -> u16 {
        match self {
            Self::Query(error) => error.code(),
            Self::Unknown => 1243,
            Self::Registry(_) => 1235,
        }
    }
    fn sql_state(&self) -> [u8; 5] {
        match self {
            Self::Query(error) => error.sql_state(),
            Self::Unknown => *b"HY000",
            Self::Registry(_) => *b"42000",
        }
    }
    fn message(&self) -> &'static str {
        match self {
            Self::Query(error) => error.message(),
            Self::Unknown => "Unknown prepared statement handler",
            Self::Registry(error) => match error {
                PreparedStatementError::CountLimit { .. } => {
                    "Prepared statement count limit reached"
                }
                PreparedStatementError::SqlByteLimit { .. } => {
                    "Prepared statement SQL byte limit reached"
                }
                PreparedStatementError::IdsExhausted => "Prepared statement IDs exhausted",
                _ => "Prepared statement registry invariant failed",
            },
        }
    }
}

enum PreparedOutput {
    Prepared {
        id: u32,
        columns: Vec<darmok_protocol::ColumnDefinition>,
        parameters: Vec<darmok_protocol::ColumnDefinition>,
    },
    Select(BinarySelect),
    Ok,
    Error(PreparedError),
}

impl PreparedCommands {
    pub(crate) fn new(limits: PreparedStatementLimits) -> Self {
        Self {
            statements: PreparedStatementRegistry::new(limits),
        }
    }

    pub(crate) async fn command<W: AsyncWrite + Unpin>(
        &mut self,
        state: &mut SessionState,
        globals: &ServerSetValues,
        command: Command,
        mut sequence: u8,
        output: &mut W,
    ) -> Result<(), QueryExecutionError> {
        let result = match command {
            Command::StmtClose { stmt_id } => {
                // The stock precheck deliberately gives unknown IDs the same
                // no-response close outcome. Never invent an ERR packet here.
                self.statements.remove(stmt_id);
                return Ok(());
            }
            Command::StmtPrepare(sql) => match admit(state, globals, &sql)? {
                Ok(plan) => {
                    let parameters = plan.parameters().to_vec();
                    let columns = plan.columns().to_vec();
                    match self.statements.register(sql, parameters.len() as u16, plan) {
                        Ok(id) => PreparedOutput::Prepared {
                            id,
                            columns,
                            parameters,
                        },
                        Err(
                            error @ (PreparedStatementError::CountLimit { .. }
                            | PreparedStatementError::SqlByteLimit { .. }
                            | PreparedStatementError::IdsExhausted),
                        ) => PreparedOutput::Error(PreparedError::Registry(error)),
                        Err(_) => return Err(SourceProvenanceError.into()),
                    }
                }
                Err(error) => PreparedOutput::Error(PreparedError::Query(error)),
            },
            Command::StmtExecute {
                stmt_id,
                parameter_payload,
                ..
            } => match self.statements.get(stmt_id) {
                None => PreparedOutput::Error(PreparedError::Unknown),
                Some(statement) => {
                    let parameters = decode_stmt_execute_parameters(
                        &parameter_payload,
                        statement.param_count(),
                        statement.last_param_types(),
                    )?;
                    if let Some(types) = parameters.new_types {
                        self.statements
                            .set_param_types(stmt_id, types)
                            .map_err(|_| SourceProvenanceError)?;
                    }
                    let types = self
                        .statements
                        .get(stmt_id)
                        .ok_or(SourceProvenanceError)?
                        .last_param_types()
                        .to_vec();
                    match self
                        .statements
                        .plan_mut(stmt_id)
                        .ok_or(SourceProvenanceError)?
                        .resolve(state, globals, &parameters.values, &types)
                    {
                        Ok(plan) => PreparedOutput::Select(plan),
                        Err(SelectAdmissionError::Sql(error)) => PreparedOutput::Error(
                            PreparedError::Query(QuerySqlError::Select(error)),
                        ),
                        Err(SelectAdmissionError::Source(error)) => return Err(error.into()),
                        Err(SelectAdmissionError::Settings(error)) => return Err(error.into()),
                    }
                }
            },
            Command::StmtReset { stmt_id } => {
                // No cursor or long data is admitted. The stock reset preserves
                // cached binding types and the derived SELECT description.
                if self.statements.get(stmt_id).is_some() {
                    PreparedOutput::Ok
                } else {
                    PreparedOutput::Error(PreparedError::Unknown)
                }
            }
            _ => return Err(QueryExecutionError::NotQuery),
        };

        let capabilities = CapabilityFlags::from_bits_retain(state.client_capabilities);
        let mut stage = state.stage_command()?;
        stage.clear_diagnostics();
        match &result {
            PreparedOutput::Error(error) => stage.record_sql_error(error.code(), error.message()),
            PreparedOutput::Select(_) => stage.record_select_success(1),
            PreparedOutput::Ok => stage.record_sql_success(),
            // PREPARE reports metadata, not execution of its SELECT. It clears
            // conditions but preserves ROW_COUNT and FOUND_ROWS.
            PreparedOutput::Prepared { .. } => {}
        }
        let flags = session_status_flags(stage.settings()?);
        let warnings = stage.statement_condition_count_u16();
        let mut payload = BytesMut::new();
        let mut encoded = BytesMut::new();
        let mut append =
            |payload: &mut BytesMut| append_packet(payload, &mut sequence, &mut encoded);
        let eof = EofPacket {
            warnings: 0,
            status_flags: flags,
        };
        let ok = OkPacket {
            affected_rows: 0,
            last_insert_id: 0,
            status_flags: flags,
            warnings,
            info: Bytes::new(),
            session_state_changes: None,
        };
        match result {
            PreparedOutput::Error(error) => {
                ErrPacket {
                    error_code: error.code(),
                    sql_state: error.sql_state(),
                    message: Bytes::from_static(error.message().as_bytes()),
                }
                .encode(&mut payload);
                append(&mut payload)?;
            }
            PreparedOutput::Prepared {
                id,
                columns,
                parameters,
            } => {
                encode_stmt_prepare_ok(
                    id,
                    columns.len() as u16,
                    parameters.len() as u16,
                    warnings,
                    &mut payload,
                );
                append(&mut payload)?;
                for definitions in [&parameters, &columns] {
                    for column in definitions {
                        column.encode(&mut payload);
                        append(&mut payload)?;
                    }
                    if !definitions.is_empty()
                        && !capabilities.contains(CapabilityFlags::CLIENT_DEPRECATE_EOF)
                    {
                        eof.encode(&mut payload);
                        append(&mut payload)?;
                    }
                }
            }
            PreparedOutput::Select(plan) => {
                encode_result_set_header(plan.columns.len() as u64, &mut payload);
                append(&mut payload)?;
                for column in &plan.columns {
                    column.encode(&mut payload);
                    append(&mut payload)?;
                }
                if !capabilities.contains(CapabilityFlags::CLIENT_DEPRECATE_EOF) {
                    eof.encode(&mut payload);
                    append(&mut payload)?;
                }
                encode_binary_row(&plan.row, &plan.columns, &mut payload)?;
                append(&mut payload)?;
                if capabilities.contains(CapabilityFlags::CLIENT_DEPRECATE_EOF) {
                    ok.encode_ok_as_eof(&mut payload, capabilities)?;
                } else {
                    EofPacket {
                        warnings,
                        status_flags: flags,
                    }
                    .encode(&mut payload);
                }
                append(&mut payload)?;
            }
            PreparedOutput::Ok => {
                ok.encode(&mut payload, capabilities)?;
                append(&mut payload)?;
            }
        }
        output.write_all(&encoded).await?;
        output.flush().await?;
        stage.finish_with_sent_output()?;
        Ok(())
    }
}

fn admit(
    state: &SessionState,
    globals: &ServerSetValues,
    sql: &str,
) -> Result<Result<PreparedSelect, QuerySqlError>, QueryExecutionError> {
    let parsed = match parse_mysql_source(sql, state.sql_modes()?.parser_flags()) {
        Ok(parsed) => parsed,
        Err(MySqlSourceParseError::Parse(_)) => return Ok(Err(QuerySqlError::Parse)),
        Err(MySqlSourceParseError::UnsupportedExecutableComment) => {
            return Ok(Err(QuerySqlError::UnsupportedExecutableComment));
        }
        Err(MySqlSourceParseError::Source(error)) => return Err(error.into()),
    };
    if !matches!(parsed.statements(), [Statement::Query(_)]) {
        return Ok(Err(QuerySqlError::UnsupportedStatement));
    }
    let Some(source) = parsed.single_select()? else {
        return Ok(Err(QuerySqlError::UnsupportedStatement));
    };
    match PreparedSelect::admit(state, globals, source) {
        Ok(plan) => Ok(Ok(plan)),
        Err(SelectAdmissionError::Sql(error)) => Ok(Err(QuerySqlError::Select(error))),
        Err(SelectAdmissionError::Source(error)) => Err(error.into()),
        Err(SelectAdmissionError::Settings(error)) => Err(error.into()),
    }
}
