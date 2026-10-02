//! Ordinary portal protocol and representation fixtures; not semantic admission.

use bytes::{Bytes, BytesMut};
use darmok_execute::{
    NativeResultError, NativeResultUtc, NativeRowFormat, NativeStatementError, NativeStatementUtc,
};
use darmok_protocol::ColumnDefinition;
use darmok_types::{
    Value,
    mysql_const::{charset, field_type},
};
use futures_util::{StreamExt, stream::FusedStream};
use tokio_postgres::{
    Client, DescribedPortal, Error, NoTls, PortalBindMismatch, QueryEvent, Row, TransactionState,
};

async fn client() -> (Client, tokio::task::JoinHandle<Result<(), Error>>) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

fn column(kind: u8) -> ColumnDefinition {
    ColumnDefinition {
        catalog: Bytes::from_static(b"def"),
        schema: Bytes::new(),
        table: Bytes::new(),
        org_table: Bytes::new(),
        name: Bytes::from_static(b"result"),
        org_name: Bytes::new(),
        character_set: charset::BINARY,
        column_length: 11,
        column_type: kind,
        flags: 0,
        decimals: 0,
    }
}

struct Completion {
    rows: Vec<Row>,
    tag: Option<String>,
    empty: bool,
    suspended: bool,
    errors: Vec<Error>,
}

async fn execute(client: &Client, portal: &DescribedPortal, limit: i32) -> Completion {
    let mut stream = client.query_portal_events(portal, limit).unwrap();
    let mut result = Completion {
        rows: vec![],
        tag: None,
        empty: false,
        suspended: false,
        errors: vec![],
    };
    let mut ready = None;
    assert!(!stream.is_terminated());
    while let Some(event) = stream.next().await {
        assert!(ready.is_none(), "event after readiness");
        match event.unwrap() {
            QueryEvent::Row(row) => {
                assert!(result.tag.is_none() && !result.empty && !result.suspended);
                result.rows.push(row);
            }
            QueryEvent::CommandComplete(tag) => {
                assert!(result.tag.is_none() && !result.empty && !result.suspended);
                result.tag = Some(tag);
            }
            QueryEvent::EmptyQuery => {
                assert!(result.rows.is_empty() && result.tag.is_none() && !result.suspended);
                result.empty = true;
            }
            QueryEvent::PortalSuspended => {
                assert!(result.tag.is_none() && !result.empty && !result.suspended);
                result.suspended = true;
            }
            QueryEvent::BackendError(error) => result.errors.push(error),
            QueryEvent::ReadyForQuery(state) => ready = Some(state),
        }
    }
    assert_eq!(ready, Some(TransactionState::Transaction));
    assert!(stream.is_terminated());
    assert!(stream.next().await.is_none());
    result
}

