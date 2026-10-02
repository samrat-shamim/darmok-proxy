//! Darmok additions: observe text simple-query rows and native completion.

use crate::client::{InnerClient, Responses};
use crate::codec::FrontendMessage;
use crate::connection::RequestMessages;
use crate::{Error, SimpleColumn, SimpleQueryRow, TransactionState, simple_query};
use fallible_iterator::FallibleIterator;
use futures_util::{Stream, stream::FusedStream};
use postgres_protocol::message::backend::Message;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

/// An observed event from a text simple-query request.
#[derive(Debug)]
pub enum SimpleQueryEvent {
    /// A statement's native description, also retained by each of its rows.
    RowDescription(Arc<[SimpleColumn]>),
    /// A row in the backend's text representation, without type lookup SQL.
    Row(SimpleQueryRow),
    /// The exact native tag completing one statement, not the whole request.
    CommandComplete(String),
    /// The backend recognized an empty query, not a zero-row command.
    EmptyQuery,
    /// A backend SQL error; continue to ReadyForQuery to observe its final state.
    BackendError(Error),
    /// The backend finished this request and reported its state at that boundary.
    ReadyForQuery(TransactionState),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SimpleQueryPhase {
    Commands,
    Rows,
    Empty,
    Failed,
    Done,
}

/// Stream of text rows and exact outcomes through final ReadyForQuery.
///
/// Backend errors are events, not terminal failures. A stream `Err` instead
/// terminates without confirming completion. Binary descriptions and COPY are
/// unsupported responses observed after submission, not pre-execution checks.
/// Dropping or stopping a stream supplies no ownership or cleanup receipt.
pub struct SimpleQueryEventStream {
    responses: Responses,
    phase: SimpleQueryPhase,
    columns: Option<Arc<[SimpleColumn]>>,
    has_yielded: bool,
}

impl SimpleQueryEventStream {
    /// Whether an event or terminal error has already been returned.
    ///
    /// A complete-request consumer can reject a partially consumed stream.
    /// Pending-only polling leaves this false. It is not an execution, readiness
    /// or ownership observation.
    pub fn has_yielded(&self) -> bool {
        self.has_yielded
    }
}

pub(crate) fn simple_query_events(
    client: &InnerClient,
    sql: &str,
) -> Result<SimpleQueryEventStream, Error> {
    let buf = simple_query::encode(client, sql)?;
    Ok(SimpleQueryEventStream {
        responses: client.send(RequestMessages::Single(FrontendMessage::Raw(buf)))?,
        phase: SimpleQueryPhase::Commands,
        columns: None,
        has_yielded: false,
    })
}

impl Stream for SimpleQueryEventStream {
    type Item = Result<SimpleQueryEvent, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.phase == SimpleQueryPhase::Done {
            return Poll::Ready(None);
        }
        let message = match ready!(this.responses.poll_next_raw(cx)) {
            Ok(message) => message,
            Err(error) => {
                this.phase = SimpleQueryPhase::Done;
                this.columns = None;
                this.has_yielded = true;
                return Poll::Ready(Some(Err(error)));
            }
        };
        let event = match (this.phase, message) {
            (SimpleQueryPhase::Commands, Message::RowDescription(body)) => {
                text_columns(body).map(|columns| {
                    this.phase = SimpleQueryPhase::Rows;
                    this.columns = Some(columns.clone());
                    SimpleQueryEvent::RowDescription(columns)
                })
            }
            (SimpleQueryPhase::Rows, Message::DataRow(body)) => match &this.columns {
                Some(columns) => {
                    SimpleQueryRow::new(columns.clone(), body).map(SimpleQueryEvent::Row)
                }
                None => Err(Error::unexpected_message()),
            },
            (
                SimpleQueryPhase::Commands | SimpleQueryPhase::Rows,
                Message::CommandComplete(body),
            ) => body.tag().map_err(Error::parse).map(|tag| {
                this.phase = SimpleQueryPhase::Commands;
                this.columns = None;
                SimpleQueryEvent::CommandComplete(tag.to_owned())
            }),
            (SimpleQueryPhase::Commands, Message::EmptyQueryResponse) => {
                this.phase = SimpleQueryPhase::Empty;
                Ok(SimpleQueryEvent::EmptyQuery)
            }
            (SimpleQueryPhase::Commands | SimpleQueryPhase::Rows, Message::ErrorResponse(body)) => {
                let error = Error::db(body);
                if error.as_db_error().is_some() {
                    this.phase = SimpleQueryPhase::Failed;
                    this.columns = None;
                    Ok(SimpleQueryEvent::BackendError(error))
                } else {
                    Err(error)
                }
            }
            (
                SimpleQueryPhase::Commands | SimpleQueryPhase::Empty | SimpleQueryPhase::Failed,
                Message::ReadyForQuery(body),
            ) => {
                this.phase = SimpleQueryPhase::Done;
                TransactionState::from_status(body.status()).map(SimpleQueryEvent::ReadyForQuery)
            }
            _ => Err(Error::unexpected_message()),
        };
        if event.is_err() {
            this.phase = SimpleQueryPhase::Done;
            this.columns = None;
        }
        this.has_yielded = true;
        Poll::Ready(Some(event))
    }
}

impl FusedStream for SimpleQueryEventStream {
    fn is_terminated(&self) -> bool {
        self.phase == SimpleQueryPhase::Done
    }
}

fn text_columns(
    body: postgres_protocol::message::backend::RowDescriptionBody,
) -> Result<Arc<[SimpleColumn]>, Error> {
    let mut columns = Vec::new();
    let mut fields = body.fields();
    while let Some(field) = fields.next().map_err(Error::parse)? {
        if field.format() != 0 {
            return Err(Error::parse(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "simple-query column {} has format {}; text format is required",
                    columns.len(),
                    field.format()
                ),
            )));
        }
        columns.push(SimpleColumn::new(field));
    }
    Ok(columns.into())
}
