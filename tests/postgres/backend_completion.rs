use std::sync::atomic::{AtomicUsize, Ordering};

use bytes::BytesMut;
use darmok_execute::{NativeParamUtc, NativeStatementUtc};
use darmok_types::Value;
use futures_util::{StreamExt, stream::FusedStream};
use tokio_postgres::types::{IsNull, ToSql, Type, to_sql_checked};
use tokio_postgres::{
    Client, CommandEvent, CommandEventStream, Error, NoTls, QueryEvent, QueryEventStream, Row,
    TransactionState,
};

async fn client() -> (Client, tokio::task::JoinHandle<Result<(), Error>>) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

struct Commands {
    tags: Vec<String>,
    empty: usize,
    errors: Vec<Error>,
    state: TransactionState,
}

async fn drain_commands(mut stream: CommandEventStream) -> Commands {
    let mut tags = Vec::new();
    let mut empty = 0;
    let mut errors = Vec::new();
    let mut state = None;
    assert!(!stream.is_terminated());
    while let Some(event) = stream.next().await {
        assert!(state.is_none(), "event after ReadyForQuery");
        match event.unwrap() {
            CommandEvent::CommandComplete(tag) => tags.push(tag),
            CommandEvent::EmptyQuery => empty += 1,
            CommandEvent::BackendError(error) => errors.push(error),
            CommandEvent::ReadyForQuery(value) => state = Some(value),
        }
    }
    assert!(stream.is_terminated());
    assert!(stream.next().await.is_none());
    Commands {
        tags,
        empty,
        errors,
        state: state.expect("request did not confirm ReadyForQuery"),
    }
}

async fn command(client: &Client, sql: &str, tags: &[&str], state: TransactionState) {
    let result = drain_commands(client.command_events(sql).unwrap()).await;
    assert_eq!(result.tags, tags);
    assert_eq!(result.empty, 0);
    assert!(result.errors.is_empty());
    assert_eq!(result.state, state);
}

struct Query {
    rows: Vec<Row>,
    tag: Option<String>,
    empty: bool,
    errors: Vec<Error>,
    state: TransactionState,
}

async fn drain_query(mut stream: QueryEventStream) -> Query {
    let mut rows = Vec::new();
    let mut tag = None;
    let mut empty = false;
    let mut errors = Vec::new();
    let mut state = None;
    assert!(!stream.is_terminated());
    while let Some(event) = stream.next().await {
        assert!(state.is_none(), "event after ReadyForQuery");
        match event.unwrap() {
            QueryEvent::Row(row) => {
                assert!(tag.is_none() && !empty && errors.is_empty());
                rows.push(row);
            }
            QueryEvent::CommandComplete(value) => {
                assert!(tag.is_none() && !empty && errors.is_empty());
                tag = Some(value);
            }
            QueryEvent::EmptyQuery => {
                assert!(tag.is_none() && !empty && rows.is_empty() && errors.is_empty());
                empty = true;
            }
            QueryEvent::PortalSuspended => panic!("unbounded execution suspended a portal"),
            QueryEvent::BackendError(error) => {
                assert!(errors.is_empty());
                errors.push(error);
            }
            QueryEvent::ReadyForQuery(value) => state = Some(value),
        }
    }
    assert!(stream.is_terminated());
    assert!(stream.next().await.is_none());
    Query {
        rows,
        tag,
        empty,
        errors,
        state: state.expect("request did not confirm ReadyForQuery"),
    }
}

