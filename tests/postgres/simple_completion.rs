//! Ordinary text simple-query observations, separate from catalog validity.
use std::{error::Error as StdError, sync::Arc};

use futures_util::{StreamExt, stream::FusedStream};
use tokio_postgres::types::Type;
use tokio_postgres::{
    Client, Error, NoTls, SimpleColumn, SimpleQueryEvent, SimpleQueryEventStream, SimpleQueryRow,
    TransactionState,
};

async fn client() -> (Client, tokio::task::JoinHandle<Result<(), Error>>) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

struct Observed {
    events: Vec<SimpleQueryEvent>,
    state: TransactionState,
}

impl Observed {
    fn tags(&self) -> Vec<&str> {
        self.events
            .iter()
            .filter_map(|event| match event {
                SimpleQueryEvent::CommandComplete(tag) => Some(tag.as_str()),
                _ => None,
            })
            .collect()
    }

    fn rows(&self) -> Vec<&SimpleQueryRow> {
        self.events
            .iter()
            .filter_map(|event| match event {
                SimpleQueryEvent::Row(row) => Some(row),
                _ => None,
            })
            .collect()
    }

    fn descriptions(&self) -> Vec<&Arc<[SimpleColumn]>> {
        self.events
            .iter()
            .filter_map(|event| match event {
                SimpleQueryEvent::RowDescription(columns) => Some(columns),
                _ => None,
            })
            .collect()
    }

    fn errors(&self) -> Vec<&Error> {
        self.events
            .iter()
            .filter_map(|event| match event {
                SimpleQueryEvent::BackendError(error) => Some(error),
                _ => None,
            })
            .collect()
    }

    fn kinds(&self) -> Vec<&str> {
        self.events
            .iter()
            .map(|event| match event {
                SimpleQueryEvent::RowDescription(_) => "description",
                SimpleQueryEvent::Row(_) => "row",
                SimpleQueryEvent::CommandComplete(_) => "tag",
                SimpleQueryEvent::EmptyQuery => "empty",
                SimpleQueryEvent::BackendError(_) => "error",
                SimpleQueryEvent::ReadyForQuery(_) => "ready",
            })
            .collect()
    }
}

async fn drain(mut stream: SimpleQueryEventStream) -> Observed {
    assert!(!stream.has_yielded() && !stream.is_terminated());
    let mut events = Vec::new();
    let mut state = None;
    while let Some(event) = stream.next().await {
        assert!(stream.has_yielded());
        assert!(state.is_none(), "event after ReadyForQuery");
        let event = event.unwrap();
        if let SimpleQueryEvent::ReadyForQuery(value) = &event {
            state = Some(*value);
        }
        events.push(event);
    }
    assert!(stream.has_yielded() && stream.is_terminated());
    assert!(stream.next().await.is_none());
    Observed {
        events,
        state: state.expect("request did not confirm ReadyForQuery"),
    }
}

async fn query(client: &Client, sql: &str) -> Observed {
    drain(client.simple_query_events(sql).unwrap()).await
}

