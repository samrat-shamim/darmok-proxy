// Modified for Darmok: retain native simple-query column facts and share encoding.
// See ../UPSTREAM.md for the source pin and local changes.
use crate::client::{InnerClient, Responses};
use crate::codec::FrontendMessage;
use crate::connection::RequestMessages;
use crate::query::extract_row_affected;
use crate::{Error, SimpleQueryMessage, SimpleQueryRow};
use bytes::Bytes;
use fallible_iterator::FallibleIterator;
use futures_util::Stream;
use log::debug;
use pin_project_lite::pin_project;
use postgres_protocol::message::backend::{Field, Message};
use postgres_protocol::message::frontend;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

/// Information about a column of a single query row.
#[derive(Debug)]
pub struct SimpleColumn {
    name: String,
    table_oid: Option<u32>,
    column_id: Option<i16>,
    type_oid: u32,
    type_size: i16,
    type_modifier: i32,
    format: i16,
}

impl SimpleColumn {
    pub(crate) fn new(field: Field<'_>) -> SimpleColumn {
        SimpleColumn {
            name: field.name().to_owned(),
            table_oid: Some(field.table_oid()).filter(|oid| *oid != 0),
            column_id: Some(field.column_id()).filter(|id| *id != 0),
            type_oid: field.type_oid(),
            type_size: field.type_size(),
            type_modifier: field.type_modifier(),
            format: field.format(),
        }
    }

    /// Returns the name of the column.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the native table OID, or None when the backend reports no origin.
    pub fn table_oid(&self) -> Option<u32> {
        self.table_oid
    }

    /// Returns the native column ID, or None when the backend reports no origin.
    pub fn column_id(&self) -> Option<i16> {
        self.column_id
    }

    /// Returns the raw type OID without looking up connector type metadata.
    pub fn type_oid(&self) -> u32 {
        self.type_oid
    }

    /// Returns the type size reported by the backend, including special values.
    pub fn type_size(&self) -> i16 {
        self.type_size
    }

    /// Returns the native type modifier.
    pub fn type_modifier(&self) -> i32 {
        self.type_modifier
    }

    /// Returns the native format code: 0 for text, 1 for binary.
    pub fn format(&self) -> i16 {
        self.format
    }
}

pub async fn simple_query(client: &InnerClient, query: &str) -> Result<SimpleQueryStream, Error> {
    debug!("executing simple query: {query}");

    let buf = encode(client, query)?;
    let responses = client.send(RequestMessages::Single(FrontendMessage::Raw(buf)))?;

    Ok(SimpleQueryStream {
        responses,
        columns: None,
    })
}

pub async fn batch_execute(client: &InnerClient, query: &str) -> Result<(), Error> {
    debug!("executing statement batch: {query}");

    let buf = encode(client, query)?;
    let mut responses = client.send(RequestMessages::Single(FrontendMessage::Raw(buf)))?;

    loop {
        match responses.next().await? {
            Message::ReadyForQuery(_) => return Ok(()),
            Message::CommandComplete(_)
            | Message::EmptyQueryResponse
            | Message::RowDescription(_)
            | Message::DataRow(_) => {}
            _ => return Err(Error::unexpected_message()),
        }
    }
}

pub(crate) fn encode(client: &InnerClient, query: &str) -> Result<Bytes, Error> {
    client.with_buf(|buf| {
        frontend::query(query, buf).map_err(Error::encode)?;
        Ok(buf.split().freeze())
    })
}

pin_project! {
    /// A stream of simple query results.
    #[project(!Unpin)]
    pub struct SimpleQueryStream {
        responses: Responses,
        columns: Option<Arc<[SimpleColumn]>>,
    }
}

impl Stream for SimpleQueryStream {
    type Item = Result<SimpleQueryMessage, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        match ready!(this.responses.poll_next(cx)?) {
            Message::CommandComplete(body) => {
                let rows = extract_row_affected(&body)?;
                Poll::Ready(Some(Ok(SimpleQueryMessage::CommandComplete(rows))))
            }
            Message::EmptyQueryResponse => {
                Poll::Ready(Some(Ok(SimpleQueryMessage::CommandComplete(0))))
            }
            Message::RowDescription(body) => {
                let columns: Arc<[SimpleColumn]> = body
                    .fields()
                    .map(|f| Ok(SimpleColumn::new(f)))
                    .collect::<Vec<_>>()
                    .map_err(Error::parse)?
                    .into();

                *this.columns = Some(columns.clone());
                Poll::Ready(Some(Ok(SimpleQueryMessage::RowDescription(columns))))
            }
            Message::DataRow(body) => {
                let row = match &this.columns {
                    Some(columns) => SimpleQueryRow::new(columns.clone(), body)?,
                    None => return Poll::Ready(Some(Err(Error::unexpected_message()))),
                };
                Poll::Ready(Some(Ok(SimpleQueryMessage::Row(row))))
            }
            Message::ReadyForQuery(_) => Poll::Ready(None),
            _ => Poll::Ready(Some(Err(Error::unexpected_message()))),
        }
    }
}
