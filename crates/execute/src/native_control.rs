use std::{error::Error as StdError, fmt};

use futures_util::StreamExt;
use tokio_postgres::{CommandEvent, CommandEventStream, Error, TransactionState};

/// Expected observations for the owner's known internal transaction controls.
/// This is not SQL admission, connection ownership or savepoint identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeControl {
    Begin,
    Commit,
    Rollback,
    Savepoint,
    RollbackTo,
    Release,
    /// ROLLBACK TO followed by RELEASE of the owner's same savepoint.
    RecoverSavepoint,
}

impl NativeControl {
    pub fn expected_tags(self) -> &'static [&'static str] {
        match self {
            Self::Begin => &["BEGIN"],
            Self::Commit => &["COMMIT"],
            Self::Rollback | Self::RollbackTo => &["ROLLBACK"],
            Self::Savepoint => &["SAVEPOINT"],
            Self::Release => &["RELEASE"],
            Self::RecoverSavepoint => &["ROLLBACK", "RELEASE"],
        }
    }

    pub fn expected_state(self) -> TransactionState {
        match self {
            Self::Commit | Self::Rollback => TransactionState::Idle,
            Self::Begin
            | Self::Savepoint
            | Self::RollbackTo
            | Self::Release
            | Self::RecoverSavepoint => TransactionState::Transaction,
        }
    }
}

/// A fully observed expected control outcome. Only the checker constructs it.
/// It confirms this request's observations, not connection ownership or reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct NativeControlCompletion {
    control: NativeControl,
}

impl NativeControlCompletion {
    pub fn control(self) -> NativeControl {
        self.control
    }

    pub fn ready_state(self) -> TransactionState {
        self.control.expected_state()
    }
}

/// The first mismatch with an expected control. Tag positions are zero-based.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeControlMismatch {
    #[error("control stream was already consumed before checking the complete request")]
    AlreadyConsumed,
    #[error(
        "unexpected command tag at position {position}: expected {expected:?}, observed {actual:?}"
    )]
    Tag {
        position: usize,
        expected: Option<&'static str>,
        actual: String,
    },
    #[error("empty query at expected command position {position}")]
    EmptyQuery { position: usize },
    #[error("incomplete control tags: expected {expected}, observed {matched}")]
    MissingTags { expected: usize, matched: usize },
    #[error("unexpected final transaction state: expected {expected:?}, observed {actual:?}")]
    State {
        expected: TransactionState,
        actual: TransactionState,
    },
    #[error("control request ended without ReadyForQuery")]
    MissingReady,
}

/// Failure preserves SQL errors, terminal errors and observed request state.
/// An observed state is information for the owner, not a successful reset.
#[derive(Debug)]
pub struct NativeControlFailure {
    control: NativeControl,
    matched_tags: usize,
    ready_state: Option<TransactionState>,
    mismatch: Option<NativeControlMismatch>,
    backend_error: Option<Error>,
    stream_error: Option<Error>,
}

impl NativeControlFailure {
    pub fn control(&self) -> NativeControl {
        self.control
    }

    /// Number of expected prefix tags matched before any error or mismatch.
    /// For example, COMMIT followed by a terminal stream error retains one.
    pub fn matched_tags(&self) -> usize {
        self.matched_tags
    }

    /// This request's observed ReadyForQuery state, if confirmed.
    pub fn ready_state(&self) -> Option<TransactionState> {
        self.ready_state
    }

    pub fn mismatch(&self) -> Option<&NativeControlMismatch> {
        self.mismatch.as_ref()
    }

    pub fn backend_error(&self) -> Option<&Error> {
        self.backend_error.as_ref()
    }

    pub fn stream_error(&self) -> Option<&Error> {
        self.stream_error.as_ref()
    }
}

impl fmt::Display for NativeControlFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native {:?} completion failed", self.control)?;
        if let Some(source) = self.source() {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl StdError for NativeControlFailure {
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

/// Drain the supplied request and require its exact control tags and state.
///
/// SQL has already been queued. The caller supplies the expectation for known
/// internal SQL; this function does not authorize or submit frontend commands.
/// A previously consumed stream is rejected; any remaining tail is still drained.
/// A mismatch or SQL error still drains to readiness or a terminal stream error.
/// Dropping this future supplies no receipt and performs no owned recovery.
pub async fn check_native_control(
    control: NativeControl,
    mut events: CommandEventStream,
) -> Result<NativeControlCompletion, NativeControlFailure> {
    let expected_tags = control.expected_tags();
    let mut failure = NativeControlFailure {
        control,
        matched_tags: 0,
        ready_state: None,
        mismatch: events
            .has_yielded()
            .then_some(NativeControlMismatch::AlreadyConsumed),
        backend_error: None,
        stream_error: None,
    };
    while let Some(event) = events.next().await {
        match event {
            Ok(CommandEvent::CommandComplete(tag)) => {
                // Retain only the matched prefix and first mismatch. The
                // concrete connector stream supplies at most one SQL error.
                if failure.mismatch.is_none() && failure.backend_error.is_none() {
                    let expected = expected_tags.get(failure.matched_tags).copied();
                    if expected == Some(tag.as_str()) {
                        failure.matched_tags += 1;
                    } else {
                        failure.mismatch = Some(NativeControlMismatch::Tag {
                            position: failure.matched_tags,
                            expected,
                            actual: tag,
                        });
                    }
                }
            }
            Ok(CommandEvent::EmptyQuery) => {
                if failure.mismatch.is_none() && failure.backend_error.is_none() {
                    failure.mismatch = Some(NativeControlMismatch::EmptyQuery {
                        position: failure.matched_tags,
                    });
                }
            }
            Ok(CommandEvent::BackendError(error)) => failure.backend_error = Some(error),
            Ok(CommandEvent::ReadyForQuery(state)) => {
                failure.ready_state = Some(state);
                break;
            }
            Err(error) => {
                failure.stream_error = Some(error);
                break;
            }
        }
    }
    if failure.mismatch.is_none()
        && failure.backend_error.is_none()
        && failure.stream_error.is_none()
    {
        failure.mismatch = match failure.ready_state {
            None => Some(NativeControlMismatch::MissingReady),
            Some(_) if failure.matched_tags != expected_tags.len() => {
                Some(NativeControlMismatch::MissingTags {
                    expected: expected_tags.len(),
                    matched: failure.matched_tags,
                })
            }
            Some(actual) if actual != control.expected_state() => {
                Some(NativeControlMismatch::State {
                    expected: control.expected_state(),
                    actual,
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
        Ok(NativeControlCompletion { control })
    }
}
