use std::error::Error as StdError;

use darmok_execute::{
    NativeControl, NativeControlCompletion, NativeControlFailure, NativeControlMismatch,
    check_native_control,
};
use tokio_postgres::{Client, Error, NoTls, TransactionState};

async fn client() -> (Client, tokio::task::JoinHandle<Result<(), Error>>) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

async fn control(
    client: &Client,
    sql: &str,
    expected: NativeControl,
) -> Result<NativeControlCompletion, NativeControlFailure> {
    check_native_control(expected, client.command_events(sql).unwrap()).await
}

async fn success(client: &Client, sql: &str, expected: NativeControl) {
    let completion = control(client, sql, expected).await.unwrap();
    assert_eq!(completion.control(), expected);
    assert_eq!(completion.ready_state(), expected.expected_state());
}

fn backend_failure(
    failure: &NativeControlFailure,
    expected: NativeControl,
    matched_tags: usize,
    code: &str,
    state: TransactionState,
) {
    assert_eq!(failure.control(), expected);
    assert_eq!(failure.matched_tags(), matched_tags);
    assert_eq!(failure.ready_state(), Some(state));
    assert!(failure.mismatch().is_none());
    assert!(failure.stream_error().is_none());
    let error = failure.backend_error().unwrap();
    assert_eq!(error.code().unwrap().code(), code);
    let source = failure.source().unwrap().downcast_ref::<Error>().unwrap();
    assert!(std::ptr::eq(source, error));
}

async fn values(client: &Client, table: &str) -> Vec<i32> {
    client
        .query(&format!("SELECT n FROM {table} ORDER BY n"), &[])
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}