#[tokio::test]
async fn bound_metadata_rows_and_exact_source_handles_are_checked_before_execution() {
    let (client, driver) = client().await;
    client.batch_execute("BEGIN; CREATE TEMP TABLE portal_values(n int4); INSERT INTO portal_values VALUES(42),(NULL)").await.unwrap();
    let statement = client
        .prepare("SELECT n AS \"Exact.Label\" FROM portal_values ORDER BY n NULLS LAST")
        .await
        .unwrap();
    let other = client
        .prepare("SELECT n AS \"Exact.Label\" FROM portal_values ORDER BY n NULLS LAST")
        .await
        .unwrap();
    let clone = statement.clone();
    let description = NativeStatementUtc::new(&statement).unwrap();
    let bindings = description.bind(&[]).unwrap();
    let portal = client
        .bind_described_builtin(bindings.statement(), bindings.parameters())
        .await
        .unwrap();
    assert_eq!(portal.ready_state(), TransactionState::Transaction);
    assert_eq!(portal.columns().unwrap()[0].name(), "Exact.Label");
    assert_eq!(
        portal.columns().unwrap()[0].table_oid(),
        statement.columns()[0].table_oid()
    );
    assert_eq!(portal.columns().unwrap()[0].column_id(), Some(1));
    assert!(portal.is_bound_from(&clone));
    assert!(!portal.is_bound_from(&other));
    assert!(matches!(
        NativeStatementUtc::new(&other)
            .unwrap()
            .check_portal(&portal),
        Err(NativeStatementError::PortalStatement)
    ));
    NativeStatementUtc::new(&clone)
        .unwrap()
        .check_portal(&portal)
        .unwrap();
    let checked = description.check_portal(&portal).unwrap();
    let columns = [column(field_type::LONG)];
    let output = NativeResultUtc::from_portal(checked, &columns).unwrap();
    let result = execute(&client, &portal, 0).await;
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.tag.as_deref(), Some("SELECT 2"));
    assert!(!result.empty && !result.suspended && result.errors.is_empty());
    assert_eq!(
        checked.decode_row(&result.rows[0]).unwrap(),
        [Value::Int(42)]
    );
    assert_eq!(checked.decode_row(&result.rows[1]).unwrap(), [Value::Null]);
    for row in &result.rows {
        assert!(std::ptr::eq(row.columns(), portal.columns().unwrap()));
    }
    for (format, expected) in [
        (NativeRowFormat::Text, vec![2, b'4', b'2', 0xfb]),
        (NativeRowFormat::Binary, vec![0, 0, 42, 0, 0, 0, 0, 4]),
    ] {
        let mut bytes = BytesMut::new();
        for row in &result.rows {
            output.encode_row(row, format, &mut bytes).unwrap();
        }
        assert_eq!(bytes.as_ref(), expected);
    }
    drop(result);
    drop(portal);
    drop(statement);
    drop(clone);
    drop(other);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    driver.await.unwrap().unwrap();
}

