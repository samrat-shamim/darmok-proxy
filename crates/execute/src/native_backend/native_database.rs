use std::{error::Error as StdError, fmt};

use darmok_catalog::{CatalogObservationError, decode_catalog_observation};
use tokio_postgres::{Config, Error, Socket, TransactionState, tls::MakeTlsConnect};

use super::NativeCatalogFailure;
use super::native_catalog::{REQUEST_SETTING, check_request};
use super::native_connection::{DisposeSource, NativeConnection, confirmed_failure_state};
use crate::{NativeControl, NativeControlCompletion, NativeControlFailure, check_native_control};

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
    fn operation(self) -> NativeDatabaseSetupOperation {
        match self {
            Self::Initialize => NativeDatabaseSetupOperation::Initialize,
            Self::Verify => NativeDatabaseSetupOperation::Verify,
        }
    }

    fn access(self) -> &'static str {
        match self {
            Self::Initialize => "READ WRITE",
            Self::Verify => "READ ONLY",
        }
    }
}

/// Setup history belongs to its own connection and never to a query backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeDatabaseSetupState {
    Ready(TransactionState),
    Controlling(NativeControl),
    Checking(NativeDatabaseAction),
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeDatabaseSetupOperation {
    Initialize,
    Verify,
    Commit,
    Rollback,
}

#[derive(Debug, thiserror::Error)]
pub enum NativeDatabaseSetupError {
    #[error("native setup connection establishment failed: {0}")]
    Connect(#[source] Error),
    #[error("cannot {operation:?} while native database setup is {state:?}")]
    InvalidState {
        operation: NativeDatabaseSetupOperation,
        state: NativeDatabaseSetupState,
    },
    #[error("native setup {control:?} submission failed: {source}")]
    Submit {
        control: NativeControl,
        #[source]
        source: Error,
    },
    #[error(transparent)]
    Control(#[from] NativeControlFailure),
}

/// Owns a fresh connection exclusively for the fixed initialization and
/// verification manifests. It exposes no query scopes, raw SQL or conversion
/// into NativeBackend. Query work must establish a different connection.
///
/// Setup cannot be used as a query backend:
/// ```compile_fail
/// use darmok_execute::{NativeBackend, NativeDatabaseSetup};
/// fn reuse(setup: NativeDatabaseSetup) -> NativeBackend {
///     setup.into()
/// }
/// ```
/// Nor can it open a query scope:
/// ```compile_fail
/// use darmok_execute::{NativeDatabaseSetup, NativeTransactionSpec};
/// async fn query(setup: &mut NativeDatabaseSetup, spec: NativeTransactionSpec) {
///     let _ = setup.transaction_scope(spec).await;
/// }
/// ```
#[must_use]
pub struct NativeDatabaseSetup {
    connection: NativeConnection,
    state: NativeDatabaseSetupState,
}

impl fmt::Debug for NativeDatabaseSetup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeDatabaseSetup")
            .field("state", &self.state)
            .finish_non_exhaustive()
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
    Owner(#[from] NativeDatabaseSetupError),
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
            Self::Owner(NativeDatabaseSetupError::Connect(error)) | Self::Submit(error) => {
                Some(error)
            }
            Self::Owner(NativeDatabaseSetupError::Submit { source, .. }) => Some(source),
            Self::Owner(NativeDatabaseSetupError::Control(failure)) => failure.backend_error(),
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
    cleanup: Option<Result<NativeControlCompletion, NativeDatabaseSetupError>>,
}

impl NativeDatabaseFailure {
    pub fn action(&self) -> NativeDatabaseAction {
        self.action
    }

    pub fn original(&self) -> &NativeDatabaseError {
        &self.original
    }

    pub fn cleanup(&self) -> Option<&Result<NativeControlCompletion, NativeDatabaseSetupError>> {
        self.cleanup.as_ref()
    }
}

impl NativeDatabaseSetup {
    /// Retain a fresh connector-owned client and driver, then observe the same
    /// fixed idle lookup context as a query owner. No existing Client is adopted.
    pub async fn connect<T>(config: &Config, connector: T) -> Result<Self, NativeDatabaseSetupError>
    where
        T: MakeTlsConnect<Socket>,
        T::Stream: Send + 'static,
    {
        let connection = NativeConnection::connect(config, connector)
            .await
            .map_err(NativeDatabaseSetupError::Connect)?;
        let mut setup = Self {
            connection,
            state: NativeDatabaseSetupState::Uncertain,
        };
        let _ = setup
            .control(
                NativeControl::Initialize,
                "ROLLBACK; SET search_path = pg_catalog",
            )
            .await?;
        Ok(setup)
    }

    pub fn state(&self) -> NativeDatabaseSetupState {
        self.state
    }

    #[cfg(test)]
    pub(super) fn test_client(&self) -> &tokio_postgres::Client {
        self.connection.client()
    }

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
                NativeDatabaseSetupState::Ready(
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
            .connection
            .client()
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
                            NativeDatabaseSetupState::Ready(TransactionState::FailedTransaction)
                        }
                        Some(TransactionState::Idle) if failure.matched_events() == 0 => {
                            NativeDatabaseSetupState::Ready(TransactionState::Idle)
                        }
                        _ => NativeDatabaseSetupState::Uncertain,
                    }
                } else {
                    NativeDatabaseSetupState::Uncertain
                };
                pending.complete(state);
                return Err(failure.into());
            }
        };
        // The exact complete request confirmed Transaction. Decoder failure
        // can therefore roll back safely, unlike a malformed wire sequence.
        let decoded = decode_catalog_observation(row.get(0).expect("checked non-NULL cell"), &[]);
        pending.complete(NativeDatabaseSetupState::Ready(
            TransactionState::Transaction,
        ));
        let _ = decoded?;
        Ok(())
    }

    /// Stop and await only this setup owner's local driver. This does not
    /// resolve an uncertain commit or certify server rollback.
    pub async fn dispose(
        mut self,
    ) -> Result<NativeDatabaseSetupDisposed, NativeDatabaseSetupDisposeError> {
        let previous_state = self.state;
        self.connection
            .dispose()
            .await
            .map(|()| NativeDatabaseSetupDisposed { previous_state })
            .map_err(|source| NativeDatabaseSetupDisposeError {
                previous_state,
                source,
            })
    }

    fn require(
        &self,
        operation: NativeDatabaseSetupOperation,
        states: &[TransactionState],
    ) -> Result<(), NativeDatabaseSetupError> {
        if let NativeDatabaseSetupState::Ready(state) = self.state
            && states.contains(&state)
        {
            Ok(())
        } else {
            Err(NativeDatabaseSetupError::InvalidState {
                operation,
                state: self.state,
            })
        }
    }

    pub(super) async fn commit(
        &mut self,
    ) -> Result<NativeControlCompletion, NativeDatabaseSetupError> {
        self.require(
            NativeDatabaseSetupOperation::Commit,
            &[
                TransactionState::Transaction,
                TransactionState::FailedTransaction,
            ],
        )?;
        self.control(NativeControl::Commit, "COMMIT").await
    }

    pub(super) async fn rollback(
        &mut self,
    ) -> Result<NativeControlCompletion, NativeDatabaseSetupError> {
        self.require(
            NativeDatabaseSetupOperation::Rollback,
            &[
                TransactionState::Transaction,
                TransactionState::FailedTransaction,
            ],
        )?;
        self.control(NativeControl::Rollback, "ROLLBACK").await
    }

    pub(super) async fn control(
        &mut self,
        control: NativeControl,
        sql: &str,
    ) -> Result<NativeControlCompletion, NativeDatabaseSetupError> {
        let pending = PendingSetup::control(&mut self.state, control);
        let events = self
            .connection
            .client()
            .command_events(sql)
            .map_err(|source| NativeDatabaseSetupError::Submit { control, source })?;
        match check_native_control(control, events).await {
            Ok(completion) => {
                pending.complete(NativeDatabaseSetupState::Ready(completion.ready_state()));
                Ok(completion)
            }
            Err(failure) => {
                let state = confirmed_failure_state(&failure)
                    .map(NativeDatabaseSetupState::Ready)
                    .unwrap_or(NativeDatabaseSetupState::Uncertain);
                pending.complete(state);
                Err(failure.into())
            }
        }
    }
}