#[tokio::test]
async fn command_tags_empty_queries_and_ready_states_remain_distinct() {
    let (client, connection) = client().await;
    for sql in ["", "-- an ordinary empty query\n"] {
        let result = drain_commands(client.command_events(sql).unwrap()).await;
        assert_eq!(result.empty, 1);
        assert!(result.tags.is_empty() && result.errors.is_empty());
        assert_eq!(result.state, TransactionState::Idle);
    }
    command(
        &client,
        "CREATE TEMP TABLE completion_commands (n integer); \
         INSERT INTO completion_commands SELECT 1 WHERE false; \
         UPDATE completion_commands SET n = 2; DELETE FROM completion_commands",
        &["CREATE TABLE", "INSERT 0 0", "UPDATE 0", "DELETE 0"],
        TransactionState::Idle,
    )
    .await;
    command(
        &client,
        "BEGIN; SAVEPOINT statement_scope; RELEASE SAVEPOINT statement_scope",
        &["BEGIN", "SAVEPOINT", "RELEASE"],
        TransactionState::Transaction,
    )
    .await;
    command(&client, "COMMIT", &["COMMIT"], TransactionState::Idle).await;
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn aborted_commit_is_observed_as_rollback_instead_of_commit() {
    let (client, connection) = client().await;
    command(
        &client,
        "CREATE TEMP TABLE completion_check (n integer CHECK (n > 0))",
        &["CREATE TABLE"],
        TransactionState::Idle,
    )
    .await;
    command(&client, "BEGIN", &["BEGIN"], TransactionState::Transaction).await;
    let failure = drain_commands(
        client
            .command_events("INSERT INTO completion_check VALUES (-1)")
            .unwrap(),
    )
    .await;
    assert!(failure.tags.is_empty());
    assert_eq!(failure.errors.len(), 1);
    assert_eq!(failure.errors[0].code().unwrap().code(), "23514");
    assert_eq!(failure.state, TransactionState::FailedTransaction);
    command(&client, "COMMIT", &["ROLLBACK"], TransactionState::Idle).await;
    let count: i64 = client
        .query_one("SELECT count(*) FROM completion_check", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    command(
        &client,
        "INSERT INTO completion_check VALUES (1)",
        &["INSERT 0 1"],
        TransactionState::Idle,
    )
    .await;
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn deferred_constraint_failure_at_commit_retains_error_and_idle_state() {
    let (client, connection) = client().await;
    command(
        &client,
        "CREATE TEMP TABLE completion_deferred \
         (n integer UNIQUE DEFERRABLE INITIALLY DEFERRED)",
        &["CREATE TABLE"],
        TransactionState::Idle,
    )
    .await;
    command(&client, "BEGIN", &["BEGIN"], TransactionState::Transaction).await;
    command(
        &client,
        "INSERT INTO completion_deferred VALUES (1), (1)",
        &["INSERT 0 2"],
        TransactionState::Transaction,
    )
    .await;
    let finish = drain_commands(client.command_events("COMMIT").unwrap()).await;
    assert!(finish.tags.is_empty());
    assert_eq!(finish.errors.len(), 1);
    assert_eq!(finish.errors[0].code().unwrap().code(), "23505");
    assert_eq!(finish.state, TransactionState::Idle);
    let count: i64 = client
        .query_one("SELECT count(*) FROM completion_deferred", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    command(
        &client,
        "BEGIN; INSERT INTO completion_deferred VALUES (1); COMMIT",
        &["BEGIN", "INSERT 0 1", "COMMIT"],
        TransactionState::Idle,
    )
    .await;
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn autocommit_sync_error_after_command_complete_keeps_error_and_ready_state() {
    let (client, connection) = client().await;
    command(
        &client,
        "CREATE TEMP TABLE completion_auto_deferred \
         (n integer UNIQUE DEFERRABLE INITIALLY DEFERRED)",
        &["CREATE TABLE"],
        TransactionState::Idle,
    )
    .await;
    for returning in [false, true] {
        let suffix = if returning { " RETURNING n" } else { "" };
        let statement = client
            .prepare(&format!(
                "INSERT INTO completion_auto_deferred VALUES (1), (1){suffix}"
            ))
            .await
            .unwrap();
        let description = NativeStatementUtc::new(&statement).unwrap();
        let bindings = description.bind(&[]).unwrap();
        let result = drain_query(
            client
                .query_events(bindings.statement(), bindings.parameters())
                .unwrap(),
        )
        .await;
        assert_eq!(result.tag.as_deref(), Some("INSERT 0 2"));
        assert!(!result.empty);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].code().unwrap().code(), "23505");
        assert_eq!(result.state, TransactionState::Idle);
        assert_eq!(result.rows.len(), if returning { 2 } else { 0 });
        for row in &result.rows {
            assert_eq!(description.decode_row(row).unwrap(), [Value::Int(1)]);
        }
        let count: i64 = client
            .query_one("SELECT count(*) FROM completion_auto_deferred", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(count, 0);
        // A following valid autocommit write succeeds on the same connection
        // after the failed request's final ReadyForQuery has been observed.
        let valid = client
            .prepare("INSERT INTO completion_auto_deferred VALUES (2) RETURNING n")
            .await
            .unwrap();
        let valid_description = NativeStatementUtc::new(&valid).unwrap();
        let bindings = valid_description.bind(&[]).unwrap();
        let result = drain_query(
            client
                .query_events(bindings.statement(), bindings.parameters())
                .unwrap(),
        )
        .await;
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            valid_description.decode_row(&result.rows[0]).unwrap(),
            [Value::Int(2)]
        );
        assert_eq!(result.tag.as_deref(), Some("INSERT 0 1"));
        assert!(result.errors.is_empty());
        assert_eq!(result.state, TransactionState::Idle);
        command(
            &client,
            "DELETE FROM completion_auto_deferred",
            &["DELETE 1"],
            TransactionState::Idle,
        )
        .await;
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[derive(Debug)]
struct CountedInteger<'a> {
    value: i32,
    calls: &'a AtomicUsize,
}

impl ToSql for CountedInteger<'_> {
    fn to_sql(
        &self,
        ty: &Type,
        buf: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Send + Sync>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.value.to_sql(ty, buf)
    }

    fn accepts(ty: &Type) -> bool {
        <i32 as ToSql>::accepts(ty)
    }

    to_sql_checked!();
}

#[tokio::test]
async fn prepared_rows_empty_and_zero_row_results_keep_explicit_completion() {
    let (client, connection) = client().await;
    let calls = AtomicUsize::new(0);
    let statement = client
        .prepare("SELECT $1::integer AS \"exact.Label\"")
        .await
        .unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    let param = CountedInteger {
        value: 42,
        calls: &calls,
    };
    let stream = client.query_events(&statement, [&param]).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let result = drain_query(stream).await;
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        description.decode_row(&result.rows[0]).unwrap(),
        [Value::Int(42)]
    );
    assert_eq!(result.rows[0].columns()[0].name(), "exact.Label");
    assert_eq!(result.tag.as_deref(), Some("SELECT 1"));
    assert!(!result.empty && result.errors.is_empty());
    assert_eq!(result.state, TransactionState::Idle);
    drop(statement);
    for (sql, expected_rows, tag, empty) in [
        ("SELECT 1 WHERE false", 0, Some("SELECT 0"), false),
        (
            "SELECT FROM generate_series(1, 2)",
            2,
            Some("SELECT 2"),
            false,
        ),
        ("", 0, None, true),
    ] {
        let statement = client.prepare(sql).await.unwrap();
        let description = NativeStatementUtc::new(&statement).unwrap();
        let bindings = description.bind(&[]).unwrap();
        let result = drain_query(
            client
                .query_events(bindings.statement(), bindings.parameters())
                .unwrap(),
        )
        .await;
        assert_eq!(result.rows.len(), expected_rows);
        assert_eq!(result.tag.as_deref(), tag);
        assert_eq!(result.empty, empty);
        assert!(result.errors.is_empty());
        assert_eq!(result.state, TransactionState::Idle);
        if expected_rows == 2 {
            for row in &result.rows {
                assert!(description.decode_row(row).unwrap().is_empty());
            }
        }
    }
    command(
        &client,
        "CREATE TEMP TABLE completion_no_data (n integer)",
        &["CREATE TABLE"],
        TransactionState::Idle,
    )
    .await;
    for (sql, tag) in [
        ("INSERT INTO completion_no_data VALUES (1)", "INSERT 0 1"),
        (
            "UPDATE completion_no_data SET n = 2 WHERE false",
            "UPDATE 0",
        ),
        ("DELETE FROM completion_no_data", "DELETE 1"),
    ] {
        let statement = client.prepare(sql).await.unwrap();
        let description = NativeStatementUtc::new(&statement).unwrap();
        assert!(statement.columns().is_empty());
        let bindings = description.bind(&[]).unwrap();
        let result = drain_query(
            client
                .query_events(bindings.statement(), bindings.parameters())
                .unwrap(),
        )
        .await;
        assert!(result.rows.is_empty() && !result.empty && result.errors.is_empty());
        assert_eq!(result.tag.as_deref(), Some(tag));
        assert_eq!(result.state, TransactionState::Idle);
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn prepared_execution_and_pre_bind_errors_can_be_drained_before_recovery() {
    let (client, connection) = client().await;
    command(
        &client,
        "CREATE TEMP TABLE completion_prepared (n integer PRIMARY KEY)",
        &["CREATE TABLE"],
        TransactionState::Idle,
    )
    .await;
    let insert = client
        .prepare("INSERT INTO completion_prepared VALUES ($1::integer) RETURNING n")
        .await
        .unwrap();
    let read = client
        .prepare("SELECT 10 / (2 - i) FROM generate_series(1, 3) AS g(i)")
        .await
        .unwrap();
    command(
        &client,
        "BEGIN; INSERT INTO completion_prepared VALUES (1); SAVEPOINT statement_scope",
        &["BEGIN", "INSERT 0 1", "SAVEPOINT"],
        TransactionState::Transaction,
    )
    .await;
    let result = drain_query(
        client
            .query_events(&read, std::iter::empty::<&i32>())
            .unwrap(),
    )
    .await;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].get::<_, i32>(0), 10);
    assert!(result.tag.is_none());
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].code().unwrap().code(), "22012");
    assert_eq!(result.state, TransactionState::FailedTransaction);
    // PostgreSQL rejects this Bind while the transaction is failed, before
    // BindComplete. The same event stream must still expose its ReadyForQuery.
    let failed_bind = drain_query(client.query_events(&insert, [&2_i32]).unwrap()).await;
    assert!(failed_bind.rows.is_empty() && failed_bind.tag.is_none());
    assert_eq!(failed_bind.errors.len(), 1);
    assert_eq!(failed_bind.errors[0].code().unwrap().code(), "25P02");
    assert_eq!(failed_bind.state, TransactionState::FailedTransaction);
    command(
        &client,
        "ROLLBACK TO SAVEPOINT statement_scope; RELEASE SAVEPOINT statement_scope",
        &["ROLLBACK", "RELEASE"],
        TransactionState::Transaction,
    )
    .await;
    let description = NativeStatementUtc::new(&insert).unwrap();
    let values = [Value::Int(2)];
    let bindings = description.bind(&values).unwrap();
    let result = drain_query(
        client
            .query_events(bindings.statement(), bindings.parameters())
            .unwrap(),
    )
    .await;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        description.decode_row(&result.rows[0]).unwrap(),
        [Value::Int(2)]
    );
    assert_eq!(result.tag.as_deref(), Some("INSERT 0 1"));
    assert!(result.errors.is_empty());
    assert_eq!(result.state, TransactionState::Transaction);
    command(&client, "COMMIT", &["COMMIT"], TransactionState::Idle).await;
    let rows = client
        .query("SELECT n FROM completion_prepared ORDER BY n", &[])
        .await
        .unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row.get::<_, i32>(0))
            .collect::<Vec<_>>(),
        [1, 2]
    );
    drop(read);
    drop(insert);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn local_parameter_errors_do_not_queue_a_partial_execute_request() {
    let (client, connection) = client().await;
    command(
        &client,
        "CREATE TEMP TABLE completion_encoding (n smallint)",
        &["CREATE TABLE"],
        TransactionState::Idle,
    )
    .await;
    let insert = client
        .prepare("INSERT INTO completion_encoding VALUES ($1::smallint)")
        .await
        .unwrap();
    command(&client, "BEGIN", &["BEGIN"], TransactionState::Transaction).await;
    let value = Value::Int(i64::from(i16::MAX) + 1);
    assert!(
        client
            .query_events(&insert, [NativeParamUtc::new(&value)])
            .is_err()
    );
    assert!(
        client
            .query_events(&insert, std::iter::empty::<&i16>())
            .is_err()
    );
    command(
        &client,
        "SAVEPOINT encoding_failed; RELEASE SAVEPOINT encoding_failed",
        &["SAVEPOINT", "RELEASE"],
        TransactionState::Transaction,
    )
    .await;
    command(&client, "ROLLBACK", &["ROLLBACK"], TransactionState::Idle).await;
    let count: i64 = client
        .query_one("SELECT count(*) FROM completion_encoding", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    command(
        &client,
        "INSERT INTO completion_encoding VALUES (7)",
        &["INSERT 0 1"],
        TransactionState::Idle,
    )
    .await;
    drop(insert);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn queued_requests_keep_their_own_command_tags_and_ready_states() {
    let (client, connection) = client().await;
    command(
        &client,
        "CREATE TEMP TABLE completion_queued (n integer)",
        &["CREATE TABLE"],
        TransactionState::Idle,
    )
    .await;
    let insert = client
        .prepare("INSERT INTO completion_queued VALUES ($1::integer) RETURNING n")
        .await
        .unwrap();
    // This checks the connector's request association, not an executor policy
    // allowing commit before output validation.
    let begin = client.command_events("BEGIN").unwrap();
    let query = client.query_events(&insert, [&9_i32]).unwrap();
    let commit = client.command_events("COMMIT").unwrap();
    let begin = drain_commands(begin).await;
    assert_eq!(begin.tags, ["BEGIN"]);
    assert_eq!(begin.state, TransactionState::Transaction);
    assert!(begin.errors.is_empty());
    let query = drain_query(query).await;
    assert_eq!(query.rows.len(), 1);
    assert_eq!(query.rows[0].get::<_, i32>(0), 9);
    assert_eq!(query.tag.as_deref(), Some("INSERT 0 1"));
    assert_eq!(query.state, TransactionState::Transaction);
    assert!(query.errors.is_empty());
    let commit = drain_commands(commit).await;
    assert_eq!(commit.tags, ["COMMIT"]);
    assert_eq!(commit.state, TransactionState::Idle);
    assert!(commit.errors.is_empty());
    drop(insert);
    drop(client);
    connection.await.unwrap().unwrap();
}
