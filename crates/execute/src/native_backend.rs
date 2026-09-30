use std::{error::Error as StdError, fmt};

use tokio::task::{JoinError, JoinHandle};
use tokio_postgres::{Client, Config, Error, Socket, TransactionState, tls::MakeTlsConnect};

use crate::{
    NativeControl, NativeControlCompletion, NativeControlFailure, NativeControlMismatch,
    check_native_control,
};

/// Known lifecycle state under this owner's exclusive SQL submission boundary.
/// Ready is a confirmed request observation, not a promise of future connectivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeBackendState {
    Ready(TransactionState),
    Controlling(NativeControl),
    Scoped(NativeScopeBoundary),
    /// An unfinished operation or unconfirmed cleanup permits only disposal.
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeBackendOperation {
    Begin,
    Commit,
    Rollback,
    StartScope,
}

#[derive(Debug, thiserror::Error)]
pub enum NativeBackendError {
    #[error("native connection establishment failed: {0}")]
    Connect(#[source] Error),
    #[error("cannot {operation:?} while native backend is {state:?}")]
    InvalidState {
        operation: NativeBackendOperation,
        state: NativeBackendState,
    },
    #[error("internal statement savepoint identifiers exhausted")]
    SavepointIdentifiersExhausted,
    #[error("native {control:?} submission failed: {source}")]
    Submit {
        control: NativeControl,
        #[source]
        source: Error,
    },
    #[error(transparent)]
    Control(#[from] NativeControlFailure),
}

/// The boundary being held open. Savepoints belong to an explicit outer
/// transaction; Transaction is an owned autocommit statement transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeScopeBoundary {
    Transaction,
    Savepoint,
}

/// Owns both halves of a newly established connection. It exposes no Client,
/// arbitrary SQL, GenericClient implementation, or caller-selected control SQL.
/// This component owns controls only; admission and the row executor are pending.
///
/// A scope excludes interleaved parent commands:
/// ```compile_fail
/// use darmok_execute::NativeBackend;
/// async fn interleave(backend: &mut NativeBackend) {
///     let scope = backend.scope().await.unwrap();
///     backend.commit().await.unwrap();
///     scope.finish().await.unwrap();
/// }
/// ```
#[must_use]
pub struct NativeBackend {
    client: Option<Client>,
    driver: Option<JoinHandle<Result<(), Error>>>,
    state: NativeBackendState,
    last_savepoint: u64,
}

impl NativeBackend {
    /// Forward an existing connector to tokio-postgres, retain both connection
    /// halves privately, then confirm ROLLBACK / idle before exposing the owner.
    /// This adds one control round trip per connection, not per statement.
    /// No arbitrary existing Client can be adopted.
    pub async fn connect<T>(config: &Config, connector: T) -> Result<Self, NativeBackendError>
    where
        T: MakeTlsConnect<Socket>,
        T::Stream: Send + 'static,
    {
        let (client, connection) = config
            .connect(connector)
            .await
            .map_err(NativeBackendError::Connect)?;
        let mut backend = Self {
            client: Some(client),
            driver: Some(tokio::spawn(connection)),
            state: NativeBackendState::Uncertain,
            last_savepoint: 0,
        };
        let _ = backend.control(NativeControl::Rollback, "ROLLBACK").await?;
        Ok(backend)
    }

    pub fn state(&self) -> NativeBackendState {
        self.state
    }

    pub async fn begin(&mut self) -> Result<NativeControlCompletion, NativeBackendError> {
        self.require(NativeBackendOperation::Begin, &[TransactionState::Idle])?;
        self.control(NativeControl::Begin, "BEGIN").await
    }

    pub async fn commit(&mut self) -> Result<NativeControlCompletion, NativeBackendError> {
        self.require(
            NativeBackendOperation::Commit,
            &[
                TransactionState::Transaction,
                TransactionState::FailedTransaction,
            ],
        )?;
        self.control(NativeControl::Commit, "COMMIT").await
    }

    pub async fn rollback(&mut self) -> Result<NativeControlCompletion, NativeBackendError> {
        self.require(
            NativeBackendOperation::Rollback,
            &[
                TransactionState::Transaction,
                TransactionState::FailedTransaction,
            ],
        )?;
        self.control(NativeControl::Rollback, "ROLLBACK").await
    }

