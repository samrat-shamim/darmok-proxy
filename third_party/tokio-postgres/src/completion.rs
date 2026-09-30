//! Darmok additions: observe backend completion without discarding its outcome.

use crate::client::{InnerClient, Responses};
use crate::codec::FrontendMessage;
use crate::connection::RequestMessages;
use crate::types::BorrowToSql;
use crate::{Error, Row, Statement, query};
use futures_util::{Stream, stream::FusedStream};
use postgres_protocol::message::{backend::Message, frontend};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

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
}

pub(crate) fn command_events(client: &InnerClient, sql: &str) -> Result<CommandEventStream, Error> {
    let buf = client.with_buf(|buf| {
        frontend::query(sql, buf).map_err(Error::encode)?;
        Ok(buf.split().freeze())
    })?;
    Ok(CommandEventStream {
        responses: client.send(RequestMessages::Single(FrontendMessage::Raw(buf)))?,
        phase: CommandPhase::Commands,
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
    statement: Statement,
    responses: Responses,
    phase: QueryPhase,
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
        statement,
        responses: client.send(RequestMessages::Single(FrontendMessage::Raw(buf)))?,
        phase: QueryPhase::Binding,
    })
}

impl Stream for QueryEventStream {
    type Item = Result<QueryEvent, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if this.phase == QueryPhase::Done {
                return Poll::Ready(None);
            }
            let message = match ready!(this.responses.poll_next_raw(cx)) {
                Ok(message) => message,
                Err(error) => {
                    this.phase = QueryPhase::Done;
                    return Poll::Ready(Some(Err(error)));
                }
            };
            let event = match (this.phase, message) {
                (QueryPhase::Binding, Message::BindComplete) => {
                    this.phase = QueryPhase::Rows;
                    continue;
                }
                (QueryPhase::Rows, Message::DataRow(body)) => {
                    Row::new(this.statement.clone(), body).map(QueryEvent::Row)
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
                    QueryPhase::Binding | QueryPhase::Rows | QueryPhase::Complete,
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
            return Poll::Ready(Some(event));
        }
    }
}

impl FusedStream for QueryEventStream {
    fn is_terminated(&self) -> bool {
        self.phase == QueryPhase::Done
    }
}