async fn close(client: Client, connection: tokio::task::JoinHandle<Result<(), Error>>) {
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn show_rows_preserve_native_text_description_and_exact_utility_tags() {
    let (client, connection) = client().await;
    let result = query(
        &client,
        "SET application_name = 'simple completion'; SHOW application_name; \
         SET application_name = 'second value'; SHOW application_name; RESET application_name",
    )
    .await;
    assert_eq!(result.tags(), ["SET", "SHOW", "SET", "SHOW", "RESET"]);
    assert_eq!(result.state, TransactionState::Idle);
    assert!(result.errors().is_empty());
    assert_eq!(
        result.kinds(),
        [
            "tag",
            "description",
            "row",
            "tag",
            "tag",
            "description",
            "row",
            "tag",
            "tag",
            "ready"
        ]
    );
    let rows = result.rows();
    assert_eq!(rows[0].try_get(0).unwrap(), Some("simple completion"));
    assert_eq!(
        rows[1].try_get("application_name").unwrap(),
        Some("second value")
    );
    for (description, row) in result.descriptions().into_iter().zip(rows) {
        assert!(std::ptr::eq(description.as_ref(), row.columns()));
        let column = &description[0];
        assert_eq!(column.name(), "application_name");
        assert_eq!(column.type_oid(), Type::TEXT.oid());
        assert_eq!(column.type_size(), -1);
        assert_eq!(column.type_modifier(), -1);
        assert_eq!(column.format(), 0);
        assert_eq!(column.table_oid(), None);
        assert_eq!(column.column_id(), None);
    }
    close(client, connection).await;
}

#[tokio::test]
async fn each_statement_retains_its_own_columns_origins_and_nullable_text_values() {
    let (client, connection) = client().await;
    client
        .batch_execute(
            "CREATE TEMP TABLE simple_columns(n integer, label varchar(7)); \
             INSERT INTO simple_columns VALUES (4, 'four')",
        )
        .await
        .unwrap();
    let table_oid: u32 = client
        .query_one("SELECT 'simple_columns'::regclass::oid", &[])
        .await
        .unwrap()
        .get(0);
    let result = query(
        &client,
        "SELECT n, label AS \"Exact label\" FROM simple_columns; \
         SELECT n, label AS \"Zero label\" FROM simple_columns WHERE false; \
         SELECT NULL::text AS duplicate, ''::text AS duplicate, '東京🖖'::text AS unicode",
    )
    .await;
    assert_eq!(result.tags(), ["SELECT 1", "SELECT 0", "SELECT 1"]);
    assert!(result.errors().is_empty());
    let descriptions = result.descriptions();
    let rows = result.rows();
    assert_eq!(descriptions.len(), 3);
    assert_eq!(rows.len(), 2);
    assert!(std::ptr::eq(descriptions[0].as_ref(), rows[0].columns()));
    assert!(std::ptr::eq(descriptions[2].as_ref(), rows[1].columns()));
    assert_eq!(descriptions[1][1].name(), "Zero label");
    let integer = &descriptions[0][0];
    assert_eq!(integer.type_oid(), Type::INT4.oid());
    assert_eq!(integer.type_size(), 4);
    assert_eq!(integer.type_modifier(), -1);
    assert_eq!(integer.table_oid(), Some(table_oid));
    assert_eq!(integer.column_id(), Some(1));
    let varchar = &descriptions[0][1];
    assert_eq!(varchar.name(), "Exact label");
    assert_eq!(varchar.type_oid(), Type::VARCHAR.oid());
    assert_eq!(varchar.type_size(), -1);
    assert_eq!(varchar.type_modifier(), 11);
    assert_eq!(varchar.table_oid(), Some(table_oid));
    assert_eq!(varchar.column_id(), Some(2));
    assert!(
        descriptions
            .iter()
            .flat_map(|columns| columns.iter())
            .all(|column| column.format() == 0)
    );
    assert_eq!(rows[0].try_get(0).unwrap(), Some("4"));
    assert_eq!(rows[0].try_get("Exact label").unwrap(), Some("four"));
    assert_eq!(rows[1].try_get(0).unwrap(), None);
    assert_eq!(rows[1].try_get(1).unwrap(), Some(""));
    assert_eq!(rows[1].try_get(2).unwrap(), Some("東京🖖"));
    assert!(rows[1].try_get(3).is_err());
    close(client, connection).await;
}

#[tokio::test]
async fn empty_queries_zero_columns_and_explicit_zero_counts_remain_distinct() {
    let (client, connection) = client().await;
    for sql in ["", " ; -- empty query\n;"] {
        let result = query(&client, sql).await;
        assert_eq!(result.kinds(), ["empty", "ready"]);
        assert_eq!(result.state, TransactionState::Idle);
    }
    let result = query(
        &client,
        "CREATE TEMP TABLE simple_empty(n integer); \
         INSERT INTO simple_empty SELECT 1 WHERE false; \
         SELECT FROM generate_series(1, 2); SELECT FROM generate_series(1, 2) WHERE false; \
         UPDATE simple_empty SET n = 2; DELETE FROM simple_empty",
    )
    .await;
    assert_eq!(
        result.tags(),
        [
            "CREATE TABLE",
            "INSERT 0 0",
            "SELECT 2",
            "SELECT 0",
            "UPDATE 0",
            "DELETE 0"
        ]
    );
    assert_eq!(result.descriptions().len(), 2);
    assert!(
        result
            .descriptions()
            .iter()
            .all(|columns| columns.is_empty())
    );
    assert_eq!(result.rows().len(), 2);
    assert!(result.rows().iter().all(|row| row.is_empty()));
    assert!(result.errors().is_empty());
    close(client, connection).await;
}

#[tokio::test]
async fn custom_type_oids_are_observed_without_requiring_connector_type_objects() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TYPE pg_temp.simple_mood AS ENUM ('neutral', 'focused')")
        .await
        .unwrap();
    let oid: u32 = client
        .query_one("SELECT 'pg_temp.simple_mood'::regtype::oid", &[])
        .await
        .unwrap()
        .get(0);
    assert!(Type::from_oid(oid).is_none());
    let result = query(
        &client,
        "SELECT 'focused'::pg_temp.simple_mood AS mood; \
         SELECT 'neutral'::pg_temp.simple_mood AS empty_mood WHERE false",
    )
    .await;
    assert_eq!(result.tags(), ["SELECT 1", "SELECT 0"]);
    assert!(result.errors().is_empty());
    for description in result.descriptions() {
        assert_eq!(description[0].type_oid(), oid);
        assert_eq!(description[0].type_size(), 4);
        assert_eq!(description[0].format(), 0);
    }
    assert_eq!(result.rows()[0].try_get(0).unwrap(), Some("focused"));
    close(client, connection).await;
}

