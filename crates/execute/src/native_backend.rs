use std::{error::Error as StdError, fmt};

use tokio::task::{JoinError, JoinHandle};
use tokio_postgres::{Client, Config, Error, Socket, TransactionState, tls::MakeTlsConnect};

use crate::{
    NativeControl, NativeControlCompletion, NativeControlFailure, NativeControlMismatch,
    NativeTransactionSpec, check_native_control,
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
    StartTransactionScope,
    StartSavepointScope,
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

/// Recovery requested by the controller, independently of the native boundary.
/// This component does not classify frontend errors or choose their policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRecovery {
    /// Undo this statement, preserving earlier work in an explicit transaction.
    /// An owned autocommit scope has no earlier work and rolls back in full.
    Statement,
    /// Undo the entire native transaction, including work before this scope.
    Transaction,
}

/// Owns both halves of a newly established connection. It exposes no Client,
/// arbitrary SQL, GenericClient implementation, or caller-selected control SQL.
/// This component owns controls only; admission and the row executor are pending.
///
/// A scope excludes interleaved parent commands:
/// ```compile_fail
/// use darmok_execute::{
///     NativeBackend, NativeIsolation, NativeTransactionAccess, NativeTransactionSpec,
/// };
/// async fn interleave(backend: &mut NativeBackend) {
///     let spec = NativeTransactionSpec {
///         isolation: NativeIsolation::ReadCommitted,
///         access: NativeTransactionAccess::ReadWrite,
///     };
///     let scope = backend.transaction_scope(spec).await.unwrap();
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

    /// Start a native transaction with explicit characteristics. No frontend
    /// isolation mapping or ambient session defaults are inferred here.
    pub async fn begin(
        &mut self,
        spec: NativeTransactionSpec,
    ) -> Result<NativeControlCompletion, NativeBackendError> {
        self.require(NativeBackendOperation::Begin, &[TransactionState::Idle])?;
        self.control(NativeControl::Begin, spec.begin_sql()).await
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

    /// Start an owned transaction scope from confirmed idle. There is no
    /// statement execution method yet. A savepoint requires savepoint_scope;
    /// these options are never silently ignored inside an existing transaction.
    pub async fn transaction_scope(
        &mut self,
        spec: NativeTransactionSpec,
    ) -> Result<NativeScope<'_>, NativeBackendError> {
        self.require(
            NativeBackendOperation::StartTransactionScope,
            &[TransactionState::Idle],
        )?;
        let _ = self.control(NativeControl::Begin, spec.begin_sql()).await?;
        Ok(self.enter_scope(OwnedBoundary::Transaction))
    }

    /// Start a savepoint scope in the existing confirmed native transaction.
    /// Its characteristics remain those of its parent. The mutable borrow and
    /// state check also exclude parent reuse after forgetting a scope.
    pub async fn savepoint_scope(&mut self) -> Result<NativeScope<'_>, NativeBackendError> {
        self.require(
            NativeBackendOperation::StartSavepointScope,
            &[TransactionState::Transaction],
        )?;
        let serial = self
            .last_savepoint
            .checked_add(1)
            .ok_or(NativeBackendError::SavepointIdentifiersExhausted)?;
        // Never reuse a name, including after a failed creation attempt.
        self.last_savepoint = serial;
        let sql = format!("SAVEPOINT \"darmok_statement_{serial}\"");
        let _ = self.control(NativeControl::Savepoint, &sql).await?;
        Ok(self.enter_scope(OwnedBoundary::Savepoint(serial)))
    }

    fn enter_scope(&mut self, boundary: OwnedBoundary) -> NativeScope<'_> {
        self.state = NativeBackendState::Scoped(boundary.kind());
        NativeScope {
            backend: self,
            boundary,
            finished: false,
        }
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

    /// Recover according to the controller's explicit choice. Statement-local
    /// recovery requires both ROLLBACK TO and RELEASE of the owned savepoint.
    /// Whole-transaction recovery uses one ROLLBACK even inside a savepoint
    /// scope; success requires confirmed idle. The caller retains its original
    /// statement error separately from this cleanup result.
    pub async fn recover(
        mut self,
        recovery: NativeRecovery,
    ) -> Result<NativeControlCompletion, NativeBackendError> {
        let result = match (self.boundary, recovery) {
            (
                OwnedBoundary::Transaction,
                NativeRecovery::Statement | NativeRecovery::Transaction,
            )
            | (OwnedBoundary::Savepoint(_), NativeRecovery::Transaction) => {
                self.backend
                    .control(NativeControl::Rollback, "ROLLBACK")
                    .await
            }
            (OwnedBoundary::Savepoint(serial), NativeRecovery::Statement) => {
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
    mod query_results;
    mod set_controller;
    use std::{future::Future, pin::Pin, task::Context};

    use futures_util::task::noop_waker_ref;
    use tokio_postgres::{NoTls, error::SqlState};

    use super::*;
    use crate::{NativeIsolation, NativeTransactionAccess};

    const READ_COMMITTED_WRITE: NativeTransactionSpec = NativeTransactionSpec {
        isolation: NativeIsolation::ReadCommitted,
        access: NativeTransactionAccess::ReadWrite,
    };

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

    fn transaction_cases() -> [(
        NativeTransactionSpec,
        (&'static str, &'static str, &'static str),
    ); 6] {
        use NativeIsolation::{ReadCommitted, RepeatableRead, Serializable};
        use NativeTransactionAccess::{ReadOnly, ReadWrite};

        [
            (
                NativeTransactionSpec {
                    isolation: ReadCommitted,
                    access: ReadWrite,
                },
                ("read committed", "off", "off"),
            ),
            (
                NativeTransactionSpec {
                    isolation: ReadCommitted,
                    access: ReadOnly,
                },
                ("read committed", "on", "off"),
            ),
            (
                NativeTransactionSpec {
                    isolation: RepeatableRead,
                    access: ReadWrite,
                },
                ("repeatable read", "off", "off"),
            ),
            (
                NativeTransactionSpec {
                    isolation: RepeatableRead,
                    access: ReadOnly,
                },
                ("repeatable read", "on", "off"),
            ),
            (
                NativeTransactionSpec {
                    isolation: Serializable,
                    access: ReadWrite,
                },
                ("serializable", "off", "off"),
            ),
            (
                NativeTransactionSpec {
                    isolation: Serializable,
                    access: ReadOnly,
                },
                ("serializable", "on", "off"),
            ),
        ]
    }

    async fn transaction_modes(backend: &NativeBackend) -> (String, String, String) {
        let row = client(backend)
            .query_one(
                "SELECT current_setting('transaction_isolation'), current_setting('transaction_read_only'), current_setting('transaction_deferrable')",
                &[],
            )
            .await
            .unwrap();
        (row.get(0), row.get(1), row.get(2))
    }

    fn assert_modes(actual: (String, String, String), expected: (&str, &str, &str)) {
        assert_eq!(
            (actual.0.as_str(), actual.1.as_str(), actual.2.as_str()),
            expected
        );
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn explicit_begin_and_transaction_scopes_override_ambient_characteristics() {
        let mut backend = connect_backend().await;
        setup(
            &backend,
            "SET default_transaction_isolation = 'serializable'; SET default_transaction_read_only = on; SET default_transaction_deferrable = on",
        )
        .await;
        for (spec, expected) in transaction_cases() {
            let completion = backend.begin(spec).await.unwrap();
            assert_eq!(completion.control(), NativeControl::Begin);
            assert_eq!(completion.ready_state(), TransactionState::Transaction);
            assert_modes(transaction_modes(&backend).await, expected);
            let _ = backend.rollback().await.unwrap();

            let scope = backend.transaction_scope(spec).await.unwrap();
            assert_eq!(scope.boundary(), NativeScopeBoundary::Transaction);
            assert_modes(transaction_modes(scope.backend).await, expected);
            let completion = scope.finish().await.unwrap();
            assert_eq!(completion.control(), NativeControl::Commit);
            assert_eq!(completion.ready_state(), TransactionState::Idle);
            assert_eq!(
                backend.state(),
                NativeBackendState::Ready(TransactionState::Idle)
            );
        }
        // BEGIN's explicit modes do not change this connection's defaults.
        for (name, expected) in [
            ("default_transaction_isolation", "serializable"),
            ("default_transaction_read_only", "on"),
            ("default_transaction_deferrable", "on"),
        ] {
            let row = client(&backend)
                .query_one(&format!("SHOW {name}"), &[])
                .await
                .unwrap();
            assert_eq!(row.get::<_, String>(0), expected);
        }
        let _ = backend.dispose().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn savepoint_scopes_preserve_parent_characteristics_and_require_a_transaction() {
        let mut backend = connect_backend().await;
        for (spec, expected) in transaction_cases() {
            assert!(matches!(
                backend.savepoint_scope().await,
                Err(NativeBackendError::InvalidState {
                    operation: NativeBackendOperation::StartSavepointScope,
                    state: NativeBackendState::Ready(TransactionState::Idle),
                })
            ));
            let _ = backend.begin(spec).await.unwrap();
            assert!(matches!(
                backend.transaction_scope(READ_COMMITTED_WRITE).await,
                Err(NativeBackendError::InvalidState {
                    operation: NativeBackendOperation::StartTransactionScope,
                    state: NativeBackendState::Ready(TransactionState::Transaction),
                })
            ));
            assert_modes(transaction_modes(&backend).await, expected);

            let scope = backend.savepoint_scope().await.unwrap();
            assert_eq!(scope.boundary(), NativeScopeBoundary::Savepoint);
            assert_modes(transaction_modes(scope.backend).await, expected);
            let completion = scope.recover(NativeRecovery::Statement).await.unwrap();
            assert_eq!(completion.control(), NativeControl::RecoverSavepoint);
            assert_eq!(completion.ready_state(), TransactionState::Transaction);
            assert_modes(transaction_modes(&backend).await, expected);

            let scope = backend.savepoint_scope().await.unwrap();
            assert_modes(transaction_modes(scope.backend).await, expected);
            let completion = scope.finish().await.unwrap();
            assert_eq!(completion.control(), NativeControl::Release);
            assert_eq!(completion.ready_state(), TransactionState::Transaction);
            assert_modes(transaction_modes(&backend).await, expected);
            let _ = backend.rollback().await.unwrap();
        }
        let _ = backend.dispose().await.unwrap();
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
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
        assert_eq!(scope.boundary(), NativeScopeBoundary::Transaction);
        setup(scope.backend, "INSERT INTO owned_values VALUES (1), (2)").await;
        let complete = scope.finish().await.unwrap();
        assert_eq!(complete.control(), NativeControl::Commit);
        assert_eq!(values(&backend).await, [1, 2]);
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (3)").await;
        assert_eq!(
            scope
                .recover(NativeRecovery::Statement)
                .await
                .unwrap()
                .control(),
            NativeControl::Rollback
        );
        assert_eq!(values(&backend).await, [1, 2]);
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        setup(&backend, "INSERT INTO owned_values VALUES (4)").await;
        let _ = backend.rollback().await.unwrap();
        assert_eq!(values(&backend).await, [1, 2]);
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
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
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        setup(
            &backend,
            "INSERT INTO owned_values VALUES (1); SAVEPOINT client_named",
        )
        .await;
        for serial in 1..=3 {
            let scope = backend.savepoint_scope().await.unwrap();
            assert_eq!(scope.boundary(), NativeScopeBoundary::Savepoint);
            assert!(matches!(scope.boundary, OwnedBoundary::Savepoint(id) if id == serial));
            setup(scope.backend, "INSERT INTO owned_values VALUES (2)").await;
            let error = client(scope.backend)
                .execute("INSERT INTO owned_values VALUES (-1)", &[])
                .await
                .unwrap_err();
            assert_eq!(error.code(), Some(&SqlState::CHECK_VIOLATION));
            assert_eq!(
                scope
                    .recover(NativeRecovery::Statement)
                    .await
                    .unwrap()
                    .control(),
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
        let scope = backend.savepoint_scope().await.unwrap();
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
        let scope = backend.savepoint_scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (4)").await;
        let _ = scope.finish().await.unwrap();
        let _ = backend.commit().await.unwrap();
        assert_eq!(values(&backend).await, [1, 4]);
        let _ = backend.dispose().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn transaction_recovery_discards_prior_work_and_all_savepoint_identities() {
        let mut backend = connect_backend().await;
        setup(
            &backend,
            "CREATE TEMP TABLE owned_values (n integer); INSERT INTO owned_values VALUES (1)",
        )
        .await;
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        setup(
            &backend,
            "INSERT INTO owned_values VALUES (2); SAVEPOINT client_named",
        )
        .await;
        let scope = backend.savepoint_scope().await.unwrap();
        assert!(matches!(scope.boundary, OwnedBoundary::Savepoint(1)));
        setup(
            scope.backend,
            "INSERT INTO owned_values VALUES (3); SAVEPOINT client_later",
        )
        .await;
        let completion = scope.recover(NativeRecovery::Transaction).await.unwrap();
        assert_eq!(completion.control(), NativeControl::Rollback);
        assert_eq!(completion.ready_state(), TransactionState::Idle);
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
        assert_eq!(values(&backend).await, [1]);

        // None of the client or internal savepoints survives full rollback.
        for name in ["client_named", "client_later", "darmok_statement_1"] {
            let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
            let error = client(&backend)
                .batch_execute(&format!("ROLLBACK TO SAVEPOINT \"{name}\""))
                .await
                .unwrap_err();
            assert_eq!(error.code(), Some(&SqlState::from_code("3B001")));
            let _ = backend.rollback().await.unwrap();
        }

        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        let scope = backend.savepoint_scope().await.unwrap();
        // Full recovery removes the scope, but never reuses its identity.
        assert!(matches!(scope.boundary, OwnedBoundary::Savepoint(2)));
        setup(scope.backend, "INSERT INTO owned_values VALUES (4)").await;
        let _ = scope.finish().await.unwrap();
        let _ = backend.commit().await.unwrap();
        assert_eq!(values(&backend).await, [1, 4]);
        let _ = backend.dispose().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn transaction_recovery_after_native_error_confirms_idle_and_next_scope() {
        for outer in [false, true] {
            let mut backend = connect_backend().await;
            setup(
                &backend,
                "CREATE TEMP TABLE owned_values (n integer CHECK (n > 0)); INSERT INTO owned_values VALUES (1)",
            )
            .await;
            if outer {
                let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
                setup(&backend, "INSERT INTO owned_values VALUES (2)").await;
            }
            let scope = if outer {
                backend.savepoint_scope().await.unwrap()
            } else {
                backend
                    .transaction_scope(READ_COMMITTED_WRITE)
                    .await
                    .unwrap()
            };
            setup(scope.backend, "INSERT INTO owned_values VALUES (3)").await;
            let original = client(scope.backend)
                .execute("INSERT INTO owned_values VALUES (-1)", &[])
                .await
                .unwrap_err();
            let aborted = client(scope.backend)
                .execute("SELECT 1", &[])
                .await
                .unwrap_err();
            assert_eq!(aborted.code(), Some(&SqlState::IN_FAILED_SQL_TRANSACTION));
            let completion = scope.recover(NativeRecovery::Transaction).await.unwrap();
            // Recovery is a separate result, not a replacement statement error.
            assert_eq!(original.code(), Some(&SqlState::CHECK_VIOLATION));
            assert_eq!(completion.control(), NativeControl::Rollback);
            assert_eq!(completion.ready_state(), TransactionState::Idle);
            assert_eq!(values(&backend).await, [1]);
            let scope = backend
                .transaction_scope(READ_COMMITTED_WRITE)
                .await
                .unwrap();
            assert_eq!(scope.boundary(), NativeScopeBoundary::Transaction);
            setup(scope.backend, "INSERT INTO owned_values VALUES (4)").await;
            let _ = scope.finish().await.unwrap();
            assert_eq!(values(&backend).await, [1, 4]);
            let _ = backend.dispose().await.unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn transaction_recovery_does_not_depend_on_an_extant_statement_savepoint() {
        let mut backend = connect_backend().await;
        setup(
            &backend,
            "CREATE TEMP TABLE owned_values (n integer); INSERT INTO owned_values VALUES (1)",
        )
        .await;
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        setup(
            &backend,
            "INSERT INTO owned_values VALUES (2); SAVEPOINT client_named",
        )
        .await;
        let scope = backend.savepoint_scope().await.unwrap();
        setup(
            scope.backend,
            "INSERT INTO owned_values VALUES (3); ROLLBACK TO SAVEPOINT client_named",
        )
        .await;
        // Native ROLLBACK TO an earlier savepoint removes the internal one.
        // The explicit full choice must not first attempt local recovery.
        let completion = scope.recover(NativeRecovery::Transaction).await.unwrap();
        assert_eq!(completion.control(), NativeControl::Rollback);
        assert_eq!(completion.ready_state(), TransactionState::Idle);
        assert_eq!(values(&backend).await, [1]);
        let _ = backend.dispose().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn abandoned_transaction_recovery_never_restores_ready() {
        for outer in [false, true] {
            for polled in [false, true] {
                let mut backend = connect_backend().await;
                if outer {
                    let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
                }
                let scope = if outer {
                    backend.savepoint_scope().await.unwrap()
                } else {
                    backend
                        .transaction_scope(READ_COMMITTED_WRITE)
                        .await
                        .unwrap()
                };
                let mut future = Box::pin(scope.recover(NativeRecovery::Transaction));
                if polled {
                    pending(future.as_mut());
                }
                drop(future);
                // Even an unpolled consuming future drops its unfinished scope.
                // No transport interruption or asynchronous Drop is assumed.
                assert_eq!(backend.state(), NativeBackendState::Uncertain);
                assert!(matches!(
                    backend.begin(READ_COMMITTED_WRITE).await,
                    Err(NativeBackendError::InvalidState { .. })
                ));
                assert!(matches!(
                    backend.rollback().await,
                    Err(NativeBackendError::InvalidState { .. })
                ));
                assert!(matches!(
                    backend.transaction_scope(READ_COMMITTED_WRITE).await,
                    Err(NativeBackendError::InvalidState { .. })
                ));
                assert_eq!(
                    backend.dispose().await.unwrap().previous_state(),
                    NativeBackendState::Uncertain
                );
            }
        }
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
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        setup(&backend, "INSERT INTO owned_values VALUES (1)").await;
        let scope = backend.savepoint_scope().await.unwrap();
        let error = client(scope.backend)
            .prepare("SELECT missing_column FROM owned_values")
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::UNDEFINED_COLUMN));
        let _ = scope.recover(NativeRecovery::Statement).await.unwrap();
        assert_eq!(values(&backend).await, [1]);
        let scope = backend.savepoint_scope().await.unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (2)").await;
        let _ = scope.finish().await.unwrap();
        let _ = backend.commit().await.unwrap();
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
        let error = client(scope.backend)
            .execute("INSERT INTO owned_values VALUES (-1)", &[])
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::CHECK_VIOLATION));
        let _ = scope.recover(NativeRecovery::Statement).await.unwrap();
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
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
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
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
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
        setup(scope.backend, "INSERT INTO owned_values VALUES (2)").await;
        let _ = scope.finish().await.unwrap();
        assert_eq!(values(&backend).await, [2]);
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
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
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
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
        assert!(matches!(
            backend.savepoint_scope().await,
            Err(NativeBackendError::InvalidState {
                operation: NativeBackendOperation::StartSavepointScope,
                ..
            })
        ));
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        assert!(matches!(
            backend.begin(READ_COMMITTED_WRITE).await,
            Err(NativeBackendError::InvalidState {
                operation: NativeBackendOperation::Begin,
                ..
            })
        ));
        assert!(matches!(
            backend.transaction_scope(READ_COMMITTED_WRITE).await,
            Err(NativeBackendError::InvalidState {
                operation: NativeBackendOperation::StartTransactionScope,
                ..
            })
        ));
        let scope = backend.savepoint_scope().await.unwrap();
        std::mem::forget(scope);
        assert_eq!(
            backend.state(),
            NativeBackendState::Scoped(NativeScopeBoundary::Savepoint)
        );
        assert!(matches!(
            backend.savepoint_scope().await,
            Err(NativeBackendError::InvalidState {
                operation: NativeBackendOperation::StartSavepointScope,
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
        drop(backend.begin(READ_COMMITTED_WRITE));
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
        let mut future = Box::pin(backend.begin(READ_COMMITTED_WRITE));
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
            backend.transaction_scope(READ_COMMITTED_WRITE).await,
            Err(NativeBackendError::InvalidState { .. })
        ));
        assert_eq!(
            backend.dispose().await.unwrap().previous_state(),
            NativeBackendState::Uncertain
        );
        for outer in [false, true] {
            let mut backend = connect_backend().await;
            if outer {
                let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
            }
            drop(async {
                if outer {
                    backend.savepoint_scope().await
                } else {
                    backend.transaction_scope(READ_COMMITTED_WRITE).await
                }
            });
            assert!(matches!(backend.state(), NativeBackendState::Ready(_)));
            let mut future = Box::pin(async {
                if outer {
                    backend.savepoint_scope().await
                } else {
                    backend.transaction_scope(READ_COMMITTED_WRITE).await
                }
            });
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
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
        drop(scope);
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        let _ = backend.dispose().await.unwrap();
        let mut backend = connect_backend().await;
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
        drop(scope.finish());
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        let _ = backend.dispose().await.unwrap();
        let mut backend = connect_backend().await;
        let scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
        let mut future = Box::pin(scope.finish());
        pending(future.as_mut());
        drop(future);
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        let _ = backend.dispose().await.unwrap();
        for recover in [false, true] {
            let mut backend = connect_backend().await;
            let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
            let scope = backend.savepoint_scope().await.unwrap();
            if recover {
                let mut future = Box::pin(scope.recover(NativeRecovery::Statement));
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
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        let error = client(&backend)
            .execute("SELECT 1 / 0", &[])
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::DIVISION_BY_ZERO));
        let error = match backend.savepoint_scope().await {
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
            backend.savepoint_scope().await,
            Err(NativeBackendError::InvalidState { .. })
        ));
        let _ = backend.rollback().await.unwrap();
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        let _ = backend.commit().await.unwrap();
        let _ = backend.dispose().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
    async fn failed_savepoint_cleanup_preserves_source_and_requires_disposal() {
        let mut backend = connect_backend().await;
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        let scope = backend.savepoint_scope().await.unwrap();
        setup(scope.backend, "RELEASE SAVEPOINT \"darmok_statement_1\"").await;
        let error = scope.recover(NativeRecovery::Statement).await.unwrap_err();
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
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        let scope = backend.savepoint_scope().await.unwrap();
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
