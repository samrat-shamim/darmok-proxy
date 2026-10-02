use std::error::Error as StdError;
use std::pin::Pin;
use std::task::Context;

use darmok_execute::{
    NativeControl, NativeControlCompletion, NativeControlFailure, NativeControlMismatch,
    check_native_control,
};
use futures_util::{Stream, StreamExt, task::noop_waker_ref};
use tokio_postgres::{Client, CommandEvent, Error, NoTls, TransactionState};

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
async fn initialization_requires_rollback_and_fixed_lookup_setting_in_one_complete_request() {
    let (client, connection) = client().await;
    client
        .batch_execute("SET search_path = public, pg_catalog; BEGIN; CREATE TEMP TABLE initialization_values(n integer); INSERT INTO initialization_values VALUES(1)")
        .await
        .unwrap();
    success(
        &client,
        "ROLLBACK; SET search_path = pg_catalog",
        NativeControl::Initialize,
    )
    .await;
    let row = client
        .query_one("SELECT pg_catalog.current_setting('search_path'), pg_catalog.to_regclass('pg_temp.initialization_values') IS NULL", &[])
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "pg_catalog");
    assert!(row.get::<_, bool>(1));
    let failure = control(&client, "ROLLBACK", NativeControl::Initialize)
        .await
        .unwrap_err();
    assert_eq!(failure.matched_tags(), 1);
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::MissingTags {
            expected: 2,
            matched: 1
        })
    );
    let failure = control(
        &client,
        "SET search_path = pg_catalog",
        NativeControl::Initialize,
    )
    .await
    .unwrap_err();
    assert_eq!(failure.matched_tags(), 0);
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::Tag {
            position: 0,
            expected: Some("ROLLBACK"),
            actual: "SET".to_owned()
        })
    );
    let failure = control(
        &client,
        "ROLLBACK; SET search_path = pg_catalog; SET search_path = public",
        NativeControl::Initialize,
    )
    .await
    .unwrap_err();
    assert_eq!(failure.matched_tags(), 2);
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::Tag {
            position: 2,
            expected: None,
            actual: "SET".to_owned()
        })
    );
    drop(client);
    connection.await.unwrap().unwrap();
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

#[tokio::test]
async fn complete_request_check_rejects_consumed_streams_but_accepts_pending_polling() {
    let (client, connection) = client().await;
    let untouched = client.command_events("BEGIN; COMMIT").unwrap();
    assert!(!untouched.has_yielded());
    let failure = check_native_control(NativeControl::Commit, untouched)
        .await
        .unwrap_err();
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::Tag {
            position: 0,
            expected: Some("COMMIT"),
            actual: "BEGIN".to_owned(),
        })
    );

    let mut partial = client.command_events("BEGIN; COMMIT").unwrap();
    assert!(!partial.has_yielded());
    assert!(matches!(
        partial.next().await.unwrap().unwrap(),
        CommandEvent::CommandComplete(tag) if tag == "BEGIN"
    ));
    assert!(partial.has_yielded());
    let failure = check_native_control(NativeControl::Commit, partial)
        .await
        .unwrap_err();
    assert_eq!(failure.matched_tags(), 0);
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::AlreadyConsumed)
    );
    assert!(failure.backend_error().is_none() && failure.stream_error().is_none());

    let mut partial_error = client
        .command_events("BEGIN; SAVEPOINT existing_scope; RELEASE SAVEPOINT missing_scope")
        .unwrap();
    assert!(matches!(
        partial_error.next().await.unwrap().unwrap(),
        CommandEvent::CommandComplete(tag) if tag == "BEGIN"
    ));
    let failure = check_native_control(NativeControl::Begin, partial_error)
        .await
        .unwrap_err();
    assert_eq!(failure.matched_tags(), 0);
    assert_eq!(
        failure.ready_state(),
        Some(TransactionState::FailedTransaction)
    );
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::AlreadyConsumed)
    );
    assert_eq!(
        failure.backend_error().unwrap().code().unwrap().code(),
        "3B001"
    );
    assert!(failure.stream_error().is_none());
    success(&client, "ROLLBACK", NativeControl::Rollback).await;

    let mut complete = client.command_events("BEGIN").unwrap();
    while let Some(event) = complete.next().await {
        event.unwrap();
    }
    assert!(complete.has_yielded());
    let failure = check_native_control(NativeControl::Begin, complete)
        .await
        .unwrap_err();
    assert_eq!(failure.matched_tags(), 0);
    // The state consumed before handoff cannot be reconstructed or fabricated.
    assert_eq!(failure.ready_state(), None);
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::AlreadyConsumed)
    );
    assert!(failure.backend_error().is_none() && failure.stream_error().is_none());
    success(&client, "ROLLBACK", NativeControl::Rollback).await;
    drop(client);
    connection.await.unwrap().unwrap();

    // No request events are available until the normal connection future runs.
    // An initial Pending poll therefore leaves the complete stream available.
    let url = std::env::var("DARMOK_TEST_DATABASE_URL").unwrap();
    let (pending_client, pending_connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    let mut pending = pending_client.command_events("BEGIN").unwrap();
    let mut context = Context::from_waker(noop_waker_ref());
    assert!(Pin::new(&mut pending).poll_next(&mut context).is_pending());
    assert!(!pending.has_yielded());
    let pending_connection = tokio::spawn(pending_connection);
    let completion = check_native_control(NativeControl::Begin, pending)
        .await
        .unwrap();
    assert_eq!(completion.ready_state(), TransactionState::Transaction);
    success(&pending_client, "ROLLBACK", NativeControl::Rollback).await;
    drop(pending_client);
    pending_connection.await.unwrap().unwrap();
}
