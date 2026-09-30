//! Darmok additions: observe backend completion without discarding its outcome.

use crate::client::{InnerClient, Responses};
use crate::codec::FrontendMessage;
use crate::connection::RequestMessages;
use crate::types::{BorrowToSql, Type};
use crate::{Column, Error, Row, Statement, query};
use fallible_iterator::FallibleIterator;
use futures_util::{Stream, stream::FusedStream};
use postgres_protocol::message::{backend::Message, frontend};
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use std::{error::Error as StdError, fmt};

/// The transaction state carried by a backend ReadyForQuery message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    /// No transaction is open (`I`).
    Idle,
    /// A transaction is open and usable (`T`).
    Transaction,
    /// A transaction is open and failed (`E`).
    FailedTransaction,
}

impl TransactionState {
    fn from_status(status: u8) -> Result<Self, Error> {
        match status {
            b'I' => Ok(Self::Idle),
            b'T' => Ok(Self::Transaction),
            b'E' => Ok(Self::FailedTransaction),
            _ => Err(Error::unexpected_message()),
        }
    }
}

/// An observed event from commands which do not return rows.
#[derive(Debug)]
pub enum CommandEvent {
    /// The exact backend command tag, without converting missing counts to zero.
    CommandComplete(String),
    /// The backend reported an empty query, not a successful zero-row command.
    EmptyQuery,
    /// A backend SQL error; the stream continues to ReadyForQuery.
    BackendError(Error),
    /// The backend finished this request and reported its state at that boundary.
    ReadyForQuery(TransactionState),
}

/// An observed event from one prepared statement execution.
#[derive(Debug)]
pub enum QueryEvent {
    /// A row using the prepared statement's native description.
    Row(Row),
    /// The exact backend command tag.
    CommandComplete(String),
    /// The prepared statement was empty, not a successful zero-row command.
    EmptyQuery,
    /// Execution suspended a portal instead of completing the command.
    PortalSuspended,
    /// A backend SQL error; the stream continues to ReadyForQuery.
    BackendError(Error),
    /// The backend finished this request and reported its state at that boundary.
    ReadyForQuery(TransactionState),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandPhase {
    Commands,
    Failed,
    Done,
}

/// Stream of command outcomes through a final ReadyForQuery event.
///
/// Backend errors are events, not stream failures: receiving one does not
/// establish transaction state. An `Err` item is terminal and leaves completion
/// unconfirmed. The stream is fused after that item or its ReadyForQuery event.
pub struct CommandEventStream {
    responses: Responses,
    phase: CommandPhase,
    has_yielded: bool,
}

impl CommandEventStream {
    /// Whether this stream has already yielded an event or terminal error.
    ///
    /// A consumer requiring the complete request must check this before taking
    /// over the stream. Polling Pending does not consume an event. This flag
    /// says nothing about SQL execution, readiness or connection ownership.
    pub fn has_yielded(&self) -> bool {
        self.has_yielded
    }
}

pub(crate) fn command_events(client: &InnerClient, sql: &str) -> Result<CommandEventStream, Error> {
    let buf = client.with_buf(|buf| {
        frontend::query(sql, buf).map_err(Error::encode)?;
        Ok(buf.split().freeze())
    })?;
    Ok(CommandEventStream {
        responses: client.send(RequestMessages::Single(FrontendMessage::Raw(buf)))?,
        phase: CommandPhase::Commands,
        has_yielded: false,
    })
}

impl Stream for CommandEventStream {
    type Item = Result<CommandEvent, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.phase == CommandPhase::Done {
            return Poll::Ready(None);
        }
        let message = match ready!(this.responses.poll_next_raw(cx)) {
            Ok(message) => message,
            Err(error) => {
                this.phase = CommandPhase::Done;
                this.has_yielded = true;
                return Poll::Ready(Some(Err(error)));
            }
        };
        let event = match (this.phase, message) {
            (CommandPhase::Commands, Message::CommandComplete(body)) => body
                .tag()
                .map(|tag| CommandEvent::CommandComplete(tag.to_owned()))
                .map_err(Error::parse),
            (CommandPhase::Commands, Message::EmptyQueryResponse) => Ok(CommandEvent::EmptyQuery),
            (CommandPhase::Commands, Message::ErrorResponse(body)) => {
                let error = Error::db(body);
                if error.as_db_error().is_some() {
                    this.phase = CommandPhase::Failed;
                    Ok(CommandEvent::BackendError(error))
                } else {
                    Err(error)
                }
            }
            (_, Message::ReadyForQuery(body)) => {
                this.phase = CommandPhase::Done;
                TransactionState::from_status(body.status()).map(CommandEvent::ReadyForQuery)
            }
            _ => Err(Error::unexpected_message()),
        };
        if event.is_err() {
            this.phase = CommandPhase::Done;
        }
        this.has_yielded = true;
        Poll::Ready(Some(event))
    }
}