#[tokio::test]
async fn native_errors_retain_prior_events_and_failed_transaction_recovery() {
    let (client, connection) = client().await;
    let parse_error = query(&client, "SELECT 1; SELECT FROM").await;
    assert_eq!(parse_error.kinds(), ["error", "ready"]);
    assert_eq!(parse_error.errors()[0].code().unwrap().code(), "42601");
    assert_eq!(parse_error.state, TransactionState::Idle);
    client
        .batch_execute("CREATE TEMP TABLE simple_recovery(n integer); BEGIN; INSERT INTO simple_recovery VALUES (1); SAVEPOINT statement_scope")
        .await
        .unwrap();
    let result = query(
        &client,
        "SELECT 9 AS earlier; SELECT 12 / (2-n) AS result FROM generate_series(1, 2) AS n; \
         INSERT INTO simple_recovery VALUES (99)",
    )
    .await;
    assert_eq!(result.tags(), ["SELECT 1"]);
    assert_eq!(result.rows().len(), 2);
    assert_eq!(result.rows()[0].try_get(0).unwrap(), Some("9"));
    assert_eq!(result.rows()[1].try_get(0).unwrap(), Some("12"));
    assert_eq!(result.errors()[0].code().unwrap().code(), "22012");
    assert_eq!(result.state, TransactionState::FailedTransaction);
    let failed = query(&client, "SHOW application_name").await;
    assert_eq!(failed.kinds(), ["error", "ready"]);
    assert_eq!(failed.errors()[0].code().unwrap().code(), "25P02");
    assert_eq!(failed.state, TransactionState::FailedTransaction);
    let recovered = query(
        &client,
        "ROLLBACK TO SAVEPOINT statement_scope; RELEASE SAVEPOINT statement_scope; \
         INSERT INTO simple_recovery VALUES (2) RETURNING n; SELECT n FROM simple_recovery ORDER BY n",
    )
    .await;
    assert_eq!(
        recovered.tags(),
        ["ROLLBACK", "RELEASE", "INSERT 0 1", "SELECT 2"]
    );
    assert_eq!(recovered.state, TransactionState::Transaction);
    assert!(recovered.errors().is_empty());
    assert_eq!(
        recovered
            .rows()
            .iter()
            .map(|row| row.try_get(0).unwrap())
            .collect::<Vec<_>>(),
        [Some("2"), Some("1"), Some("2")]
    );
    let commit = query(&client, "COMMIT").await;
    assert_eq!(commit.tags(), ["COMMIT"]);
    assert_eq!(commit.state, TransactionState::Idle);
    close(client, connection).await;
}

#[tokio::test]
async fn deferred_commit_error_retains_earlier_tag_and_last_rows_without_last_tag() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE simple_deferred(n integer UNIQUE DEFERRABLE INITIALLY DEFERRED); INSERT INTO simple_deferred VALUES (1)")
        .await
        .unwrap();
    let result = query(
        &client,
        "INSERT INTO simple_deferred VALUES (1) RETURNING n; SELECT n FROM simple_deferred ORDER BY n",
    )
    .await;
    assert_eq!(result.tags(), ["INSERT 0 1"]);
    assert_eq!(result.rows().len(), 3);
    assert_eq!(result.errors()[0].code().unwrap().code(), "23505");
    assert_eq!(result.state, TransactionState::Idle);
    for sql in [
        "INSERT INTO simple_deferred VALUES (1)",
        "INSERT INTO simple_deferred VALUES (1) RETURNING n",
    ] {
        let result = query(&client, sql).await;
        assert!(result.tags().is_empty());
        assert_eq!(result.errors()[0].code().unwrap().code(), "23505");
        assert_eq!(result.state, TransactionState::Idle);
        assert_eq!(
            result.rows().len(),
            usize::from(sql.ends_with("RETURNING n"))
        );
    }
    let count = query(&client, "SELECT count(*) FROM simple_deferred").await;
    assert_eq!(count.rows()[0].try_get(0).unwrap(), Some("1"));
    close(client, connection).await;
}

