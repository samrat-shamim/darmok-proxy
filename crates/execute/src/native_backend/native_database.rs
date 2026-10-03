use std::fmt;

use darmok_catalog::{CatalogObservationError, decode_catalog_observation};
use tokio_postgres::{Error, TransactionState};

use super::native_catalog::{REQUEST_SETTING, check_request};
use super::{
    NativeBackend, NativeBackendError, NativeBackendOperation, NativeBackendState,
    NativeCatalogFailure,
};
use crate::NativeControlCompletion;

pub(super) const DATABASE_SQL: &str = include_str!("../../sql/database-installation.sql");
const CREATE_MARKER: &str = "allow_create constant pg_catalog.bool := true;";

/// Functional compatibility format, distinct from the native utility protocol.
pub const NATIVE_SCHEMA_VERSION: i32 = 1;
/// Functional per-database extension version, not an artifact fingerprint.
pub const NATIVE_SERVER_EXTENSION_VERSION: &str = "1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeDatabaseAction {
    Initialize,
    Verify,
}

impl NativeDatabaseAction {
    fn operation(self) -> NativeBackendOperation {
        match self {
            Self::Initialize => NativeBackendOperation::InitializeDatabase,
            Self::Verify => NativeBackendOperation::VerifyDatabase,
        }
    }

    fn access(self) -> &'static str {
        match self {
            Self::Initialize => "READ WRITE",
            Self::Verify => "READ ONLY",
        }
    }
}

impl fmt::Display for NativeDatabaseAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Initialize => "database initialization",
            Self::Verify => "database verification",
        })
    }
}

/// Both functional manifests and the live empty native response were checked
/// before this separately observed COMMIT. No execution admission is inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct NativeDatabaseCompletion {
    action: NativeDatabaseAction,
    completion: NativeControlCompletion,
}

impl NativeDatabaseCompletion {
    pub fn action(self) -> NativeDatabaseAction {
        self.action
    }

    pub fn schema_version(self) -> i32 {
        NATIVE_SCHEMA_VERSION
    }

