use std::{error::Error as StdError, fmt, io};

use darmok_catalog::{
    CATALOG_REQUEST_MAX_BYTES, CATALOG_REQUEST_MAX_PAIRS, CatalogObservation,
    CatalogObservationError, NativeRelationName, decode_catalog_observation,
};
use futures_util::{Stream, StreamExt};
use tokio_postgres::{
    Error, SimpleQueryEvent, SimpleQueryEventStream, SimpleQueryRow, TransactionState,
};

use super::{NativeBackend, NativeBackendState, NativeScope, NativeScopeBoundary};

pub(super) const REQUEST_SETTING: &str = "darmok_server.catalog_request_v1";

#[derive(Debug, thiserror::Error)]
pub enum NativeCatalogError {
    #[error("catalog discovery requires a confirmed owned scope, observed {0:?}")]
    InvalidState(NativeBackendState),
    #[error("catalog discovery request exceeds its v1 byte or pair limit")]
    RequestLimit,
    #[error("catalog discovery request encoding failed: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("catalog discovery submission failed: {0}")]
    Submit(#[source] Error),
    #[error(transparent)]
    Completion(#[from] NativeCatalogFailure),
    #[error(transparent)]
    Observation(#[from] CatalogObservationError),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeCatalogMismatch {
    #[error("catalog stream was already consumed")]
    AlreadyConsumed,
    #[error("expected {expected}, observed {actual}")]
    Sequence {
        expected: &'static str,
        actual: &'static str,
    },
    #[error("unexpected catalog command tag: {0:?}")]
    Tag(String),
    #[error("catalog description is not one unmodified native TEXT column")]
    Description,
    #[error("catalog response is not one non-NULL text cell")]
    Row,
    #[error("catalog request ended without ReadyForQuery")]
    MissingReady,
    #[error("catalog request finished in {0:?}, expected Transaction")]
    State(TransactionState),
}

/// An unsuccessful observed request, including its actual readiness. This is
/// not a recovery receipt and does not make a partially received row usable.
#[derive(Debug)]
pub struct NativeCatalogFailure {
    matched_events: usize,
    ready_state: Option<TransactionState>,
    mismatch: Option<NativeCatalogMismatch>,
    backend_error: Option<Error>,
    stream_error: Option<Error>,
}

impl NativeCatalogFailure {
    pub fn matched_events(&self) -> usize {
        self.matched_events
    }
    pub fn ready_state(&self) -> Option<TransactionState> {
        self.ready_state
    }
    pub fn mismatch(&self) -> Option<&NativeCatalogMismatch> {
        self.mismatch.as_ref()
    }
    pub fn backend_error(&self) -> Option<&Error> {
        self.backend_error.as_ref()
    }
    pub fn stream_error(&self) -> Option<&Error> {
        self.stream_error.as_ref()
    }
}

impl fmt::Display for NativeCatalogFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native catalog completion failed")?;
        if let Some(source) = self.source() {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl StdError for NativeCatalogFailure {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        if let Some(error) = &self.backend_error {
            Some(error)
        } else if let Some(error) = &self.stream_error {
            Some(error)
        } else {
            self.mismatch.as_ref().map(|error| error as &dyn StdError)
        }
    }
}

fn request_sql(names: &[NativeRelationName<'_>]) -> Result<String, NativeCatalogError> {
    if names.len() > CATALOG_REQUEST_MAX_PAIRS {
        return Err(NativeCatalogError::RequestLimit);
    }
    let pairs: Vec<_> = names
        .iter()
        .map(|name| [name.schema_name, name.relation_name])
        .collect();
    let mut buffer = RequestBuffer {
        bytes: Vec::new(),
        overflow: false,
    };
    if let Err(error) = serde_json::to_writer(&mut buffer, &pairs) {
        return Err(if buffer.overflow {
            NativeCatalogError::RequestLimit
        } else {
            NativeCatalogError::Encoding(error)
        });
    }
    let json = String::from_utf8(buffer.bytes).expect("serde JSON strings are UTF8");
    // Escape the JSON bytes for an explicit E literal, independently of the
    // session's standard_conforming_strings. Inputs are literal names, not SQL.
    let mut sql = format!("SET LOCAL {REQUEST_SETTING} = E'");
    for character in json.chars() {
        if matches!(character, '\\' | '\'') {
            sql.push(character);
        }
        sql.push(character);
    }
    sql.push_str("'; SHOW ");
    sql.push_str(REQUEST_SETTING);
    Ok(sql)
}

struct RequestBuffer {
    bytes: Vec<u8>,
    overflow: bool,
}

impl io::Write for RequestBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > CATALOG_REQUEST_MAX_BYTES - self.bytes.len() {
            self.overflow = true;
            return Err(io::Error::other("catalog request limit"));
        }
        let required = self.bytes.len() + bytes.len();
        if required > self.bytes.capacity() {
            let capacity = required
                .max(self.bytes.capacity().saturating_mul(2))
                .min(CATALOG_REQUEST_MAX_BYTES);
            self.bytes.reserve_exact(capacity - self.bytes.len());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const EXPECTED: [&str; 5] = [
    "SET",
    "TEXT description",
    "one row",
    "SHOW",
    "ReadyForQuery",
];

pub(super) async fn check_request(
    events: SimpleQueryEventStream,
    prefix: &'static [&'static str],
) -> Result<SimpleQueryRow, NativeCatalogFailure> {
    let consumed = events.has_yielded();
    check_events(events, prefix, consumed).await
}

fn expected_event(prefix: &'static [&'static str], position: usize) -> &'static str {
    if position < prefix.len() {
        prefix[position]
    } else {
        EXPECTED
            .get(position - prefix.len())
            .copied()
            .unwrap_or("end of request")
    }
}

async fn check_events(
    mut events: impl Stream<Item = Result<SimpleQueryEvent, Error>> + Unpin,
    prefix: &'static [&'static str],
    consumed: bool,
) -> Result<SimpleQueryRow, NativeCatalogFailure> {
    let mut failure = NativeCatalogFailure {
        matched_events: 0,
        ready_state: None,
        mismatch: consumed.then_some(NativeCatalogMismatch::AlreadyConsumed),
        backend_error: None,
        stream_error: None,
    };
    let mut row = None;
    while let Some(event) = events.next().await {
        let event = match event {
            Ok(SimpleQueryEvent::BackendError(error)) => {
                failure.backend_error = Some(error);
                continue;
            }
            Ok(SimpleQueryEvent::ReadyForQuery(state)) => {
                failure.ready_state = Some(state);
                break;
            }
            Err(error) => {
                failure.stream_error = Some(error);
                break;
            }
            Ok(event) => event,
        };
        if failure.mismatch.is_some() || failure.backend_error.is_some() {
            continue;
        }
        let position = failure.matched_events;
        let actual = match &event {
            SimpleQueryEvent::CommandComplete(_) => "command tag",
            SimpleQueryEvent::RowDescription(_) => "row description",
            SimpleQueryEvent::Row(_) => "row",
            SimpleQueryEvent::EmptyQuery => "empty query",
            SimpleQueryEvent::BackendError(_) | SimpleQueryEvent::ReadyForQuery(_) => {
                unreachable!()
            }
        };
        let mismatch = if position < prefix.len() {
            match event {
                SimpleQueryEvent::CommandComplete(tag) if tag == prefix[position] => None,
                SimpleQueryEvent::CommandComplete(tag) => Some(NativeCatalogMismatch::Tag(tag)),
                _ => Some(NativeCatalogMismatch::Sequence {
                    expected: prefix[position],
                    actual,
                }),
            }
        } else {
            match (position - prefix.len(), event) {
                (0, SimpleQueryEvent::CommandComplete(tag)) if tag == "SET" => None,
                (3, SimpleQueryEvent::CommandComplete(tag)) if tag == "SHOW" => None,
                (0 | 3, SimpleQueryEvent::CommandComplete(tag)) => {
                    Some(NativeCatalogMismatch::Tag(tag))
                }
                (1, SimpleQueryEvent::RowDescription(columns)) => {
                    if columns.len() == 1
                        && columns[0].name() == REQUEST_SETTING
                        && columns[0].table_oid().is_none()
                        && columns[0].column_id().is_none()
                        && columns[0].type_oid() == 25
                        && columns[0].type_size() == -1
                        && columns[0].type_modifier() == -1
                        && columns[0].format() == 0
                    {
                        None
                    } else {
                        Some(NativeCatalogMismatch::Description)
                    }
                }
                (2, SimpleQueryEvent::Row(value)) => {
                    if value.len() == 1 && value.get(0).is_some() {
                        row = Some(value);
                        None
                    } else {
                        Some(NativeCatalogMismatch::Row)
                    }
                }
                _ => Some(NativeCatalogMismatch::Sequence {
                    expected: expected_event(prefix, position),
                    actual,
                }),
            }
        };
        if let Some(mismatch) = mismatch {
            failure.mismatch = Some(mismatch);
        } else {
            failure.matched_events += 1;
        }
    }
    if failure.mismatch.is_none()
        && failure.backend_error.is_none()
        && failure.stream_error.is_none()
    {
        failure.mismatch = match failure.ready_state {
            None => Some(NativeCatalogMismatch::MissingReady),
            Some(state) if state != TransactionState::Transaction => {
                Some(NativeCatalogMismatch::State(state))
            }
            Some(_) if failure.matched_events != prefix.len() + 4 => {
                Some(NativeCatalogMismatch::Sequence {
                    expected: expected_event(prefix, failure.matched_events),
                    actual: "ReadyForQuery",
                })
            }
            Some(_) => None,
        };
    }
    if failure.mismatch.is_some()
        || failure.backend_error.is_some()
        || failure.stream_error.is_some()
    {
        Err(failure)
    } else {
        Ok(row.expect("the complete observed sequence contains one row"))
    }
}

struct PendingRead<'a> {
    state: &'a mut NativeBackendState,
    finished: bool,
}