impl FusedStream for CommandEventStream {
    fn is_terminated(&self) -> bool {
        self.phase == CommandPhase::Done
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueryPhase {
    Parsing,
    TypedBinding,
    Parameters,
    Description,
    Binding,
    Rows,
    Complete,
    Failed,
    Done,
}

/// Stream of prepared rows and outcomes through ReadyForQuery.
///
/// A command tag, empty query or suspended portal is separate from readiness.
/// Errors before BindComplete remain observable alongside their final backend
/// transaction state. An `Err` item terminates without confirming completion.
pub struct QueryEventStream {
    inner: QueryEvents,
}

/// A result OID outside the built-in result description contract.
///
/// This is the source of a terminal stream error, after SQL was submitted.
/// It does not establish backend completion or rollback.
#[derive(Debug)]
pub struct UnsupportedBuiltinResultType {
    oid: u32,
}

impl UnsupportedBuiltinResultType {
    /// The exact native result type OID which required a separate type lookup.
    pub fn oid(&self) -> u32 {
        self.oid
    }
}

impl fmt::Display for UnsupportedBuiltinResultType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "result type OID {} is not a built-in PostgreSQL type",
            self.oid
        )
    }
}

impl StdError for UnsupportedBuiltinResultType {}

/// One-shot typed query outcomes with built-in result descriptions only.
///
/// Parse, Bind, Describe, Execute and Sync are submitted together. The result
/// description is observed after submission, not checked before Execute. This
/// primitive is intended for fixed internal queries with known built-in output;
/// it is not semantic admission, catalog validity or frontend SQL execution.
/// Unknown result OIDs fail explicitly without issuing another type query.
///
/// SQL errors remain events through ReadyForQuery; a terminal `Err` leaves
/// completion unconfirmed. Stopping consumption supplies no cleanup receipt.
pub struct BuiltinQueryEventStream {
    inner: QueryEvents,
}

impl BuiltinQueryEventStream {
    /// The observed description, including an empty description for NoData.
    /// None means no result description was observed, not a zero-column result.
    /// This is not a prepared statement or a pre-execution type-check receipt.
    pub fn columns(&self) -> Option<&[Column]> {
        self.inner.statement.as_ref().map(Statement::columns)
    }

    /// Whether an event or terminal error has already been returned.
    /// Pending-only polling does not set this flag or establish readiness.
    pub fn has_yielded(&self) -> bool {
        self.inner.has_yielded
    }
}

struct QueryEvents {
    statement: Option<Statement>,
    responses: Responses,
    phase: QueryPhase,
    parameter_count: usize,
    has_yielded: bool,
}

pub(crate) fn query_events<P, I>(
    client: &InnerClient,
    statement: Statement,
    params: I,
) -> Result<QueryEventStream, Error>
where
    P: BorrowToSql,
    I: IntoIterator<Item = P>,
    I::IntoIter: ExactSizeIterator,
{
    let buf = query::encode(client, &statement, params)?;
    Ok(QueryEventStream {
        inner: QueryEvents {
            statement: Some(statement),
            responses: client.send(RequestMessages::Single(FrontendMessage::Raw(buf)))?,
            phase: QueryPhase::Binding,
            parameter_count: 0,
            has_yielded: false,
        },
    })
}

pub(crate) fn query_typed_builtin_events<P, I>(
    client: &InnerClient,
    sql: &str,
    params: I,
) -> Result<BuiltinQueryEventStream, Error>
where
    P: BorrowToSql,
    I: IntoIterator<Item = (P, Type)>,
{
    let params = params.into_iter().collect::<Vec<_>>();
    let parameter_count = params.len();
    let buf = client.with_buf(|buf| {
        frontend::parse("", sql, params.iter().map(|(_, ty)| ty.oid()), buf)
            .map_err(Error::encode)?;
        query::encode_bind_raw("", params, "", buf)?;
        frontend::describe(b'S', "", buf).map_err(Error::encode)?;
        frontend::execute("", 0, buf).map_err(Error::encode)?;
        frontend::sync(buf);
        Ok(buf.split().freeze())
    })?;
    Ok(BuiltinQueryEventStream {
        inner: QueryEvents {
            statement: None,
            responses: client.send(RequestMessages::Single(FrontendMessage::Raw(buf)))?,
            phase: QueryPhase::Parsing,
            parameter_count,
            has_yielded: false,
        },
    })
}