    pub fn server_extension_version(self) -> &'static str {
        NATIVE_SERVER_EXTENSION_VERSION
    }

    pub fn completion(self) -> NativeControlCompletion {
        self.completion
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NativeDatabaseError {
    #[error(transparent)]
    Owner(#[from] NativeBackendError),
    #[error("database setup submission failed: {0}")]
    Submit(#[source] Error),
    #[error(transparent)]
    Completion(#[from] NativeCatalogFailure),
    #[error(transparent)]
    Observation(#[from] CatalogObservationError),
}

impl NativeDatabaseError {
    pub fn backend_error(&self) -> Option<&Error> {
        match self {
            Self::Owner(NativeBackendError::Connect(error)) | Self::Submit(error) => Some(error),
            Self::Owner(NativeBackendError::Submit { source, .. }) => Some(source),
            Self::Owner(NativeBackendError::Control(failure)) => failure.backend_error(),
            Self::Completion(failure) => failure.backend_error(),
            Self::Owner(_) | Self::Observation(_) => None,
        }
    }
}

/// The original error and separately awaited rollback, if the complete setup
/// request established an owned transaction. An uncertain outcome is not reset.
#[derive(Debug, thiserror::Error)]
#[error("native {action} failed: {original}")]
pub struct NativeDatabaseFailure {
    action: NativeDatabaseAction,
    #[source]
    original: NativeDatabaseError,
    cleanup: Option<Result<NativeControlCompletion, NativeBackendError>>,
}

impl NativeDatabaseFailure {
    pub fn action(&self) -> NativeDatabaseAction {
        self.action
    }

    pub fn original(&self) -> &NativeDatabaseError {
        &self.original
    }

    pub fn cleanup(&self) -> Option<&Result<NativeControlCompletion, NativeBackendError>> {
        self.cleanup.as_ref()
    }
}

impl NativeBackend {
    /// Install or validate both reserved components in this physical database.
    /// The database and preloaded module must already exist. No repair occurs.
    pub async fn initialize_database(
        &mut self,
    ) -> Result<NativeDatabaseCompletion, NativeDatabaseFailure> {
        self.database_operation(NativeDatabaseAction::Initialize)
            .await
    }

    /// Check both manifests and the live native handler, without object creation.
    pub async fn verify_database(
        &mut self,
    ) -> Result<NativeDatabaseCompletion, NativeDatabaseFailure> {
        self.database_operation(NativeDatabaseAction::Verify).await
    }

    async fn database_operation(
        &mut self,
        action: NativeDatabaseAction,
    ) -> Result<NativeDatabaseCompletion, NativeDatabaseFailure> {
        // Check before any cleanup decision: an invalid caller transaction
        // must never be mistaken for a transaction owned by this operation.
        self.require(action.operation(), &[TransactionState::Idle])
            .map_err(|original| NativeDatabaseFailure {
                action,
                original: original.into(),
                cleanup: None,
            })?;
        let manifest = match action {
            NativeDatabaseAction::Initialize => DATABASE_SQL.to_owned(),
            NativeDatabaseAction::Verify => {
                assert_eq!(DATABASE_SQL.matches(CREATE_MARKER).count(), 1);
                DATABASE_SQL.replacen(
                    CREATE_MARKER,
                    "allow_create constant pg_catalog.bool := false;",
                    1,
                )
            }
        };
        if let Err(original) = self.check_database_installation(action, &manifest).await {
            let cleanup = if matches!(
                self.state,
                NativeBackendState::Ready(
                    TransactionState::Transaction | TransactionState::FailedTransaction
                )
            ) {
                Some(self.rollback().await)
            } else {
                None
            };
            return Err(NativeDatabaseFailure {
                action,
                original,
                cleanup,
            });
        }
        // This distinct request is submitted only after strict response decoding.
        self.commit()
            .await
            .map(|completion| NativeDatabaseCompletion { action, completion })
            .map_err(|original| NativeDatabaseFailure {
                action,
                original: original.into(),
                cleanup: None,
            })
    }

    // Private staging boundary also permits ordinary fixtures to inspect the
    // lock after checked SHOW. It exposes no public SQL or alternate manifest.
    pub(super) async fn check_database_installation(
        &mut self,
        action: NativeDatabaseAction,
        manifest: &str,
    ) -> Result<(), NativeDatabaseError> {
        self.require(action.operation(), &[TransactionState::Idle])?;
        let sql = format!(
            "BEGIN ISOLATION LEVEL READ COMMITTED {} NOT DEFERRABLE;\n{manifest}\nSET LOCAL {REQUEST_SETTING} = E'[]'; SHOW {REQUEST_SETTING}",
            action.access()
        );
        let pending = PendingSetup::new(&mut self.state, action);
        let events = self
            .client
            .as_ref()
            .expect("live owner retains its client")
            .simple_query_events(&sql)
            .map_err(NativeDatabaseError::Submit)?;
        let row = match check_request(events, &["BEGIN", "DO"]).await {
            Ok(row) => row,
            Err(failure) => {
                let state = if failure.mismatch().is_none()
                    && failure.stream_error().is_none()
                    && failure.backend_error().is_some()
                {
                    match failure.ready_state() {
                        Some(TransactionState::FailedTransaction) => {
                            NativeBackendState::Ready(TransactionState::FailedTransaction)
                        }
                        Some(TransactionState::Idle) if failure.matched_events() == 0 => {
                            NativeBackendState::Ready(TransactionState::Idle)
                        }
                        _ => NativeBackendState::Uncertain,
                    }
                } else {
                    NativeBackendState::Uncertain
                };
                pending.complete(state);
                return Err(failure.into());
            }
        };
        // The exact complete request confirmed Transaction. Decoder failure
        // can therefore roll back safely, unlike a malformed wire sequence.
        let decoded = decode_catalog_observation(row.get(0).expect("checked non-NULL cell"), &[]);
        pending.complete(NativeBackendState::Ready(TransactionState::Transaction));
        let _ = decoded?;
        Ok(())
    }
}

struct PendingSetup<'a> {
    state: &'a mut NativeBackendState,
    finished: bool,
}

impl<'a> PendingSetup<'a> {
    fn new(state: &'a mut NativeBackendState, action: NativeDatabaseAction) -> Self {
        *state = NativeBackendState::SettingUp(action);
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

impl Drop for PendingSetup<'_> {
    fn drop(&mut self) {
        if !self.finished {
            *self.state = NativeBackendState::Uncertain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_setup_has_no_cleanup_receipt() {
        let mut state = NativeBackendState::Ready(TransactionState::Idle);
        drop(PendingSetup::new(
            &mut state,
            NativeDatabaseAction::Initialize,
        ));
        assert_eq!(state, NativeBackendState::Uncertain);
    }
}