#[tokio::test]
async fn no_data_zero_columns_zero_rows_and_empty_query_have_distinct_receipts() {
    let (client, driver) = client().await;
    client
        .batch_execute("BEGIN; CREATE TEMP TABLE portal_no_data(n int4)")
        .await
        .unwrap();
    for (sql, columns, rows, tag, empty) in [
        (
            "INSERT INTO portal_no_data VALUES(7)",
            None,
            0,
            Some("INSERT 0 1"),
            false,
        ),
        (
            "SELECT FROM generate_series(1,2)",
            Some(0),
            2,
            Some("SELECT 2"),
            false,
        ),
        (
            "SELECT 1::int4 WHERE false",
            Some(1),
            0,
            Some("SELECT 0"),
            false,
        ),
        ("", None, 0, None, true),
    ] {
        let statement = client.prepare(sql).await.unwrap();
        let description = NativeStatementUtc::new(&statement).unwrap();
        let bindings = description.bind(&[]).unwrap();
        let portal = client
            .bind_described_builtin(bindings.statement(), bindings.parameters())
            .await
            .unwrap();
        assert_eq!(
            portal.columns().map(<[tokio_postgres::Column]>::len),
            columns,
            "{sql}"
        );
        let checked = description.check_portal(&portal).unwrap();
        match columns {
            None => assert!(matches!(
                NativeResultUtc::from_portal(checked, &[]),
                Err(NativeResultError::NoRowDescription)
            )),
            Some(0) => assert!(matches!(
                NativeResultUtc::from_portal(checked, &[]),
                Err(NativeResultError::EmptyDescription)
            )),
            Some(_) => {
                NativeResultUtc::from_portal(checked, &[column(field_type::LONG)]).unwrap();
            }
        }
        let result = execute(&client, &portal, 0).await;
        assert_eq!(result.rows.len(), rows);
        assert_eq!(result.tag.as_deref(), tag);
        assert_eq!(result.empty, empty);
        assert!(!result.suspended && result.errors.is_empty());
        for row in &result.rows {
            assert!(checked.decode_row(row).unwrap().is_empty());
        }
    }
    let count: i64 = client
        .query_one("SELECT count(*) FROM portal_no_data", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    driver.await.unwrap().unwrap();
}

#[tokio::test]
async fn ordinary_dml_is_bound_and_described_before_its_execute_is_queued() {
    let (client, driver) = client().await;
    client
        .batch_execute("BEGIN; CREATE TEMP TABLE portal_write(n int4)")
        .await
        .unwrap();
    let statement = client
        .prepare("INSERT INTO portal_write VALUES($1::int4) RETURNING n")
        .await
        .unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    let values = [Value::Int(7)];
    let bindings = description.bind(&values).unwrap();
    let portal = client
        .bind_described_builtin(bindings.statement(), bindings.parameters())
        .await
        .unwrap();
    let checked = description.check_portal(&portal).unwrap();
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM portal_write", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let result = execute(&client, &portal, 0).await;
    assert_eq!(
        checked.decode_row(&result.rows[0]).unwrap(),
        [Value::Int(7)]
    );
    assert_eq!(result.tag.as_deref(), Some("INSERT 0 1"));
    assert!(result.errors.is_empty());
    drop(result);
    drop(portal);
    drop(statement);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    driver.await.unwrap().unwrap();
}

async fn changed_origin(sql_kind: u8) {
    let (owner, owner_driver) = client().await;
    let (ddl, ddl_driver) = client().await;
    let pid: i32 = owner
        .query_one("SELECT pg_catalog.pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let source = format!("darmok_portal_{pid}");
    let moved = format!("darmok_portal_moved_{pid}");
    ddl.batch_execute(&format!("CREATE SCHEMA {source}; CREATE TABLE {source}.items(n pg_catalog.int4); INSERT INTO {source}.items VALUES(1)")).await.unwrap();
    owner
        .batch_execute("SET search_path = pg_catalog; BEGIN")
        .await
        .unwrap();
    let sql = match sql_kind {
        0 => format!("SELECT n FROM {source}.items"),
        1 => format!("SELECT n FROM {source}.items WHERE false"),
        2 => format!("INSERT INTO {source}.items VALUES(3) RETURNING n"),
        _ => unreachable!(),
    };
    let statement = owner.prepare(&sql).await.unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    let original = statement.columns()[0].table_oid().unwrap();
    ddl.batch_execute(&format!("ALTER SCHEMA {source} RENAME TO {moved}; CREATE SCHEMA {source}; CREATE TABLE {source}.items(n pg_catalog.int4); INSERT INTO {source}.items VALUES(2)")).await.unwrap();
    let replacement: u32 = ddl
        .query_one(
            "SELECT pg_catalog.to_regclass($1::pg_catalog.text)::pg_catalog.oid",
            &[&format!("{source}.items")],
        )
        .await
        .unwrap()
        .get(0);
    owner
        .batch_execute("CREATE TEMP TABLE portal_lookup_change(n pg_catalog.int4)")
        .await
        .unwrap();
    let bindings = description.bind(&[]).unwrap();
    let portal = owner
        .bind_described_builtin(bindings.statement(), bindings.parameters())
        .await
        .unwrap();
    let observed = portal.columns().unwrap()[0].table_oid().unwrap();
    let rejected = description.check_portal(&portal).err();
    // Return original and observed facts before asserting, so cleanup is not
    // contingent on whether PostgreSQL agrees with the candidate hypothesis.
    let result = if sql_kind < 2 {
        Some(execute(&owner, &portal, 0).await)
    } else {
        None
    };
    let untouched: Vec<i32> = ddl
        .query(&format!("SELECT n FROM {source}.items ORDER BY n"), &[])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    drop(portal);
    drop(statement);
    owner.batch_execute("ROLLBACK").await.unwrap();
    ddl.batch_execute(&format!(
        "DROP SCHEMA {source} CASCADE; DROP SCHEMA {moved} CASCADE"
    ))
    .await
    .unwrap();
    drop(owner);
    drop(ddl);
    owner_driver.await.unwrap().unwrap();
    ddl_driver.await.unwrap().unwrap();
    assert_ne!(original, replacement);
    assert_eq!(observed, replacement);
    assert_eq!(
        rejected,
        Some(NativeStatementError::ColumnDescription {
            column: 0,
            field: "relation origin"
        })
    );
    assert_eq!(untouched, [2]);
    if let Some(result) = result {
        assert_eq!(result.rows.len(), if sql_kind == 0 { 1 } else { 0 });
        assert!(result.errors.is_empty());
        for row in result.rows {
            assert_eq!(row.columns()[0].table_oid(), Some(replacement));
            assert_eq!(row.get::<_, i32>(0), 2);
        }
    }
}

#[tokio::test]
async fn portal_origins_detect_rebinding_before_rows_or_dml_execution() {
    for sql_kind in 0..3 {
        changed_origin(sql_kind).await;
    }
}

#[tokio::test]
async fn local_parameter_errors_submit_nothing_and_leave_the_transaction_usable() {
    let (client, driver) = client().await;
    client
        .batch_execute("BEGIN; CREATE TEMP TABLE portal_params(n int4)")
        .await
        .unwrap();
    let statement = client
        .prepare("INSERT INTO portal_params VALUES($1::int4)")
        .await
        .unwrap();
    for wrong in [false, true] {
        let failure = if wrong {
            client
                .bind_described_builtin(&statement, [&"wrong type"])
                .await
                .unwrap_err()
        } else {
            client
                .bind_described_builtin(&statement, std::iter::empty::<&i32>())
                .await
                .unwrap_err()
        };
        assert!(
            !failure.submitted() && !failure.bind_complete() && !failure.description_observed()
        );
        assert!(failure.ready_state().is_none() && failure.mismatch().is_none());
        assert!(failure.representation_error().is_some());
        assert!(failure.backend_error().is_none() && failure.stream_error().is_none());
    }
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM portal_params", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let portal = client
        .bind_described_builtin(&statement, [&7i32])
        .await
        .unwrap();
    assert!(portal.columns().is_none());
    assert_eq!(
        execute(&client, &portal, 0).await.tag.as_deref(),
        Some("INSERT 0 1")
    );
    drop(portal);
    drop(statement);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    driver.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_planning_error_retains_failed_readiness_without_a_bound_portal() {
    let (client, driver) = client().await;
    client.batch_execute("BEGIN").await.unwrap();
    let statement = client.prepare("SELECT 1 / 0").await.unwrap();
    let failure = client
        .bind_described_builtin(&statement, std::iter::empty::<&i32>())
        .await
        .unwrap_err();
    assert!(failure.submitted());
    assert!(!failure.bind_complete() && !failure.description_observed());
    assert_eq!(
        failure.backend_error().unwrap().code().unwrap().code(),
        "22012"
    );
    assert_eq!(
        failure.ready_state(),
        Some(TransactionState::FailedTransaction)
    );
    assert!(
        failure.mismatch().is_none()
            && failure.representation_error().is_none()
            && failure.stream_error().is_none()
    );
    drop(statement);
    client.batch_execute("ROLLBACK; BEGIN").await.unwrap();
    let statement = client.prepare("SELECT 7").await.unwrap();
    let portal = client
        .bind_described_builtin(&statement, std::iter::empty::<&i32>())
        .await
        .unwrap();
    assert_eq!(
        execute(&client, &portal, 0).await.rows[0].get::<_, i32>(0),
        7
    );
    drop(portal);
    drop(statement);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    driver.await.unwrap().unwrap();
}

#[tokio::test]
async fn unknown_native_result_oid_rejects_the_observed_description_and_drains_readiness() {
    let (client, driver) = client().await;
    client.batch_execute("BEGIN; CREATE TEMP TABLE portal_type_namespace(n int4); CREATE TYPE pg_temp.portal_colour AS ENUM('red')").await.unwrap();
    let statement = client
        .prepare("SELECT 'red'::pg_temp.portal_colour")
        .await
        .unwrap();
    let failure = client
        .bind_described_builtin(&statement, std::iter::empty::<&i32>())
        .await
        .unwrap_err();
    assert!(failure.submitted() && failure.bind_complete() && failure.description_observed());
    assert_eq!(failure.ready_state(), Some(TransactionState::Transaction));
    assert!(failure.representation_error().is_some());
    assert!(
        failure.backend_error().is_none()
            && failure.stream_error().is_none()
            && failure.mismatch().is_none()
    );
    drop(statement);
    client.batch_execute("ROLLBACK; BEGIN").await.unwrap();
    let statement = client.prepare("SELECT 7").await.unwrap();
    let portal = client
        .bind_described_builtin(&statement, std::iter::empty::<&i32>())
        .await
        .unwrap();
    assert_eq!(
        execute(&client, &portal, 0).await.rows[0].get::<_, i32>(0),
        7
    );
    drop(portal);
    drop(statement);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    driver.await.unwrap().unwrap();
}

#[tokio::test]
async fn implicit_transaction_portals_and_failed_transactions_never_return_success_receipts() {
    let (client, driver) = client().await;
    let statement = client.prepare("SELECT 7").await.unwrap();
    let failure = client
        .bind_described_builtin(&statement, std::iter::empty::<&i32>())
        .await
        .unwrap_err();
    assert!(failure.submitted() && failure.bind_complete() && failure.description_observed());
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert_eq!(
        failure.mismatch(),
        Some(&PortalBindMismatch::State {
            actual: TransactionState::Idle
        })
    );
    assert!(
        failure.backend_error().is_none()
            && failure.representation_error().is_none()
            && failure.stream_error().is_none()
    );
    client.batch_execute("BEGIN").await.unwrap();
    client.batch_execute("SELECT 1 / 0").await.unwrap_err();
    let failure = client
        .bind_described_builtin(&statement, std::iter::empty::<&i32>())
        .await
        .unwrap_err();
    assert!(failure.submitted());
    assert!(!failure.bind_complete() && !failure.description_observed());
    assert_eq!(
        failure.backend_error().unwrap().code().unwrap().code(),
        "25P02"
    );
    assert_eq!(
        failure.ready_state(),
        Some(TransactionState::FailedTransaction)
    );
    client.batch_execute("ROLLBACK; BEGIN").await.unwrap();
    let portal = client
        .bind_described_builtin(&statement, std::iter::empty::<&i32>())
        .await
        .unwrap();
    assert_eq!(
        execute(&client, &portal, 0).await.rows[0].get::<_, i32>(0),
        7
    );
    drop(portal);
    drop(statement);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    driver.await.unwrap().unwrap();
}

#[tokio::test]
async fn suspended_portals_resume_and_negative_limits_fail_before_submission() {
    let (client, driver) = client().await;
    client.batch_execute("BEGIN").await.unwrap();
    let statement = client
        .prepare("SELECT n FROM generate_series(1,5) AS n ORDER BY n")
        .await
        .unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    let bindings = description.bind(&[]).unwrap();
    let portal = client
        .bind_described_builtin(bindings.statement(), bindings.parameters())
        .await
        .unwrap();
    let checked = description.check_portal(&portal).unwrap();
    assert!(client.query_portal_events(&portal, -1).is_err());
    let first = execute(&client, &portal, 2).await;
    assert!(first.suspended && first.tag.is_none() && first.errors.is_empty());
    assert_eq!(
        first
            .rows
            .iter()
            .map(|r| checked.decode_row(r).unwrap())
            .collect::<Vec<_>>(),
        [vec![Value::Int(1)], vec![Value::Int(2)]]
    );
    let second = execute(&client, &portal, 0).await;
    assert_eq!(second.tag.as_deref(), Some("SELECT 5"));
    assert!(!second.suspended && second.errors.is_empty());
    assert_eq!(
        second
            .rows
            .iter()
            .map(|r| checked.decode_row(r).unwrap())
            .collect::<Vec<_>>(),
        [
            vec![Value::Int(3)],
            vec![Value::Int(4)],
            vec![Value::Int(5)]
        ]
    );
    // Stream ownership keeps the portal and source statement alive after both
    // external handles are released; only the request is queued before drops.
    let portal = client
        .bind_described_builtin(&statement, std::iter::empty::<&i32>())
        .await
        .unwrap();
    let mut stream = client.query_portal_events(&portal, 0).unwrap();
    drop(portal);
    drop(statement);
    let mut row_count = 0;
    while let Some(event) = stream.next().await {
        match event.unwrap() {
            QueryEvent::Row(_) => row_count += 1,
            QueryEvent::CommandComplete(tag) => assert_eq!(tag, "SELECT 5"),
            QueryEvent::ReadyForQuery(state) => assert_eq!(state, TransactionState::Transaction),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(row_count, 5);
    assert!(stream.is_terminated());
    drop(stream);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    driver.await.unwrap().unwrap();
}