impl QueryEvents {
    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<QueryEvent, Error>>> {
        let this = self;
        loop {
            if this.phase == QueryPhase::Done {
                return Poll::Ready(None);
            }
            let message = match ready!(this.responses.poll_next_raw(cx)) {
                Ok(message) => message,
                Err(error) => {
                    this.phase = QueryPhase::Done;
                    this.has_yielded = true;
                    return Poll::Ready(Some(Err(error)));
                }
            };
            let event = match (this.phase, message) {
                (QueryPhase::Parsing, Message::ParseComplete) => {
                    this.phase = QueryPhase::TypedBinding;
                    continue;
                }
                (QueryPhase::TypedBinding, Message::BindComplete) => {
                    this.phase = QueryPhase::Parameters;
                    continue;
                }
                (QueryPhase::Parameters, Message::ParameterDescription(body)) => {
                    match body.parameters().count().map_err(Error::parse) {
                        Ok(count) if count == this.parameter_count => {
                            this.phase = QueryPhase::Description;
                            continue;
                        }
                        Ok(_) => Err(Error::unexpected_message()),
                        Err(error) => Err(error),
                    }
                }
                (QueryPhase::Description, Message::RowDescription(body)) => {
                    match builtin_columns(body) {
                        Ok(columns) => {
                            this.statement = Some(Statement::unnamed(vec![], columns));
                            this.phase = QueryPhase::Rows;
                            continue;
                        }
                        Err(error) => Err(error),
                    }
                }
                (QueryPhase::Description, Message::NoData) => {
                    this.statement = Some(Statement::unnamed(vec![], vec![]));
                    this.phase = QueryPhase::Rows;
                    continue;
                }
                (QueryPhase::Binding, Message::BindComplete) => {
                    this.phase = QueryPhase::Rows;
                    continue;
                }
                (QueryPhase::Rows, Message::DataRow(body)) => {
                    let statement = this
                        .statement
                        .as_ref()
                        .expect("Rows requires a description");
                    Row::new(statement.clone(), body).map(QueryEvent::Row)
                }
                (QueryPhase::Rows, Message::CommandComplete(body)) => {
                    this.phase = QueryPhase::Complete;
                    body.tag()
                        .map(|tag| QueryEvent::CommandComplete(tag.to_owned()))
                        .map_err(Error::parse)
                }
                (QueryPhase::Rows, Message::EmptyQueryResponse) => {
                    this.phase = QueryPhase::Complete;
                    Ok(QueryEvent::EmptyQuery)
                }
                (QueryPhase::Rows, Message::PortalSuspended) => {
                    this.phase = QueryPhase::Complete;
                    Ok(QueryEvent::PortalSuspended)
                }
                (
                    QueryPhase::Parsing
                    | QueryPhase::TypedBinding
                    | QueryPhase::Parameters
                    | QueryPhase::Description
                    | QueryPhase::Binding
                    | QueryPhase::Rows
                    | QueryPhase::Complete,
                    Message::ErrorResponse(body),
                ) => {
                    // In autocommit, Sync can fail a deferred constraint after
                    // Execute already emitted rows and CommandComplete.
                    let error = Error::db(body);
                    if error.as_db_error().is_some() {
                        this.phase = QueryPhase::Failed;
                        Ok(QueryEvent::BackendError(error))
                    } else {
                        Err(error)
                    }
                }
                (QueryPhase::Complete | QueryPhase::Failed, Message::ReadyForQuery(body)) => {
                    this.phase = QueryPhase::Done;
                    TransactionState::from_status(body.status()).map(QueryEvent::ReadyForQuery)
                }
                _ => Err(Error::unexpected_message()),
            };
            if event.is_err() {
                this.phase = QueryPhase::Done;
            }
            this.has_yielded = true;
            return Poll::Ready(Some(event));
        }
    }
}

fn builtin_columns(
    body: postgres_protocol::message::backend::RowDescriptionBody,
) -> Result<Vec<Column>, Error> {
    let mut columns = Vec::new();
    let mut fields = body.fields();
    while let Some(field) = fields.next().map_err(Error::parse)? {
        let type_ = Type::from_oid(field.type_oid()).ok_or_else(|| {
            Error::from_sql(
                Box::new(UnsupportedBuiltinResultType {
                    oid: field.type_oid(),
                }),
                columns.len(),
            )
        })?;
        columns.push(Column {
            name: field.name().to_owned(),
            table_oid: Some(field.table_oid()).filter(|oid| *oid != 0),
            column_id: Some(field.column_id()).filter(|id| *id != 0),
            type_modifier: field.type_modifier(),
            r#type: type_,
        });
    }
    Ok(columns)
}

impl Stream for QueryEventStream {
    type Item = Result<QueryEvent, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().inner.poll_next(cx)
    }
}

impl FusedStream for QueryEventStream {
    fn is_terminated(&self) -> bool {
        self.inner.phase == QueryPhase::Done
    }
}

impl Stream for BuiltinQueryEventStream {
    type Item = Result<QueryEvent, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().inner.poll_next(cx)
    }
}

impl FusedStream for BuiltinQueryEventStream {
    fn is_terminated(&self) -> bool {
        self.inner.phase == QueryPhase::Done
    }
}