#[tokio::test]
async fn queued_requests_keep_descriptions_values_and_their_own_ready_states() {
    let (client, connection) = client().await;
    let prepared = client
        .prepare("SELECT $1::integer AS prepared")
        .await
        .unwrap();
    let begin = client.simple_query_events("BEGIN").unwrap();
    let first = client
        .simple_query_events("SELECT 'first'::text AS first")
        .unwrap();
    let middle = client.query_events(&prepared, [&7_i32]).unwrap();
    let second = client
        .simple_query_events("SELECT 'second'::text AS second")
        .unwrap();
    let commit = client.simple_query_events("COMMIT").unwrap();
    assert_eq!(drain(begin).await.state, TransactionState::Transaction);
    let first = drain(first).await;
    assert_eq!(first.rows()[0].try_get("first").unwrap(), Some("first"));
    assert_eq!(first.tags(), ["SELECT 1"]);
    assert_eq!(first.state, TransactionState::Transaction);
    let middle: Vec<_> = middle.collect().await;
    assert_eq!(middle.len(), 3);
    match middle[0].as_ref().unwrap() {
        tokio_postgres::QueryEvent::Row(row) => assert_eq!(row.get::<_, i32>(0), 7),
        _ => panic!("expected prepared row"),
    }
    assert!(matches!(
        middle[2].as_ref().unwrap(),
        tokio_postgres::QueryEvent::ReadyForQuery(TransactionState::Transaction)
    ));
    let second = drain(second).await;
    assert_eq!(second.rows()[0].try_get("second").unwrap(), Some("second"));
    assert_eq!(second.state, TransactionState::Transaction);
    let commit = drain(commit).await;
    assert_eq!(commit.tags(), ["COMMIT"]);
    assert_eq!(commit.state, TransactionState::Idle);
    drop(prepared);
    close(client, connection).await;
}

#[tokio::test]
async fn show_does_not_select_the_first_repeatable_read_data_snapshot() {
    let (owner, owner_connection) = client().await;
    let (writer, writer_connection) = client().await;
    let pid: i32 = owner
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let table = format!("simple_snapshot_{pid}");
    writer
        .batch_execute(&format!("CREATE TABLE {table}(n integer)"))
        .await
        .unwrap();
    owner
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ")
        .await
        .unwrap();
    let utility = query(&owner, "SHOW transaction_isolation").await;
    assert_eq!(utility.tags(), ["SHOW"]);
    assert_eq!(
        utility.rows()[0].try_get(0).unwrap(),
        Some("repeatable read")
    );
    assert_eq!(utility.state, TransactionState::Transaction);
    writer
        .batch_execute(&format!("INSERT INTO {table} VALUES (1)"))
        .await
        .unwrap();
    let first = query(&owner, &format!("SELECT count(*) FROM {table}")).await;
    assert_eq!(first.rows()[0].try_get(0).unwrap(), Some("1"));
    writer
        .batch_execute(&format!("INSERT INTO {table} VALUES (2)"))
        .await
        .unwrap();
    let second = query(&owner, &format!("SELECT count(*) FROM {table}")).await;
    assert_eq!(second.rows()[0].try_get(0).unwrap(), Some("1"));
    assert_eq!(query(&owner, "COMMIT").await.state, TransactionState::Idle);
    let next = query(&owner, &format!("SELECT count(*) FROM {table}")).await;
    assert_eq!(next.rows()[0].try_get(0).unwrap(), Some("2"));
    writer
        .batch_execute(&format!("DROP TABLE {table}"))
        .await
        .unwrap();
    close(owner, owner_connection).await;
    close(writer, writer_connection).await;
}

#[tokio::test]
async fn binary_cursor_description_fails_before_a_text_row_is_exposed() {
    let (client, connection) = client().await;
    client
        .batch_execute("BEGIN; DECLARE simple_binary BINARY CURSOR FOR SELECT 42::integer")
        .await
        .unwrap();
    let mut stream = client
        .simple_query_events("FETCH ALL FROM simple_binary")
        .unwrap();
    assert!(!stream.has_yielded());
    let error = stream.next().await.unwrap().unwrap_err();
    assert!(error.as_db_error().is_none());
    assert_eq!(
        error.source().unwrap().to_string(),
        "simple-query column 0 has format 1; text format is required"
    );
    assert!(stream.has_yielded() && stream.is_terminated());
    assert!(stream.next().await.is_none());
    drop(stream);
    // A separate native rollback confirms cleanup; the terminal error above
    // does not establish either readiness or rollback.
    let rollback = query(&client, "ROLLBACK").await;
    assert_eq!(rollback.tags(), ["ROLLBACK"]);
    assert_eq!(rollback.state, TransactionState::Idle);
    close(client, connection).await;
}

#[tokio::test]
async fn local_query_encoding_error_does_not_queue_partial_sql() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE simple_encoding(n integer)")
        .await
        .unwrap();
    assert!(
        client
            .simple_query_events("INSERT INTO simple_encoding VALUES (1);\0SELECT 1")
            .is_err()
    );
    let next = query(&client, "SELECT count(*) FROM simple_encoding").await;
    assert_eq!(next.rows()[0].try_get(0).unwrap(), Some("0"));
    assert!(next.errors().is_empty());
    close(client, connection).await;
}
