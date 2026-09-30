use std::{
    error::Error as StdError,
    sync::atomic::{AtomicUsize, Ordering},
};

use bytes::BytesMut;
use futures_util::{StreamExt, stream::FusedStream};
use tokio_postgres::types::{IsNull, Kind, ToSql, Type, to_sql_checked};
use tokio_postgres::{
    BuiltinQueryEventStream, Client, Error, NoTls, QueryEvent, Row, TransactionState,
    UnsupportedBuiltinResultType,
};

async fn client() -> (Client, tokio::task::JoinHandle<Result<(), Error>>) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

fn query(client: &Client, sql: &str) -> BuiltinQueryEventStream {
    client
        .query_typed_builtin_events(sql, std::iter::empty::<(&i32, Type)>())
        .unwrap()
}

type ColumnDescription = (String, u32, Option<u32>, Option<i16>, i32);

struct Observed {
    rows: Vec<Row>,
    tag: Option<String>,
    empty: bool,
    errors: Vec<Error>,
    state: TransactionState,
    columns: Option<Vec<ColumnDescription>>,
}

async fn drain(mut stream: BuiltinQueryEventStream) -> Observed {
    let mut rows = Vec::new();
    let mut tag = None;
    let mut empty = false;
    let mut errors = Vec::new();
    let mut state = None;
    assert!(!stream.has_yielded());
    assert!(stream.columns().is_none());
    while let Some(event) = stream.next().await {
        assert!(state.is_none(), "event after ReadyForQuery");
        assert!(stream.has_yielded());
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
            QueryEvent::PortalSuspended => panic!("unbounded execution suspended"),
            QueryEvent::BackendError(error) => errors.push(error),
            QueryEvent::ReadyForQuery(value) => state = Some(value),
        }
    }
    assert!(stream.is_terminated());
    assert!(stream.next().await.is_none());
    let columns = stream.columns().map(|columns| {
        columns
            .iter()
            .map(|column| {
                (
                    column.name().to_owned(),
                    column.type_().oid(),
                    column.table_oid(),
                    column.column_id(),
                    column.type_modifier(),
                )
            })
            .collect()
    });
    Observed {
        rows,
        tag,
        empty,
        errors,
        state: state.expect("no ReadyForQuery"),
        columns,
    }
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
        output: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn StdError + Sync + Send>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.value.to_sql(ty, output)
    }
    fn accepts(ty: &Type) -> bool {
        *ty == Type::INT4
    }
    to_sql_checked!();
}

