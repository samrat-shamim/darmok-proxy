//! Darmok additions: describe a bound native portal before queuing Execute.

use crate::client::InnerClient;
use crate::codec::FrontendMessage;
use crate::completion::{TransactionState, builtin_columns};
use crate::connection::RequestMessages;
use crate::types::BorrowToSql;
use crate::{Column, Error, Portal, Statement, query};
use futures_util::future;
use postgres_protocol::message::{backend::Message, frontend};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{error::Error as StdError, fmt};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

/// A portal with an observed binary description and transaction readiness.
///
/// Bind, portal Describe and Sync completed without Execute being queued.
/// The receipt describes that request; connection ownership, semantic admission,
/// dependency validity and subsequent portal lifetime remain caller duties.
#[derive(Clone)]
pub struct DescribedPortal {
    pub(crate) portal: Portal,
    pub(crate) description: Statement,
    pub(crate) returns_rows: bool,
}

impl DescribedPortal {
    /// Native portal columns, including Some(empty) for a zero-column result.
    /// None is an observed NoData response, not an absent description.
    pub fn columns(&self) -> Option<&[Column]> {
        self.returns_rows.then(|| self.description.columns())
    }

    /// Whether this portal was bound from this exact prepared handle.
    /// Clones of the same handle match; matching column facts alone do not.
    pub fn is_bound_from(&self, statement: &Statement) -> bool {
        self.portal.statement().is_same(statement)
    }

    /// The successfully observed Bind/Describe request ended in a transaction.
    /// This is historical readiness, not a current connection state check.
    pub fn ready_state(&self) -> TransactionState {
        TransactionState::Transaction
    }
}

impl fmt::Debug for DescribedPortal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DescribedPortal")
            .field("columns", &self.columns())
            .field("ready_state", &self.ready_state())
            .finish_non_exhaustive()
    }
}

/// A mismatch with the fixed Bind/Describe/Sync response sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalBindMismatch {
    /// An expected response was missing or a different response arrived.
    Response {
        /// Expected response or phase.
        expected: &'static str,
    },
    /// A completed portal cannot survive outside an explicit transaction.
    State {
        /// The state actually observed at this request boundary.
        actual: TransactionState,
    },
}

impl fmt::Display for PortalBindMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Response { expected } => write!(f, "expected {expected} during portal binding"),
            Self::State { actual } => write!(
                f,
                "portal binding requires transaction readiness, observed {actual:?}"
            ),
        }
    }
}

impl StdError for PortalBindMismatch {}

/// Failure retains local/description, native SQL and terminal stream errors.
///
/// When possible, the submitted request is drained through ReadyForQuery even
/// after a rejected description. No failed result returns a usable portal.
/// Portal Drop may queue Close/Sync; this is not a cleanup or rollback receipt.
#[derive(Debug)]
pub struct PortalBindFailure {
    submitted: bool,
    bind_complete: bool,
    description_observed: bool,
    ready_state: Option<TransactionState>,
    mismatch: Option<PortalBindMismatch>,
    representation_error: Option<Error>,
    backend_error: Option<Error>,
    stream_error: Option<Error>,
}

impl PortalBindFailure {
    fn new() -> Self {
        Self {
            submitted: false,
            bind_complete: false,
            description_observed: false,
            ready_state: None,
            mismatch: None,
            representation_error: None,
            backend_error: None,
            stream_error: None,
        }
    }

    /// Whether the request was accepted by the local connection dispatcher.
    pub fn submitted(&self) -> bool {
        self.submitted
    }
    /// Whether this request reported BindComplete.
    pub fn bind_complete(&self) -> bool {
        self.bind_complete
    }
    /// Whether RowDescription or NoData was observed, even if rejected.
    pub fn description_observed(&self) -> bool {
        self.description_observed
    }
    /// Observed readiness for this request, not permission to reuse the client.
    pub fn ready_state(&self) -> Option<TransactionState> {
        self.ready_state
    }
    /// First response-sequence or readiness mismatch.
    pub fn mismatch(&self) -> Option<&PortalBindMismatch> {
        self.mismatch.as_ref()
    }
    /// Local parameter encoding or native description representation failure.
    pub fn representation_error(&self) -> Option<&Error> {
        self.representation_error.as_ref()
    }
    /// Native ErrorResponse retained separately from terminal stream errors.
    pub fn backend_error(&self) -> Option<&Error> {
        self.backend_error.as_ref()
    }
    /// A terminal response-stream error, with no confirmed request boundary.
    pub fn stream_error(&self) -> Option<&Error> {
        self.stream_error.as_ref()
    }

    fn failed(&self) -> bool {
        self.mismatch.is_some()
            || self.representation_error.is_some()
            || self.backend_error.is_some()
            || self.stream_error.is_some()
    }
}

