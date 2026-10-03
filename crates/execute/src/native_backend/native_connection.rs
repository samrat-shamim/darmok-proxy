//! Private connection mechanism shared by distinct command-history owners.
use std::error::Error as StdError;

use tokio::task::{JoinError, JoinHandle};
use tokio_postgres::{Client, Config, Error, Socket, TransactionState, tls::MakeTlsConnect};

use crate::{NativeControl, NativeControlFailure, NativeControlMismatch};

pub(super) struct NativeConnection {
    client: Option<Client>,
    driver: Option<JoinHandle<Result<(), Error>>>,
}

impl NativeConnection {
    pub(super) async fn connect<T>(config: &Config, connector: T) -> Result<Self, Error>
    where
        T: MakeTlsConnect<Socket>,
        T::Stream: Send + 'static,
    {
        let (client, connection) = config.connect(connector).await?;
        Ok(Self {
            client: Some(client),
            driver: Some(tokio::spawn(connection)),
        })
    }

    pub(super) fn client(&self) -> &Client {
        self.client.as_ref().expect("live owner retains its client")
    }

    pub(super) async fn dispose(&mut self) -> Result<(), DisposeSource> {
        self.client.take();
        let driver = self.driver.as_mut().expect("owner retains its driver");
        driver.abort();
        let outcome = driver.await;
        self.driver.take();
        match outcome {
            Ok(Ok(())) => Ok(()),
            Err(error) if error.is_cancelled() => Ok(()),
            Ok(Err(error)) => Err(DisposeSource::Backend(error)),
            Err(error) => Err(DisposeSource::Task(error)),
        }
    }
}

impl Drop for NativeConnection {
    fn drop(&mut self) {
        if let Some(driver) = &self.driver {
            driver.abort();
        }
    }
}

#[derive(Debug)]
pub(super) enum DisposeSource {
    Backend(Error),
    Task(JoinError),
}

impl DisposeSource {
    pub(super) fn as_error(&self) -> &(dyn StdError + 'static) {
        match self {
            Self::Backend(error) => error,
            Self::Task(error) => error,
        }
    }
}

/// Confirmed state after a failed fixed control, shared without sharing owners.
pub(super) fn confirmed_failure_state(failure: &NativeControlFailure) -> Option<TransactionState> {
    use TransactionState::{FailedTransaction, Idle, Transaction};

    if failure.stream_error().is_some() {
        return None;
    }
    // COMMIT-as-ROLLBACK remains an error, even though no transaction remains.
    if failure.control() == NativeControl::Commit
        && failure.ready_state() == Some(Idle)
        && failure.backend_error().is_none()
        && failure.matched_tags() == 0
        && matches!(failure.mismatch(), Some(NativeControlMismatch::Tag {
            position: 0, expected: Some("COMMIT"), actual,
        }) if actual == "ROLLBACK")
    {
        return Some(Idle);
    }
    if failure.mismatch().is_none() && failure.backend_error().is_some() {
        match (failure.control(), failure.ready_state()) {
            (NativeControl::Begin | NativeControl::Commit, Some(Idle)) => return Some(Idle),
            (NativeControl::Savepoint, Some(state @ (Transaction | FailedTransaction))) => {
                return Some(state);
            }
            _ => {}
        }
    }
    // Failed rollback or savepoint cleanup is never a reusable reset.
    None
}