    /// Start a control scope before future admission/preparation work. There is
    /// no statement execution method yet. The mutable borrow excludes another
    /// command; the state check also excludes reuse after forgetting a scope.
    pub async fn scope(&mut self) -> Result<NativeScope<'_>, NativeBackendError> {
        self.require(
            NativeBackendOperation::StartScope,
            &[TransactionState::Idle, TransactionState::Transaction],
        )?;
        let boundary = match self.state {
            NativeBackendState::Ready(TransactionState::Idle) => {
                let _ = self.control(NativeControl::Begin, "BEGIN").await?;
                OwnedBoundary::Transaction
            }
            NativeBackendState::Ready(TransactionState::Transaction) => {
                let serial = self
                    .last_savepoint
                    .checked_add(1)
                    .ok_or(NativeBackendError::SavepointIdentifiersExhausted)?;
                // Never reuse a name, including after a failed creation attempt.
                self.last_savepoint = serial;
                let sql = format!("SAVEPOINT \"darmok_statement_{serial}\"");
                let _ = self.control(NativeControl::Savepoint, &sql).await?;
                OwnedBoundary::Savepoint(serial)
            }
            _ => unreachable!("require established a ready scope state"),
        };
        self.state = NativeBackendState::Scoped(boundary.kind());
        Ok(NativeScope {
            backend: self,
            boundary,
            finished: false,
        })
    }

    /// Stop and await the owned local driver. Disposal does not prove rollback
    /// or resolve an uncertain commit. Dropping this future still aborts the
    /// driver through the owner's Drop implementation.
    pub async fn dispose(mut self) -> Result<NativeBackendDisposed, NativeBackendDisposeError> {
        let previous_state = self.state;
        self.client.take();
        let driver = self.driver.as_mut().expect("owner retains its driver");
        driver.abort();
        let outcome = driver.await;
        self.driver.take();
        match outcome {
            Ok(Ok(())) => Ok(NativeBackendDisposed { previous_state }),
            Err(error) if error.is_cancelled() => Ok(NativeBackendDisposed { previous_state }),
            Ok(Err(error)) => Err(NativeBackendDisposeError {
                previous_state,
                source: DisposeSource::Backend(error),
            }),
            Err(error) => Err(NativeBackendDisposeError {
                previous_state,
                source: DisposeSource::Task(error),
            }),
        }
    }

    fn require(
        &self,
        operation: NativeBackendOperation,
        states: &[TransactionState],
    ) -> Result<(), NativeBackendError> {
        if let NativeBackendState::Ready(state) = self.state
            && states.contains(&state)
        {
            Ok(())
        } else {
            Err(NativeBackendError::InvalidState {
                operation,
                state: self.state,
            })
        }
    }

    async fn control(
        &mut self,
        control: NativeControl,
        sql: &str,
    ) -> Result<NativeControlCompletion, NativeBackendError> {
        let pending = PendingControl::new(&mut self.state, control);
        let events = self
            .client
            .as_ref()
            .expect("live owner retains its client")
            .command_events(sql)
            .map_err(|source| NativeBackendError::Submit { control, source })?;
        match check_native_control(control, events).await {
            Ok(completion) => {
                pending.complete(NativeBackendState::Ready(completion.ready_state()));
                Ok(completion)
            }
            Err(failure) => {
                pending.complete(failure_state(&failure));
                Err(failure.into())
            }
        }
    }
}

impl Drop for NativeBackend {
    fn drop(&mut self) {
        if let Some(driver) = &self.driver {
            driver.abort();
        }
    }
}

impl fmt::Debug for NativeBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeBackend")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

struct PendingControl<'a> {
    state: &'a mut NativeBackendState,
    finished: bool,
}