struct PendingSetup<'a> {
    state: &'a mut NativeDatabaseSetupState,
    finished: bool,
}

impl<'a> PendingSetup<'a> {
    fn new(state: &'a mut NativeDatabaseSetupState, action: NativeDatabaseAction) -> Self {
        *state = NativeDatabaseSetupState::Checking(action);
        Self {
            state,
            finished: false,
        }
    }

    fn control(state: &'a mut NativeDatabaseSetupState, control: NativeControl) -> Self {
        *state = NativeDatabaseSetupState::Controlling(control);
        Self {
            state,
            finished: false,
        }
    }

    fn complete(mut self, state: NativeDatabaseSetupState) {
        *self.state = state;
        self.finished = true;
    }
}

impl Drop for PendingSetup<'_> {
    fn drop(&mut self) {
        if !self.finished {
            *self.state = NativeDatabaseSetupState::Uncertain;
        }
    }
}

/// Confirms local setup-driver termination, retaining the preceding state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct NativeDatabaseSetupDisposed {
    previous_state: NativeDatabaseSetupState,
}

impl NativeDatabaseSetupDisposed {
    pub fn previous_state(self) -> NativeDatabaseSetupState {
        self.previous_state
    }
}

#[derive(Debug)]
pub struct NativeDatabaseSetupDisposeError {
    previous_state: NativeDatabaseSetupState,
    source: DisposeSource,
}

impl NativeDatabaseSetupDisposeError {
    pub fn previous_state(&self) -> NativeDatabaseSetupState {
        self.previous_state
    }
}

impl fmt::Display for NativeDatabaseSetupDisposeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native setup driver failed during disposal: {}",
            self.source.as_error()
        )
    }
}

impl StdError for NativeDatabaseSetupDisposeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.source.as_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_setup_has_no_cleanup_receipt() {
        let mut state = NativeDatabaseSetupState::Ready(TransactionState::Idle);
        drop(PendingSetup::new(
            &mut state,
            NativeDatabaseAction::Initialize,
        ));
        assert_eq!(state, NativeDatabaseSetupState::Uncertain);
    }
}