#[tokio::test]
async fn typed_parameters_encode_once_and_native_columns_survive_zero_rows() {
    let (client, connection) = client().await;
    client
        .batch_execute(
            "CREATE TEMP TABLE typed_columns(n integer, label varchar(7)); \
        INSERT INTO typed_columns VALUES (4, 'four')",
        )
        .await
        .unwrap();
    let oid: u32 = client
        .query_one("SELECT 'typed_columns'::regclass::oid", &[])
        .await
        .unwrap()
        .get(0);
    let calls = AtomicUsize::new(0);
    let value = CountedInteger {
        value: 4,
        calls: &calls,
    };
    let stream = client
        .query_typed_builtin_events(
            "SELECT n, label AS \"Exact label\" FROM typed_columns WHERE n = $1",
            [(&value, Type::INT4)],
        )
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let result = drain(stream).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].get::<_, i32>(0), 4);
    assert_eq!(result.rows[0].get::<_, &str>(1), "four");
    assert_eq!(result.tag.as_deref(), Some("SELECT 1"));
    assert_eq!(result.state, TransactionState::Idle);
    assert!(result.errors.is_empty());
    let expected = vec![
        ("n".into(), Type::INT4.oid(), Some(oid), Some(1), -1),
        (
            "Exact label".into(),
            Type::VARCHAR.oid(),
            Some(oid),
            Some(2),
            11,
        ),
    ];
    assert_eq!(result.columns, Some(expected.clone()));
    let zero = drain(query(
        &client,
        "SELECT n, label AS \"Exact label\" FROM typed_columns WHERE false",
    ))
    .await;
    assert!(zero.rows.is_empty() && zero.errors.is_empty());
    assert_eq!(zero.tag.as_deref(), Some("SELECT 0"));
    assert_eq!(zero.columns, Some(expected));
    client
        .batch_execute(
            "CREATE DOMAIN pg_temp.typed_amount AS integer; \
            ALTER TABLE typed_columns ADD COLUMN declared pg_temp.typed_amount",
        )
        .await
        .unwrap();
    let domain_oid: u32 = client
        .query_one("SELECT 'pg_temp.typed_amount'::regtype::oid", &[])
        .await
        .unwrap()
        .get(0);
    let oids = [oid];
    let facts = drain(
        client
            .query_typed_builtin_events(
                "SELECT a.attnum, a.attname::text, a.atttypid, t.typtype, a.attnotnull \
         FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_type t ON t.oid = a.atttypid \
         WHERE a.attrelid = ANY($1) AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum",
                [(&oids.as_slice(), Type::OID_ARRAY)],
            )
            .unwrap(),
    )
    .await;
    assert_eq!(facts.tag.as_deref(), Some("SELECT 3"));
    assert!(facts.errors.is_empty());
    assert_eq!(facts.state, TransactionState::Idle);
    assert_eq!(facts.rows[2].get::<_, i16>(0), 3);
    assert_eq!(facts.rows[2].get::<_, &str>(1), "declared");
    assert_eq!(facts.rows[2].get::<_, u32>(2), domain_oid);
    assert_eq!(facts.rows[2].get::<_, i8>(3), b'd' as i8);
    assert!(!facts.rows[2].get::<_, bool>(4));
    assert!(Type::from_oid(domain_oid).is_none());
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn empty_nodata_zero_column_rows_and_zero_affected_counts_are_distinct() {
    let (client, connection) = client().await;
    for sql in ["", "-- ordinary empty query\n"] {
        let result = drain(query(&client, sql)).await;
        assert!(
            result.empty
                && result.tag.is_none()
                && result.rows.is_empty()
                && result.errors.is_empty()
        );
        assert_eq!(result.columns, Some(vec![]));
        assert_eq!(result.state, TransactionState::Idle);
    }
    let ddl = drain(query(&client, "CREATE TEMP TABLE typed_empty(n integer)")).await;
    assert_eq!(ddl.tag.as_deref(), Some("CREATE TABLE"));
    assert_eq!(ddl.columns, Some(vec![]));
    assert!(!ddl.empty && ddl.rows.is_empty() && ddl.errors.is_empty());
    let zero = drain(query(
        &client,
        "INSERT INTO typed_empty SELECT 1 WHERE false",
    ))
    .await;
    assert_eq!(zero.tag.as_deref(), Some("INSERT 0 0"));
    assert_eq!(zero.columns, Some(vec![]));
    let row = drain(query(
        &client,
        "SELECT FROM typed_empty RIGHT JOIN (SELECT 1) x ON false",
    ))
    .await;
    assert_eq!(row.rows.len(), 1);
    assert!(row.rows[0].is_empty());
    assert_eq!(row.tag.as_deref(), Some("SELECT 1"));
    assert_eq!(row.columns, Some(vec![]));
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn parse_and_execution_errors_retain_state_and_earlier_rows() {
    let (client, connection) = client().await;
    let parse = drain(query(
        &client,
        "SELECT missing_column FROM (SELECT 1 AS n) x",
    ))
    .await;
    assert!(parse.rows.is_empty() && parse.tag.is_none());
    assert!(parse.columns.is_none());
    assert_eq!(parse.errors.len(), 1);
    assert_eq!(parse.errors[0].code().unwrap().code(), "42703");
    assert_eq!(parse.state, TransactionState::Idle);
    client
        .batch_execute("BEGIN; SAVEPOINT query_scope")
        .await
        .unwrap();
    let execution = drain(query(
        &client,
        "SELECT 10/(2-n) AS value FROM generate_series(1,2) AS x(n)",
    ))
    .await;
    assert_eq!(execution.rows.len(), 1);
    assert_eq!(execution.rows[0].get::<_, i32>(0), 10);
    assert!(execution.tag.is_none());
    assert!(execution.columns.is_some());
    assert_eq!(execution.errors.len(), 1);
    assert_eq!(execution.errors[0].code().unwrap().code(), "22012");
    assert_eq!(execution.state, TransactionState::FailedTransaction);
    let aborted = drain(query(&client, "SELECT 1")).await;
    assert!(aborted.columns.is_none() && aborted.rows.is_empty() && aborted.tag.is_none());
    assert_eq!(aborted.errors[0].code().unwrap().code(), "25P02");
    assert_eq!(aborted.state, TransactionState::FailedTransaction);
    client
        .batch_execute("ROLLBACK TO SAVEPOINT query_scope; RELEASE SAVEPOINT query_scope")
        .await
        .unwrap();
    let next = drain(query(&client, "SELECT 7 AS value")).await;
    assert_eq!(next.rows[0].get::<_, i32>(0), 7);
    assert_eq!(next.state, TransactionState::Transaction);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[derive(Debug)]
struct DomainInteger(i32);

impl ToSql for DomainInteger {
    fn to_sql(
        &self,
        _: &Type,
        output: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn StdError + Sync + Send>> {
        self.0.to_sql(&Type::INT4, output)
    }
    fn accepts(ty: &Type) -> bool {
        matches!(ty.kind(), Kind::Domain(base) if *base == Type::INT4)
    }
    to_sql_checked!();
}

#[tokio::test]
async fn domain_input_failure_before_description_remains_a_backend_error() {
    let (client, connection) = client().await;
    client
        .batch_execute(
            "CREATE TEMP TABLE typed_domain(n integer); \
        CREATE DOMAIN pg_temp.positive AS integer CHECK (VALUE > 0); BEGIN; SAVEPOINT input_scope",
        )
        .await
        .unwrap();
    let oid: u32 = client
        .query_one("SELECT 'pg_temp.positive'::regtype::oid", &[])
        .await
        .unwrap()
        .get(0);
    let ty = Type::new(
        "positive".into(),
        oid,
        Kind::Domain(Type::INT4),
        "pg_temp".into(),
    );
    let failed = drain(
        client
            .query_typed_builtin_events(
                "INSERT INTO typed_domain VALUES ($1) RETURNING n",
                [(&DomainInteger(-1), ty.clone())],
            )
            .unwrap(),
    )
    .await;
    assert!(failed.columns.is_none() && failed.rows.is_empty() && failed.tag.is_none());
    assert_eq!(failed.errors.len(), 1);
    assert_eq!(failed.errors[0].code().unwrap().code(), "23514");
    assert_eq!(failed.state, TransactionState::FailedTransaction);
    client
        .batch_execute("ROLLBACK TO SAVEPOINT input_scope; RELEASE SAVEPOINT input_scope")
        .await
        .unwrap();
    let next = drain(
        client
            .query_typed_builtin_events(
                "INSERT INTO typed_domain VALUES ($1) RETURNING n",
                [(&DomainInteger(3), ty)],
            )
            .unwrap(),
    )
    .await;
    assert!(next.errors.is_empty());
    assert_eq!(next.rows[0].get::<_, i32>(0), 3);
    assert_eq!(next.tag.as_deref(), Some("INSERT 0 1"));
    assert_eq!(next.state, TransactionState::Transaction);
    client.batch_execute("COMMIT").await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn deferred_sync_failure_retains_description_rows_and_completion_tag() {
    let (client, connection) = client().await;
    client
        .batch_execute(
            "CREATE TEMP TABLE typed_deferred(n integer UNIQUE DEFERRABLE INITIALLY DEFERRED); \
        INSERT INTO typed_deferred VALUES (1)",
        )
        .await
        .unwrap();
    for sql in [
        "INSERT INTO typed_deferred VALUES (1) RETURNING n",
        "INSERT INTO typed_deferred VALUES (1)",
    ] {
        let result = drain(query(&client, sql)).await;
        assert_eq!(result.tag.as_deref(), Some("INSERT 0 1"));
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].code().unwrap().code(), "23505");
        assert_eq!(result.state, TransactionState::Idle);
        if sql.ends_with("RETURNING n") {
            assert_eq!(result.rows.len(), 1);
            assert_eq!(result.rows[0].get::<_, i32>(0), 1);
            assert_eq!(result.columns.as_ref().unwrap().len(), 1);
        } else {
            assert!(result.rows.is_empty());
            assert_eq!(result.columns, Some(vec![]));
        }
        assert_eq!(
            client
                .query_one("SELECT count(*) FROM typed_deferred", &[])
                .await
                .unwrap()
                .get::<_, i64>(0),
            1
        );
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn local_parameter_error_queues_no_partial_request() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE typed_encoding(n integer); BEGIN")
        .await
        .unwrap();
    assert!(
        client
            .query_typed_builtin_events(
                "INSERT INTO typed_encoding VALUES ($1)",
                [(&"ordinary text", Type::INT4)]
            )
            .is_err()
    );
    let result = drain(
        client
            .query_typed_builtin_events(
                "INSERT INTO typed_encoding VALUES ($1)",
                [(&4_i32, Type::INT4)],
            )
            .unwrap(),
    )
    .await;
    assert!(result.errors.is_empty());
    assert_eq!(result.tag.as_deref(), Some("INSERT 0 1"));
    assert_eq!(result.state, TransactionState::Transaction);
    client.batch_execute("COMMIT").await.unwrap();
    assert_eq!(
        client
            .query_one("SELECT array_agg(n) FROM typed_encoding", &[])
            .await
            .unwrap()
            .get::<_, Vec<i32>>(0),
        [4]
    );
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn custom_result_oid_fails_explicitly_even_with_zero_rows_and_cached_type() {
    let (client, connection) = client().await;
    client
        .batch_execute(
            "CREATE TEMP TABLE typed_enum_seed(n integer); \
        CREATE TYPE pg_temp.typed_enum AS ENUM ('value')",
        )
        .await
        .unwrap();
    let prepared = client
        .prepare("SELECT 'value'::pg_temp.typed_enum")
        .await
        .unwrap();
    let oid = prepared.columns()[0].type_().oid();
    assert!(Type::from_oid(oid).is_none());
    client.batch_execute("BEGIN").await.unwrap();
    for sql in [
        "SELECT 'value'::pg_temp.typed_enum",
        "SELECT 'value'::pg_temp.typed_enum WHERE false",
    ] {
        let mut stream = query(&client, sql);
        assert!(!stream.has_yielded());
        let error = stream.next().await.unwrap().unwrap_err();
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<UnsupportedBuiltinResultType>()
            .unwrap();
        assert_eq!(source.oid(), oid);
        assert!(stream.columns().is_none() && stream.has_yielded() && stream.is_terminated());
        assert!(stream.next().await.is_none());
        // This fixture owns its raw Client and independently drains a rollback.
        // The terminal query error itself supplied no readiness or cleanup.
        client.batch_execute("ROLLBACK; BEGIN").await.unwrap();
    }
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(prepared);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn queued_typed_and_prepared_requests_keep_their_own_rows_and_states() {
    let (client, connection) = client().await;
    let prepared = client
        .prepare("SELECT $1::smallint AS prepared")
        .await
        .unwrap();
    let first = client
        .query_typed_builtin_events("SELECT $1 AS first", [(&5_i32, Type::INT4)])
        .unwrap();
    let begin = query(&client, "BEGIN");
    let second = client
        .query_typed_builtin_events("SELECT $1 AS second", [(&"seven", Type::TEXT)])
        .unwrap();
    let third = client.query_events(&prepared, [&9_i16]).unwrap();
    let first = drain(first).await;
    assert_eq!(first.rows[0].get::<_, i32>(0), 5);
    assert_eq!(first.columns.as_ref().unwrap()[0].0, "first");
    assert_eq!(first.state, TransactionState::Idle);
    let begin = drain(begin).await;
    assert_eq!(begin.tag.as_deref(), Some("BEGIN"));
    assert_eq!(begin.state, TransactionState::Transaction);
    let second = drain(second).await;
    assert_eq!(second.rows[0].get::<_, &str>(0), "seven");
    assert_eq!(second.columns.as_ref().unwrap()[0].0, "second");
    assert_eq!(second.state, TransactionState::Transaction);
    let mut third = third;
    assert_eq!(
        match third.next().await.unwrap().unwrap() {
            QueryEvent::Row(row) => row.get::<_, i16>(0),
            other => panic!("{other:?}"),
        },
        9
    );
    assert!(
        matches!(third.next().await.unwrap().unwrap(), QueryEvent::CommandComplete(tag) if tag == "SELECT 1")
    );
    assert!(matches!(
        third.next().await.unwrap().unwrap(),
        QueryEvent::ReadyForQuery(TransactionState::Transaction)
    ));
    assert!(third.next().await.is_none());
    // Rows keep the correct local unnamed description even after later Parse.
    assert_eq!(first.rows[0].get::<_, i32>(0), 5);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(prepared);
    drop(client);
    connection.await.unwrap().unwrap();
}