impl<'a> PendingControl<'a> {
    fn new(state: &'a mut NativeBackendState, control: NativeControl) -> Self {
        *state = NativeBackendState::Controlling(control);
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

impl Drop for PendingControl<'_> {
    fn drop(&mut self) {
        if !self.finished {
            *self.state = NativeBackendState::Uncertain;
        }
    }
}

fn failure_state(failure: &NativeControlFailure) -> NativeBackendState {
    use TransactionState::{FailedTransaction, Idle, Transaction};

    if failure.stream_error().is_some() {
        return NativeBackendState::Uncertain;
    }
    // COMMIT-as-ROLLBACK is an observed failed commit with no transaction left.
    // It is never a successful commit receipt.
    if failure.control() == NativeControl::Commit
        && failure.ready_state() == Some(Idle)
        && failure.backend_error().is_none()
        && failure.matched_tags() == 0
        && matches!(failure.mismatch(), Some(NativeControlMismatch::Tag {
            position: 0, expected: Some("COMMIT"), actual,
        }) if actual == "ROLLBACK")
    {
        return NativeBackendState::Ready(Idle);
    }
    if failure.mismatch().is_none() && failure.backend_error().is_some() {
        match (failure.control(), failure.ready_state()) {
            (NativeControl::Begin | NativeControl::Commit, Some(Idle)) => {
                return NativeBackendState::Ready(Idle);
            }
            (NativeControl::Savepoint, Some(state @ (Transaction | FailedTransaction))) => {
                return NativeBackendState::Ready(state);
            }
            _ => {}
        }
    }
    // Failed rollback or savepoint cleanup never constitutes a reusable reset.
    NativeBackendState::Uncertain
}

#[derive(Debug, Clone, Copy)]
enum OwnedBoundary {
    Transaction,
    Savepoint(u64),
}

impl OwnedBoundary {
    fn kind(self) -> NativeScopeBoundary {
        match self {
            Self::Transaction => NativeScopeBoundary::Transaction,
            Self::Savepoint(_) => NativeScopeBoundary::Savepoint,
        }
    }
}

/// A borrowed control scope, not an admitted statement or validated row result.
/// Dropping it leaves the owner uncertain; no asynchronous cleanup is assumed.
#[must_use]
pub struct NativeScope<'a> {
    backend: &'a mut NativeBackend,
    boundary: OwnedBoundary,
    finished: bool,
}

impl NativeScope<'_> {
    pub fn boundary(&self) -> NativeScopeBoundary {
        self.boundary.kind()
    }

    /// Finish this control scope. Future row execution must retain this scope
    /// internally until its own output validation has completed.
    pub async fn finish(mut self) -> Result<NativeControlCompletion, NativeBackendError> {
        let result = match self.boundary {
            OwnedBoundary::Transaction => {
                self.backend.control(NativeControl::Commit, "COMMIT").await
            }
            OwnedBoundary::Savepoint(serial) => {
                let sql = format!("RELEASE SAVEPOINT \"darmok_statement_{serial}\"");
                self.backend.control(NativeControl::Release, &sql).await
            }
        };
        // An observed failed COMMIT in idle has also removed the whole scope.
        self.finished = result.is_ok()
            || self.backend.state == NativeBackendState::Ready(TransactionState::Idle);
        result
    }

    /// Recover this control scope. A savepoint recovery requires both ROLLBACK
    /// TO and RELEASE of the same generated identity. The caller retains its
    /// original statement error separately from this cleanup result.
    pub async fn recover(mut self) -> Result<NativeControlCompletion, NativeBackendError> {
        let result = match self.boundary {
            OwnedBoundary::Transaction => {
                self.backend
                    .control(NativeControl::Rollback, "ROLLBACK")
                    .await
            }
            OwnedBoundary::Savepoint(serial) => {
                let sql = format!(
                    "ROLLBACK TO SAVEPOINT \"darmok_statement_{serial}\"; RELEASE SAVEPOINT \"darmok_statement_{serial}\""
                );
                self.backend
                    .control(NativeControl::RecoverSavepoint, &sql)
                    .await
            }
        };
        self.finished = result.is_ok();
        result
    }
}

impl Drop for NativeScope<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.backend.state = NativeBackendState::Uncertain;
        }
    }
}

/// Confirms only that this owner's local driver has stopped and released its
/// connection. The preceding transaction outcome is unchanged by disposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct NativeBackendDisposed {
    previous_state: NativeBackendState,
}

impl NativeBackendDisposed {
    pub fn previous_state(self) -> NativeBackendState {
        self.previous_state
    }
}

#[derive(Debug)]
pub struct NativeBackendDisposeError {
    previous_state: NativeBackendState,
    source: DisposeSource,
}

impl NativeBackendDisposeError {
    pub fn previous_state(&self) -> NativeBackendState {
        self.previous_state
    }
}

#[derive(Debug)]
enum DisposeSource {
    Backend(Error),
    Task(JoinError),
}

impl fmt::Display for NativeBackendDisposeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native backend driver failed during disposal: {}",
            self.source().expect("disposal error has a source")
        )
    }
}