impl fmt::Display for PortalBindFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native portal binding failed")?;
        if let Some(source) = self.source() {
            write!(f, ": {source}")?;
        }
        Ok(())
    }
}

impl StdError for PortalBindFailure {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.backend_error
            .as_ref()
            .map(|e| e as &dyn StdError)
            .or_else(|| {
                self.representation_error
                    .as_ref()
                    .map(|e| e as &dyn StdError)
            })
            .or_else(|| self.stream_error.as_ref().map(|e| e as &dyn StdError))
            .or_else(|| self.mismatch.as_ref().map(|e| e as &dyn StdError))
    }
}

pub(crate) async fn bind_described_builtin<P, I>(
    client: &Arc<InnerClient>,
    statement: Statement,
    params: I,
) -> Result<DescribedPortal, PortalBindFailure>
where
    P: BorrowToSql,
    I: IntoIterator<Item = P>,
    I::IntoIter: ExactSizeIterator,
{
    let mut failure = PortalBindFailure::new();
    let serial =
        match NEXT_ID.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1)) {
            Ok(serial) => serial,
            Err(_) => {
                failure.representation_error = Some(Error::encode(std::io::Error::other(
                    "portal identifiers exhausted",
                )));
                return Err(failure);
            }
        };
    let name = format!("darmok_portal_{serial}");
    let encoded = client.with_buf(|buf| {
        query::encode_bind(&statement, params, &name, buf)?;
        frontend::describe(b'P', &name, buf).map_err(Error::encode)?;
        frontend::sync(buf);
        Ok(buf.split().freeze())
    });
    let buf = match encoded {
        Ok(buf) => buf,
        Err(error) => {
            failure.representation_error = Some(error);
            return Err(failure);
        }
    };
    let mut responses = match client.send(RequestMessages::Single(FrontendMessage::Raw(buf))) {
        Ok(responses) => responses,
        Err(error) => {
            failure.stream_error = Some(error);
            return Err(failure);
        }
    };
    failure.submitted = true;
    // BindComplete establishes ownership of this portal name. Before that
    // observation a failed Bind must not close an existing conflicting portal.
    // An abandoned request has no cleanup receipt; its owner must recover/dispose.
    let mut source = Some(statement);
    let mut name = Some(name);
    let mut portal = None;
    let mut columns = None;
    let mut returns_rows = false;
    loop {
        let message = match future::poll_fn(|cx| responses.poll_next_raw(cx)).await {
            Ok(message) => message,
            Err(error) => {
                failure.stream_error = Some(error);
                break;
            }
        };
        match message {
            Message::ReadyForQuery(body) => {
                match TransactionState::from_status(body.status()) {
                    Ok(state) => {
                        failure.ready_state = Some(state);
                        if !failure.failed() {
                            if !failure.description_observed {
                                failure.mismatch = Some(PortalBindMismatch::Response {
                                    expected: if failure.bind_complete {
                                        "portal description"
                                    } else {
                                        "BindComplete"
                                    },
                                });
                            } else if state != TransactionState::Transaction {
                                failure.mismatch =
                                    Some(PortalBindMismatch::State { actual: state });
                            }
                        }
                    }
                    Err(error) => failure.stream_error = Some(error),
                }
                break;
            }
            Message::ErrorResponse(body) => {
                let error = Error::db(body);
                if error.as_db_error().is_some() {
                    if failure.backend_error.is_none() {
                        failure.backend_error = Some(error);
                    }
                } else if failure.representation_error.is_none() {
                    failure.representation_error = Some(error);
                }
            }
            _ if failure.failed() => {}
            Message::BindComplete if !failure.bind_complete => {
                failure.bind_complete = true;
                portal = Some(Portal::new(
                    client,
                    name.take().expect("first BindComplete owns its name"),
                    source.take().expect("first BindComplete owns its source"),
                ));
            }
            Message::RowDescription(body)
                if failure.bind_complete && !failure.description_observed =>
            {
                failure.description_observed = true;
                returns_rows = true;
                match builtin_columns(body, 1) {
                    Ok(observed) => columns = Some(observed),
                    Err(error) => failure.representation_error = Some(error),
                }
            }
            Message::NoData if failure.bind_complete && !failure.description_observed => {
                failure.description_observed = true;
                columns = Some(vec![]);
            }
            _ => {
                failure.mismatch = Some(PortalBindMismatch::Response {
                    expected: if !failure.bind_complete {
                        "BindComplete"
                    } else if !failure.description_observed {
                        "portal description"
                    } else {
                        "ReadyForQuery"
                    },
                })
            }
        }
    }
    if failure.failed() {
        return Err(failure);
    }
    Ok(DescribedPortal {
        portal: portal.expect("successful binding owns its portal"),
        description: Statement::unnamed(
            vec![],
            columns.expect("successful binding has a description"),
        ),
        returns_rows,
    })
}