impl<'a> PendingRead<'a> {
    fn new(state: &'a mut NativeBackendState, boundary: NativeScopeBoundary) -> Self {
        *state = NativeBackendState::Discovering(boundary);
        Self {
            state,
            finished: false,
        }
    }

    fn complete(mut self, state: NativeBackendState) {
        *self.state = state;
        self.finished = true;
    }
}

impl Drop for PendingRead<'_> {
    fn drop(&mut self) {
        if !self.finished {
            *self.state = NativeBackendState::Uncertain;
        }
    }
}

impl NativeBackend {
    async fn discover_catalog(
        &mut self,
        boundary: NativeScopeBoundary,
        names: &[NativeRelationName<'_>],
    ) -> Result<Option<CatalogObservation>, NativeCatalogError> {
        if self.state != NativeBackendState::Scoped(boundary) {
            return Err(NativeCatalogError::InvalidState(self.state));
        }
        if names.is_empty() {
            return Ok(None);
        }
        let sql = request_sql(names)?;
        let pending = PendingRead::new(&mut self.state, boundary);
        let events = self
            .client
            .as_ref()
            .expect("live owner retains its client")
            .simple_query_events(&sql)
            .map_err(NativeCatalogError::Submit)?;
        let row = match check_request(events, &[]).await {
            Ok(row) => row,
            Err(failure) => {
                let recoverable = failure.mismatch.is_none()
                    && failure.stream_error.is_none()
                    && failure.ready_state == Some(TransactionState::FailedTransaction)
                    && matches!(
                        failure.backend_error.as_ref().and_then(Error::code),
                        Some(
                            &tokio_postgres::error::SqlState::UNDEFINED_TABLE
                                | &tokio_postgres::error::SqlState::T_R_SERIALIZATION_FAILURE
                        )
                    );
                pending.complete(if recoverable {
                    NativeBackendState::Scoped(boundary)
                } else {
                    NativeBackendState::Uncertain
                });
                return Err(failure.into());
            }
        };
        let observation =
            decode_catalog_observation(row.get(0).expect("checked non-NULL cell"), names)?;
        pending.complete(NativeBackendState::Scoped(boundary));
        Ok(Some(observation))
    }
}

impl NativeScope<'_> {
    /// Read immutable native facts without selecting the first data snapshot,
    /// under the documented continuous private-owner builtin native profile.
    /// There is no live fence after this request. Empty input performs no SQL
    /// and returns no stamp. Native guards, recheck and admission remain pending.
    pub async fn discover_catalog(
        &mut self,
        names: &[NativeRelationName<'_>],
    ) -> Result<Option<CatalogObservation>, NativeCatalogError> {
        self.backend
            .discover_catalog(self.boundary.kind(), names)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_names_are_json_encoded_then_e_literal_quoted() {
        let sql = request_sql(&[NativeRelationName {
            schema_name: "a'\\b",
            relation_name: "分析\n\"c",
        }])
        .unwrap();
        assert_eq!(
            sql,
            "SET LOCAL darmok_server.catalog_request_v1 = E'[[\"a''\\\\\\\\b\",\"分析\\\\n\\\\\"c\"]]'; SHOW darmok_server.catalog_request_v1"
        );
    }

    #[test]
    fn pending_read_without_completion_leaves_only_disposal() {
        let mut state = NativeBackendState::Scoped(NativeScopeBoundary::Transaction);
        drop(PendingRead::new(
            &mut state,
            NativeScopeBoundary::Transaction,
        ));
        assert_eq!(state, NativeBackendState::Uncertain);
    }

    #[test]
    fn request_limits_fail_before_backend_submission() {
        let names = vec![
            NativeRelationName {
                schema_name: "public",
                relation_name: "items"
            };
            CATALOG_REQUEST_MAX_PAIRS + 1
        ];
        assert!(matches!(
            request_sql(&names),
            Err(NativeCatalogError::RequestLimit)
        ));
        let oversized = "x".repeat(CATALOG_REQUEST_MAX_BYTES);
        assert!(matches!(
            request_sql(&[NativeRelationName {
                schema_name: "public",
                relation_name: &oversized
            }]),
            Err(NativeCatalogError::RequestLimit)
        ));
    }
    #[tokio::test]
    async fn fixed_setup_prefix_and_catalog_tail_reject_incomplete_or_malformed_events() {
        use SimpleQueryEvent::{
            CommandComplete as Tag, EmptyQuery, ReadyForQuery as Ready, RowDescription,
        };
        let prefix: &'static [&'static str] = &["BEGIN", "DO"];
        for (events, matched, expected) in [
            (
                vec![Tag("DO".into()), Ready(TransactionState::Transaction)],
                0,
                NativeCatalogMismatch::Tag("DO".into()),
            ),
            (
                vec![
                    Tag("BEGIN".into()),
                    EmptyQuery,
                    Ready(TransactionState::Transaction),
                ],
                1,
                NativeCatalogMismatch::Sequence {
                    expected: "DO",
                    actual: "empty query",
                },
            ),
            (
                vec![
                    Tag("BEGIN".into()),
                    Tag("DO".into()),
                    Ready(TransactionState::Transaction),
                ],
                2,
                NativeCatalogMismatch::Sequence {
                    expected: "SET",
                    actual: "ReadyForQuery",
                },
            ),
            (
                vec![
                    Tag("BEGIN".into()),
                    Tag("DO".into()),
                    Tag("SET".into()),
                    Ready(TransactionState::Transaction),
                ],
                3,
                NativeCatalogMismatch::Sequence {
                    expected: "TEXT description",
                    actual: "ReadyForQuery",
                },
            ),
            (
                vec![
                    Tag("BEGIN".into()),
                    Tag("DO".into()),
                    Tag("SET".into()),
                    RowDescription(Vec::new().into()),
                    Ready(TransactionState::Transaction),
                ],
                3,
                NativeCatalogMismatch::Description,
            ),
            (
                vec![
                    Tag("BEGIN".into()),
                    Tag("DO".into()),
                    Tag("COMMIT".into()),
                    Ready(TransactionState::Idle),
                ],
                2,
                NativeCatalogMismatch::Tag("COMMIT".into()),
            ),
            (
                vec![
                    Tag("BEGIN".into()),
                    Tag("DO".into()),
                    Ready(TransactionState::Idle),
                ],
                2,
                NativeCatalogMismatch::State(TransactionState::Idle),
            ),
            (
                vec![Tag("BEGIN".into()), Tag("DO".into())],
                2,
                NativeCatalogMismatch::MissingReady,
            ),
        ] {
            let stream = futures_util::stream::iter(events.into_iter().map(Ok));
            let failure = check_events(stream, prefix, false).await.unwrap_err();
            assert_eq!(failure.matched_events(), matched);
            assert_eq!(failure.mismatch(), Some(&expected));
            assert!(failure.backend_error().is_none() && failure.stream_error().is_none());
        }
        // Discovery's empty prefix still requires SET; setup cannot submit its
        // BEGIN/DO through the existing discovery-only expectation.
        let stream = futures_util::stream::iter([
            Ok(Tag("BEGIN".into())),
            Ok(Ready(TransactionState::Transaction)),
        ]);
        let failure = check_events(stream, &[], false).await.unwrap_err();
        assert_eq!(
            failure.mismatch(),
            Some(&NativeCatalogMismatch::Tag("BEGIN".into()))
        );
        let stream = futures_util::stream::iter([Ok(Ready(TransactionState::Transaction))]);
        let failure = check_events(stream, prefix, true).await.unwrap_err();
        assert_eq!(
            failure.mismatch(),
            Some(&NativeCatalogMismatch::AlreadyConsumed)
        );
    }
}