#[tokio::test]
async fn individual_controls_confirm_expected_tags_states_and_table_effects() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE controls_success (n integer)")
        .await
        .unwrap();
    success(&client, "BEGIN", NativeControl::Begin).await;
    client
        .execute("INSERT INTO controls_success VALUES (1)", &[])
        .await
        .unwrap();
    success(
        &client,
        "SAVEPOINT statement_scope",
        NativeControl::Savepoint,
    )
    .await;
    client
        .execute("INSERT INTO controls_success VALUES (2)", &[])
        .await
        .unwrap();
    success(
        &client,
        "ROLLBACK TO SAVEPOINT statement_scope",
        NativeControl::RollbackTo,
    )
    .await;
    assert_eq!(values(&client, "controls_success").await, [1]);
    success(
        &client,
        "RELEASE SAVEPOINT statement_scope",
        NativeControl::Release,
    )
    .await;
    client
        .execute("INSERT INTO controls_success VALUES (3)", &[])
        .await
        .unwrap();
    success(&client, "COMMIT", NativeControl::Commit).await;
    assert_eq!(values(&client, "controls_success").await, [1, 3]);
    success(&client, "BEGIN", NativeControl::Begin).await;
    client
        .execute("INSERT INTO controls_success VALUES (4)", &[])
        .await
        .unwrap();
    success(&client, "ROLLBACK", NativeControl::Rollback).await;
    assert_eq!(values(&client, "controls_success").await, [1, 3]);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn commit_completing_as_rollback_is_failure_with_confirmed_idle_state() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE controls_aborted (n integer CHECK (n > 0))")
        .await
        .unwrap();
    success(&client, "BEGIN", NativeControl::Begin).await;
    client
        .execute("INSERT INTO controls_aborted VALUES (1)", &[])
        .await
        .unwrap();
    let error = client
        .execute("INSERT INTO controls_aborted VALUES (-1)", &[])
        .await
        .unwrap_err();
    assert_eq!(error.code().unwrap().code(), "23514");
    let failure = control(&client, "COMMIT", NativeControl::Commit)
        .await
        .unwrap_err();
    assert_eq!(failure.control(), NativeControl::Commit);
    assert_eq!(failure.matched_tags(), 0);
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert!(failure.backend_error().is_none() && failure.stream_error().is_none());
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::Tag {
            position: 0,
            expected: Some("COMMIT"),
            actual: "ROLLBACK".to_owned(),
        })
    );
    assert!(
        failure
            .source()
            .unwrap()
            .downcast_ref::<NativeControlMismatch>()
            .is_some()
    );
    assert!(values(&client, "controls_aborted").await.is_empty());
    success(&client, "BEGIN", NativeControl::Begin).await;
    client
        .execute("INSERT INTO controls_aborted VALUES (2)", &[])
        .await
        .unwrap();
    success(&client, "COMMIT", NativeControl::Commit).await;
    assert_eq!(values(&client, "controls_aborted").await, [2]);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn deferred_commit_error_keeps_sql_error_without_fabricating_missing_tags() {
    let (client, connection) = client().await;
    client
        .batch_execute(
            "CREATE TEMP TABLE controls_deferred \
             (n integer UNIQUE DEFERRABLE INITIALLY DEFERRED)",
        )
        .await
        .unwrap();
    success(&client, "BEGIN", NativeControl::Begin).await;
    client
        .execute("INSERT INTO controls_deferred VALUES (1), (1)", &[])
        .await
        .unwrap();
    let failure = control(&client, "COMMIT", NativeControl::Commit)
        .await
        .unwrap_err();
    backend_failure(
        &failure,
        NativeControl::Commit,
        0,
        "23505",
        TransactionState::Idle,
    );
    assert!(values(&client, "controls_deferred").await.is_empty());
    success(&client, "BEGIN", NativeControl::Begin).await;
    client
        .execute("INSERT INTO controls_deferred VALUES (1)", &[])
        .await
        .unwrap();
    success(&client, "COMMIT", NativeControl::Commit).await;
    assert_eq!(values(&client, "controls_deferred").await, [1]);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn repeated_recovery_releases_internal_savepoints_and_keeps_client_scope() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE controls_recovery (n integer CHECK (n > 0))")
        .await
        .unwrap();
    success(&client, "BEGIN", NativeControl::Begin).await;
    client
        .execute("INSERT INTO controls_recovery VALUES (1)", &[])
        .await
        .unwrap();
    success(&client, "SAVEPOINT client_scope", NativeControl::Savepoint).await;
    for value in [2_i32, 3, 4] {
        success(
            &client,
            "SAVEPOINT statement_scope",
            NativeControl::Savepoint,
        )
        .await;
        client
            .execute("INSERT INTO controls_recovery VALUES ($1)", &[&value])
            .await
            .unwrap();
        let error = client
            .execute("INSERT INTO controls_recovery VALUES (-1)", &[])
            .await
            .unwrap_err();
        assert_eq!(error.code().unwrap().code(), "23514");
        success(
            &client,
            "ROLLBACK TO SAVEPOINT statement_scope; RELEASE SAVEPOINT statement_scope",
            NativeControl::RecoverSavepoint,
        )
        .await;
        assert_eq!(values(&client, "controls_recovery").await, [1]);
    }
    client
        .execute("INSERT INTO controls_recovery VALUES (5)", &[])
        .await
        .unwrap();
    let failure = control(
        &client,
        "RELEASE SAVEPOINT statement_scope",
        NativeControl::Release,
    )
    .await
    .unwrap_err();
    backend_failure(
        &failure,
        NativeControl::Release,
        0,
        "3B001",
        TransactionState::FailedTransaction,
    );
    success(
        &client,
        "ROLLBACK TO SAVEPOINT client_scope; RELEASE SAVEPOINT client_scope",
        NativeControl::RecoverSavepoint,
    )
    .await;
    assert_eq!(values(&client, "controls_recovery").await, [1]);
    client
        .execute("INSERT INTO controls_recovery VALUES (6)", &[])
        .await
        .unwrap();
    success(&client, "COMMIT", NativeControl::Commit).await;
    assert_eq!(values(&client, "controls_recovery").await, [1, 6]);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn release_error_after_rollback_retains_partial_recovery_and_failed_state() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE controls_partial (n integer CHECK (n > 0))")
        .await
        .unwrap();
    success(&client, "BEGIN", NativeControl::Begin).await;
    client
        .execute("INSERT INTO controls_partial VALUES (1)", &[])
        .await
        .unwrap();
    success(
        &client,
        "SAVEPOINT statement_scope",
        NativeControl::Savepoint,
    )
    .await;
    client
        .execute("INSERT INTO controls_partial VALUES (2)", &[])
        .await
        .unwrap();
    let error = client
        .execute("INSERT INTO controls_partial VALUES (-1)", &[])
        .await
        .unwrap_err();
    assert_eq!(error.code().unwrap().code(), "23514");
    let failure = control(
        &client,
        "ROLLBACK TO SAVEPOINT statement_scope; RELEASE SAVEPOINT missing_scope",
        NativeControl::RecoverSavepoint,
    )
    .await
    .unwrap_err();
    backend_failure(
        &failure,
        NativeControl::RecoverSavepoint,
        1,
        "3B001",
        TransactionState::FailedTransaction,
    );
    success(
        &client,
        "ROLLBACK TO SAVEPOINT statement_scope; RELEASE SAVEPOINT statement_scope",
        NativeControl::RecoverSavepoint,
    )
    .await;
    assert_eq!(values(&client, "controls_partial").await, [1]);
    client
        .execute("INSERT INTO controls_partial VALUES (3)", &[])
        .await
        .unwrap();
    success(&client, "COMMIT", NativeControl::Commit).await;
    assert_eq!(values(&client, "controls_partial").await, [1, 3]);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn missing_release_empty_query_and_extra_control_are_not_success() {
    let (client, connection) = client().await;
    let failure = control(
        &client,
        "-- an ordinary empty query\n",
        NativeControl::Begin,
    )
    .await
    .unwrap_err();
    assert_eq!(failure.matched_tags(), 0);
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::EmptyQuery { position: 0 })
    );
    assert!(failure.backend_error().is_none() && failure.stream_error().is_none());
    success(&client, "BEGIN", NativeControl::Begin).await;
    success(
        &client,
        "SAVEPOINT statement_scope",
        NativeControl::Savepoint,
    )
    .await;
    let failure = control(
        &client,
        "ROLLBACK TO SAVEPOINT statement_scope",
        NativeControl::RecoverSavepoint,
    )
    .await
    .unwrap_err();
    assert_eq!(failure.matched_tags(), 1);
    assert_eq!(failure.ready_state(), Some(TransactionState::Transaction));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::MissingTags {
            expected: 2,
            matched: 1
        })
    );
    assert!(failure.backend_error().is_none() && failure.stream_error().is_none());
    success(
        &client,
        "RELEASE SAVEPOINT statement_scope",
        NativeControl::Release,
    )
    .await;
    success(&client, "ROLLBACK", NativeControl::Rollback).await;
    let failure = control(
        &client,
        "BEGIN; SAVEPOINT extra_scope",
        NativeControl::Begin,
    )
    .await
    .unwrap_err();
    assert_eq!(failure.matched_tags(), 1);
    assert_eq!(failure.ready_state(), Some(TransactionState::Transaction));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::Tag {
            position: 1,
            expected: None,
            actual: "SAVEPOINT".to_owned(),
        })
    );
    assert!(failure.backend_error().is_none() && failure.stream_error().is_none());
    success(&client, "ROLLBACK", NativeControl::Rollback).await;
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn chained_finishes_keep_matching_tag_but_fail_idle_expectation() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE controls_chain (n integer)")
        .await
        .unwrap();
    for (sql, expected, remaining) in [
        ("COMMIT AND CHAIN", NativeControl::Commit, vec![1]),
        ("ROLLBACK AND CHAIN", NativeControl::Rollback, vec![]),
    ] {
        client
            .execute("DELETE FROM controls_chain", &[])
            .await
            .unwrap();
        success(&client, "BEGIN", NativeControl::Begin).await;
        client
            .execute("INSERT INTO controls_chain VALUES (1)", &[])
            .await
            .unwrap();
        let failure = control(&client, sql, expected).await.unwrap_err();
        assert_eq!(failure.control(), expected);
        assert_eq!(failure.matched_tags(), 1);
        assert_eq!(failure.ready_state(), Some(TransactionState::Transaction));
        assert_eq!(
            failure.mismatch(),
            Some(&NativeControlMismatch::State {
                expected: TransactionState::Idle,
                actual: TransactionState::Transaction,
            })
        );
        assert!(failure.backend_error().is_none() && failure.stream_error().is_none());
        // Roll back the new chained transaction. This cannot undo the earlier
        // observed COMMIT and must not mislabel it as a rolled-back write.
        success(&client, "ROLLBACK", NativeControl::Rollback).await;
        assert_eq!(values(&client, "controls_chain").await, remaining);
    }
    success(&client, "BEGIN", NativeControl::Begin).await;
    let failure = control(&client, "ROLLBACK", NativeControl::RollbackTo)
        .await
        .unwrap_err();
    assert_eq!(failure.matched_tags(), 1);
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::State {
            expected: TransactionState::Transaction,
            actual: TransactionState::Idle,
        })
    );
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn errors_before_control_tags_preserve_sqlstate_and_actual_ready_state() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE controls_setup_error (n integer CHECK (n > 0))")
        .await
        .unwrap();
    let failure = control(
        &client,
        "SAVEPOINT no_transaction",
        NativeControl::Savepoint,
    )
    .await
    .unwrap_err();
    backend_failure(
        &failure,
        NativeControl::Savepoint,
        0,
        "25P01",
        TransactionState::Idle,
    );
    success(&client, "BEGIN", NativeControl::Begin).await;
    success(
        &client,
        "SAVEPOINT statement_scope",
        NativeControl::Savepoint,
    )
    .await;
    let error = client
        .execute("INSERT INTO controls_setup_error VALUES (-1)", &[])
        .await
        .unwrap_err();
    assert_eq!(error.code().unwrap().code(), "23514");
    let failure = control(
        &client,
        "RELEASE SAVEPOINT statement_scope",
        NativeControl::Release,
    )
    .await
    .unwrap_err();
    backend_failure(
        &failure,
        NativeControl::Release,
        0,
        "25P02",
        TransactionState::FailedTransaction,
    );
    success(
        &client,
        "ROLLBACK TO SAVEPOINT statement_scope; RELEASE SAVEPOINT statement_scope",
        NativeControl::RecoverSavepoint,
    )
    .await;
    success(&client, "ROLLBACK", NativeControl::Rollback).await;
    assert!(values(&client, "controls_setup_error").await.is_empty());
    drop(client);
    connection.await.unwrap().unwrap();
}