impl StdError for NativeBackendDisposeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(match &self.source {
            DisposeSource::Backend(error) => error,
            DisposeSource::Task(error) => error,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{future::Future, pin::Pin, task::Context};

    use futures_util::task::noop_waker_ref;
    use tokio_postgres::{NoTls, error::SqlState};

    use super::*;

    // These are required database unit fixtures, invoked explicitly in both
    // PostgreSQL CI jobs. Ordinary workspace tests do not have a database.
    // Fixture SQL stays inside this private module; it is not an execution API.
    async fn connect_backend() -> NativeBackend {
        let url = std::env::var("DARMOK_TEST_DATABASE_URL")
            .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL database");
        let config = url.parse::<Config>().unwrap();
        NativeBackend::connect(&config, NoTls).await.unwrap()
    }

    fn client(backend: &NativeBackend) -> &Client {
        backend.client.as_ref().unwrap()
    }

    async fn setup(backend: &NativeBackend, sql: &str) {
        client(backend).batch_execute(sql).await.unwrap();
    }

    async fn values(backend: &NativeBackend) -> Vec<i32> {
        client(backend)
            .query("SELECT n FROM owned_values ORDER BY n", &[])
            .await
            .unwrap()
            .iter()
            .map(|row| row.get(0))
            .collect()
    }

    fn control_failure(error: &NativeBackendError) -> &NativeControlFailure {
        match error {
            NativeBackendError::Control(failure) => failure,
            other => panic!("expected control failure, got {other:?}"),
        }
    }

    fn pending<F: Future>(future: Pin<&mut F>) {
        let mut context = Context::from_waker(noop_waker_ref());
        assert!(future.poll(&mut context).is_pending());
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn owned_transactions_and_autocommit_scopes_have_confirmed_effects() {
        let mut backend = connect_backend().await;
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
        setup(&backend, "CREATE TEMP TABLE owned_values (n integer)").await;
        let scope = backend.scope().await.unwrap();
        assert_eq!(scope.boundary(), NativeScopeBoundary::Transaction);
        setup(scope.backend, "INSERT INTO owned_values VALUES (1), (2)").await;
        let complete = scope.finish().await.unwrap();
        assert_eq!(complete.control(), NativeControl::Commit);
        assert_eq!(values(&backend).await, [1, 2]);
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (3)").await;
        assert_eq!(
            scope.recover().await.unwrap().control(),
            NativeControl::Rollback
        );
        assert_eq!(values(&backend).await, [1, 2]);
        let _ = backend.begin().await.unwrap();
        setup(&backend, "INSERT INTO owned_values VALUES (4)").await;
        let _ = backend.rollback().await.unwrap();
        assert_eq!(values(&backend).await, [1, 2]);
        let _ = backend.begin().await.unwrap();
        setup(&backend, "INSERT INTO owned_values VALUES (5)").await;
        let _ = backend.commit().await.unwrap();
        assert_eq!(values(&backend).await, [1, 2, 5]);
        assert_eq!(
            backend.dispose().await.unwrap().previous_state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn repeated_statement_savepoints_recover_and_release_their_own_identity() {
        let mut backend = connect_backend().await;
        setup(
            &backend,
            "CREATE TEMP TABLE owned_values (n integer CHECK (n > 0))",
        )
        .await;
        let _ = backend.begin().await.unwrap();
        setup(
            &backend,
            "INSERT INTO owned_values VALUES (1); SAVEPOINT client_named",
        )
        .await;
        for serial in 1..=3 {
            let scope = backend.scope().await.unwrap();
            assert_eq!(scope.boundary(), NativeScopeBoundary::Savepoint);
            assert!(matches!(scope.boundary, OwnedBoundary::Savepoint(id) if id == serial));
            setup(scope.backend, "INSERT INTO owned_values VALUES (2)").await;
            let error = client(scope.backend)
                .execute("INSERT INTO owned_values VALUES (-1)", &[])
                .await
                .unwrap_err();
            assert_eq!(error.code(), Some(&SqlState::CHECK_VIOLATION));
            assert_eq!(
                scope.recover().await.unwrap().control(),
                NativeControl::RecoverSavepoint
            );
            assert_eq!(values(&backend).await, [1]);
            // A failed RELEASE proves that the internal scope was removed.
            // Recover via the earlier, separately named client savepoint.
            let error = client(&backend)
                .batch_execute(&format!("RELEASE SAVEPOINT \"darmok_statement_{serial}\""))
                .await
                .unwrap_err();
            assert_eq!(error.code(), Some(&SqlState::from_code("3B001")));
            setup(&backend, "ROLLBACK TO SAVEPOINT client_named").await;
        }
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (3)").await;
        assert_eq!(
            scope.finish().await.unwrap().control(),
            NativeControl::Release
        );
        setup(
            &backend,
            "ROLLBACK TO SAVEPOINT client_named; RELEASE SAVEPOINT client_named",
        )
        .await;
        assert_eq!(values(&backend).await, [1]);
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (4)").await;
        let _ = scope.finish().await.unwrap();
        let _ = backend.commit().await.unwrap();
        assert_eq!(values(&backend).await, [1, 4]);
        let _ = backend.dispose().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn prepare_error_and_autocommit_error_recover_before_next_scope() {
        let mut backend = connect_backend().await;
        setup(
            &backend,
            "CREATE TEMP TABLE owned_values (n integer CHECK (n > 0))",
        )
        .await;
        let _ = backend.begin().await.unwrap();
        setup(&backend, "INSERT INTO owned_values VALUES (1)").await;
        let scope = backend.scope().await.unwrap();
        let error = client(scope.backend)
            .prepare("SELECT missing_column FROM owned_values")
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::UNDEFINED_COLUMN));
        let _ = scope.recover().await.unwrap();
        assert_eq!(values(&backend).await, [1]);
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (2)").await;
        let _ = scope.finish().await.unwrap();
        let _ = backend.commit().await.unwrap();
        let scope = backend.scope().await.unwrap();
        let error = client(scope.backend)
            .execute("INSERT INTO owned_values VALUES (-1)", &[])
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::CHECK_VIOLATION));
        let _ = scope.recover().await.unwrap();
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (3)").await;
        let _ = scope.finish().await.unwrap();
        assert_eq!(values(&backend).await, [1, 2, 3]);
        let _ = backend.dispose().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn failed_commit_retains_error_and_confirmed_idle_without_success() {
        let mut backend = connect_backend().await;
        setup(
            &backend,
            "CREATE TEMP TABLE owned_values (n integer UNIQUE DEFERRABLE INITIALLY DEFERRED)",
        )
        .await;
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (1), (1)").await;
        let error = scope.finish().await.unwrap_err();
        let failure = control_failure(&error);
        assert_eq!(
            failure.backend_error().unwrap().code(),
            Some(&SqlState::UNIQUE_VIOLATION)
        );
        assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
        assert!(values(&backend).await.is_empty());
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (2)").await;
        let _ = scope.finish().await.unwrap();
        assert_eq!(values(&backend).await, [2]);
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (3)").await;
        let error = client(scope.backend)
            .execute("SELECT 1 / 0", &[])
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::DIVISION_BY_ZERO));
        let error = scope.finish().await.unwrap_err();
        assert!(
            matches!(control_failure(&error).mismatch(), Some(NativeControlMismatch::Tag { actual, .. }) if actual == "ROLLBACK")
        );
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
        assert_eq!(values(&backend).await, [2]);
        let _ = backend.begin().await.unwrap();
        let error = client(&backend)
            .execute("SELECT 1 / 0", &[])
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::DIVISION_BY_ZERO));
        let error = backend.commit().await.unwrap_err();
        assert!(
            matches!(control_failure(&error).mismatch(), Some(NativeControlMismatch::Tag { actual, .. }) if actual == "ROLLBACK")
        );
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
        let _ = backend.dispose().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn invalid_control_states_do_not_queue_commands() {
        let mut backend = connect_backend().await;
        assert!(matches!(
            backend.commit().await,
            Err(NativeBackendError::InvalidState {
                operation: NativeBackendOperation::Commit,
                ..
            })
        ));
        assert!(matches!(
            backend.rollback().await,
            Err(NativeBackendError::InvalidState {
                operation: NativeBackendOperation::Rollback,
                ..
            })
        ));
        let _ = backend.begin().await.unwrap();
        assert!(matches!(
            backend.begin().await,
            Err(NativeBackendError::InvalidState {
                operation: NativeBackendOperation::Begin,
                ..
            })
        ));
        let scope = backend.scope().await.unwrap();
        std::mem::forget(scope);
        assert_eq!(
            backend.state(),
            NativeBackendState::Scoped(NativeScopeBoundary::Savepoint)
        );
        assert!(matches!(
            backend.scope().await,
            Err(NativeBackendError::InvalidState {
                operation: NativeBackendOperation::StartScope,
                ..
            })
        ));
        assert!(matches!(
            backend.commit().await,
            Err(NativeBackendError::InvalidState { .. })
        ));
        assert_eq!(
            backend.dispose().await.unwrap().previous_state(),
            NativeBackendState::Scoped(NativeScopeBoundary::Savepoint)
        );
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn unpolled_control_is_inert_and_dropped_pending_control_is_uncertain() {
        let mut backend = connect_backend().await;
        drop(backend.begin());
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
        let mut future = Box::pin(backend.begin());
        // A current-thread test does not poll the background driver during this
        // synchronous first poll. No network fault or forced interruption.
        pending(future.as_mut());
        drop(future);
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        assert!(matches!(
            backend.rollback().await,
            Err(NativeBackendError::InvalidState { .. })
        ));
        assert!(matches!(
            backend.scope().await,
            Err(NativeBackendError::InvalidState { .. })
        ));
        assert_eq!(
            backend.dispose().await.unwrap().previous_state(),
            NativeBackendState::Uncertain
        );
        for outer in [false, true] {
            let mut backend = connect_backend().await;
            if outer {
                let _ = backend.begin().await.unwrap();
            }
            drop(backend.scope());
            assert!(matches!(backend.state(), NativeBackendState::Ready(_)));
            let mut future = Box::pin(backend.scope());
            pending(future.as_mut());
            drop(future);
            assert_eq!(backend.state(), NativeBackendState::Uncertain);
            let _ = backend.dispose().await.unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn dropped_scope_and_dropped_finish_never_restore_ready() {
        let mut backend = connect_backend().await;
        let scope = backend.scope().await.unwrap();
        drop(scope);
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        let _ = backend.dispose().await.unwrap();
        let mut backend = connect_backend().await;
        let scope = backend.scope().await.unwrap();
        drop(scope.finish());
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        let _ = backend.dispose().await.unwrap();
        let mut backend = connect_backend().await;
        let scope = backend.scope().await.unwrap();
        let mut future = Box::pin(scope.finish());
        pending(future.as_mut());
        drop(future);
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        let _ = backend.dispose().await.unwrap();
        for recover in [false, true] {
            let mut backend = connect_backend().await;
            let _ = backend.begin().await.unwrap();
            let scope = backend.scope().await.unwrap();
            if recover {
                let mut future = Box::pin(scope.recover());
                pending(future.as_mut());
                drop(future);
            } else {
                let mut future = Box::pin(scope.finish());
                pending(future.as_mut());
                drop(future);
            }
            assert_eq!(backend.state(), NativeBackendState::Uncertain);
            let _ = backend.dispose().await.unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn savepoint_creation_error_observes_failed_outer_transaction() {
        let mut backend = connect_backend().await;
        let _ = backend.begin().await.unwrap();
        let error = client(&backend)
            .execute("SELECT 1 / 0", &[])
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::DIVISION_BY_ZERO));
        let error = match backend.scope().await {
            Err(error) => error,
            Ok(_) => panic!("failed outer transaction must not create a scope"),
        };
        assert_eq!(
            control_failure(&error).backend_error().unwrap().code(),
            Some(&SqlState::IN_FAILED_SQL_TRANSACTION)
        );
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::FailedTransaction)
        );
        assert!(matches!(
            backend.scope().await,
            Err(NativeBackendError::InvalidState { .. })
        ));
        let _ = backend.rollback().await.unwrap();
        let _ = backend.begin().await.unwrap();
        let _ = backend.commit().await.unwrap();
        let _ = backend.dispose().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn failed_savepoint_cleanup_preserves_source_and_requires_disposal() {
        let mut backend = connect_backend().await;
        let _ = backend.begin().await.unwrap();
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "RELEASE SAVEPOINT \"darmok_statement_1\"").await;
        let error = scope.recover().await.unwrap_err();
        let failure = control_failure(&error);
        assert_eq!(
            failure.backend_error().unwrap().code(),
            Some(&SqlState::from_code("3B001"))
        );
        assert_eq!(
            failure.ready_state(),
            Some(TransactionState::FailedTransaction)
        );
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        assert!(matches!(
            backend.rollback().await,
            Err(NativeBackendError::InvalidState { .. })
        ));
        let _ = backend.dispose().await.unwrap();
        let mut backend = connect_backend().await;
        let _ = backend.begin().await.unwrap();
        let scope = backend.scope().await.unwrap();
        setup(scope.backend, "RELEASE SAVEPOINT \"darmok_statement_1\"").await;
        let error = scope.finish().await.unwrap_err();
        assert_eq!(
            control_failure(&error).backend_error().unwrap().code(),
            Some(&SqlState::from_code("3B001"))
        );
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        let _ = backend.dispose().await.unwrap();
    }
}
