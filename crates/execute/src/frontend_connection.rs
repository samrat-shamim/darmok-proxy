//! Exclusive command-phase TCP ownership. Authentication/listening are separate
//! boundaries; this owner never returns its socket, session or native connection
//! for continued execution after termination.

use bytes::{Bytes, BytesMut};
use darmok_protocol::{CapabilityFlags, Command, ErrPacket, MySqlPacketCodec, OkPacket, RawPacket};
use darmok_session::{PreparedStatementLimits, SessionState};
use darmok_types::error::ProxyError;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_postgres::TransactionState;
use tokio_util::codec::Decoder;

use crate::query_controller::{QueryOutcome, execute_query_command, session_status_flags};
use crate::{
    NativeBackend, NativeBackendDisposeError, NativeBackendDisposed, NativeBackendError,
    NativeBackendState, NativeControlCompletion, QueryExecutionError, ServerSetValues,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendEnd {
    Quit,
    Eof,
    Release,
}

#[derive(Debug, thiserror::Error)]
pub enum FrontendError {
    #[error(transparent)]
    Query(#[from] QueryExecutionError),
    #[error("command framing failed: {0}")]
    Framing(#[source] ProxyError),
    #[error("command decoding failed: {0}")]
    Decode(#[source] ProxyError),
    #[error("command phase requires initial sequence 0, got {0}")]
    Sequence(u8),
    #[error("the negotiated wire capabilities cannot be handled by this command owner")]
    Capabilities,
    #[error("unimplemented {0} cannot receive a protocol response; the connection is closed")]
    NoResponseCommand(&'static str),
    #[error("frontend command read failed: {0}")]
    Input(#[source] std::io::Error),
}

/// Historical session state and independent termination receipts. The session
/// describes the last command, not a reusable backend after disconnect cleanup.
/// Disposal alone is not evidence of server rollback or a committed result.
#[must_use]
#[derive(Debug)]
pub struct FrontendReport {
    pub end: Result<FrontendEnd, FrontendError>,
    pub session: SessionState,
    /// Attempted only for a normal end with a confirmed active native state.
    pub rollback: Option<Result<NativeControlCompletion, NativeBackendError>>,
    pub shutdown: Result<(), std::io::Error>,
    pub disposal: Result<NativeBackendDisposed, NativeBackendDisposeError>,
}

/// Owns one real TCP command-phase connection and its exclusive native owner.
/// The caller must have completed the separate handshake boundary. Any bytes
/// already read past that boundary are retained as original command input.
/// This type implements no listener, authentication, route selection or pooling.
#[must_use]
pub struct FrontendConnection {
    transport: TcpStream,
    input: BytesMut,
    codec: MySqlPacketCodec,
    session: SessionState,
    backend: NativeBackend,
    globals: ServerSetValues,
    prepared: crate::prepared_controller::PreparedCommands,
}

impl FrontendConnection {
    pub fn new(
        transport: TcpStream,
        session: SessionState,
        backend: NativeBackend,
        globals: ServerSetValues,
        prepared_limits: PreparedStatementLimits,
        max_packet_size: u32,
        buffered_input: BytesMut,
    ) -> Self {
        Self {
            transport,
            input: buffered_input,
            codec: MySqlPacketCodec::new(max_packet_size),
            session,
            backend,
            globals,
            prepared: crate::prepared_controller::PreparedCommands::new(prepared_limits),
        }
    }

    /// Serially reads commands until QUIT, EOF, successful RELEASE or a terminal
    /// error. Every path closes the frontend and disposes the native driver.
    /// Dropping the future also drops both owned transports; no task is detached.
    pub async fn run(mut self) -> FrontendReport {
        let end = self.commands().await;
        let shutdown = self.transport.shutdown().await;
        let rollback = if end.is_ok()
            && matches!(
                self.backend.state(),
                NativeBackendState::Ready(
                    TransactionState::Transaction | TransactionState::FailedTransaction
                )
            ) {
            Some(self.backend.rollback().await)
        } else {
            None
        };
        let disposal = self.backend.dispose().await;
        FrontendReport {
            end,
            session: self.session,
            rollback,
            shutdown,
            disposal,
        }
    }

    fn check_contract(&self) -> Result<(), FrontendError> {
        let settings = self
            .session
            .transaction_settings()
            .map_err(QueryExecutionError::from)?;
        let expected = if settings.active.is_some() {
            TransactionState::Transaction
        } else {
            TransactionState::Idle
        };
        if self.backend.state() != NativeBackendState::Ready(expected) {
            return Err(QueryExecutionError::NativeState(self.backend.state()).into());
        }
        let capabilities = CapabilityFlags::from_bits_retain(self.session.client_capabilities);
        if !capabilities.contains(CapabilityFlags::CLIENT_PROTOCOL_41)
            || capabilities.intersects(
                CapabilityFlags::CLIENT_COMPRESS
                    | CapabilityFlags::CLIENT_ZSTD_COMPRESSION_ALGORITHM
                    | CapabilityFlags::CLIENT_QUERY_ATTRIBUTES
                    | CapabilityFlags::CLIENT_SESSION_TRACK
                    | CapabilityFlags::CLIENT_OPTIONAL_RESULTSET_METADATA,
            )
        {
            return Err(FrontendError::Capabilities);
        }
        Ok(())
    }

    async fn commands(&mut self) -> Result<FrontendEnd, FrontendError> {
        self.check_contract()?;
        loop {
            self.codec.reset_sequence();
            let Some(packet) = self.read_command().await? else {
                return Ok(FrontendEnd::Eof);
            };
            if packet.header.sequence_id != 0 {
                return Err(FrontendError::Sequence(packet.header.sequence_id));
            }
            let sequence = packet.next_sequence_id();
            self.check_contract()?;
            let command = Command::decode(&packet.payload, self.session.client_capabilities)
                .map_err(FrontendError::Decode)?;
            match command {
                Command::Quit => return Ok(FrontendEnd::Quit),
                command @ Command::Query(_) => {
                    if matches!(
                        execute_query_command(
                            &mut self.session,
                            &mut self.backend,
                            &self.globals,
                            &command,
                            sequence,
                            &mut self.transport
                        )
                        .await?,
                        QueryOutcome::Release(_)
                    ) {
                        return Ok(FrontendEnd::Release);
                    }
                }
                Command::Ping => self.command_response(sequence, false).await?,
                command @ (Command::StmtPrepare(_)
                | Command::StmtExecute { .. }
                | Command::StmtReset { .. }
                | Command::StmtClose { .. }) => {
                    self.prepared
                        .command(
                            &mut self.session,
                            &self.globals,
                            command,
                            sequence,
                            &mut self.transport,
                        )
                        .await?;
                }
                Command::StmtSendLongData { .. } => {
                    return Err(FrontendError::NoResponseCommand("COM_STMT_SEND_LONG_DATA"));
                }
                Command::InitDb(_)
                | Command::FieldList { .. }
                | Command::Statistics
                | Command::ProcessInfo
                | Command::ProcessKill { .. }
                | Command::ChangeUser { .. }
                | Command::ResetConnection
                | Command::SetOption { .. }
                | Command::StmtFetch { .. } => {
                    self.command_response(sequence, true).await?;
                }
            }
        }
    }

    async fn read_command(&mut self) -> Result<Option<RawPacket>, FrontendError> {
        loop {
            if let Some(packet) = self
                .codec
                .decode(&mut self.input)
                .map_err(FrontendError::Framing)?
            {
                return Ok(Some(packet));
            }
            if self
                .transport
                .read_buf(&mut self.input)
                .await
                .map_err(FrontendError::Input)?
                == 0
            {
                return self
                    .codec
                    .decode_eof(&mut self.input)
                    .map_err(FrontendError::Framing);
            }
        }
    }

    async fn command_response(
        &mut self,
        sequence: u8,
        unsupported: bool,
    ) -> Result<(), FrontendError> {
        let capabilities = CapabilityFlags::from_bits_retain(self.session.client_capabilities);
        let mut stage = self
            .session
            .stage_command()
            .map_err(QueryExecutionError::from)?;
        let mut payload = BytesMut::new();
        if unsupported {
            let message = "This MySQL command is not implemented";
            stage.clear_diagnostics();
            stage.record_sql_error(1235, message);
            ErrPacket {
                error_code: 1235,
                sql_state: *b"42000",
                message: Bytes::from_static(message.as_bytes()),
            }
            .encode(&mut payload);
        } else {
            // PING acknowledges liveness without changing the transaction,
            // pending choices, modes, last insert id, SQL condition list or its
            // statement count. Its OK serializes that count, including errors.
            stage.record_sql_success();
            OkPacket {
                affected_rows: 0,
                last_insert_id: 0,
                status_flags: session_status_flags(
                    stage.settings().map_err(QueryExecutionError::from)?,
                ),
                warnings: stage.statement_condition_count_u16(),
                info: Bytes::new(),
                session_state_changes: None,
            }
            .encode(&mut payload, capabilities)
            .map_err(QueryExecutionError::from)?;
        }
        let mut encoded = BytesMut::new();
        RawPacket::new(sequence, payload.freeze())
            .map_err(QueryExecutionError::from)?
            .encode_into(&mut encoded)
            .map_err(QueryExecutionError::from)?;
        self.transport
            .write_all(&encoded)
            .await
            .map_err(QueryExecutionError::from)?;
        self.transport
            .flush()
            .await
            .map_err(QueryExecutionError::from)?;
        let _settings = stage
            .finish_with_sent_output()
            .map_err(QueryExecutionError::from)?;
        Ok(())
    }
}
