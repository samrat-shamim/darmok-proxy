use std::pin::pin;

use darmok_execute::{NativeParamError, NativeStatementError, NativeStatementUtc};
use darmok_types::Value;
use futures_util::StreamExt;
use tokio_postgres::types::Type;
use tokio_postgres::{Client, NoTls};

async fn client() -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

#[tokio::test]
async fn empty_results_still_have_checked_native_parameter_and_column_descriptions() {
    let (client, connection) = client().await;
    let cases = [
        ("bool", Type::BOOL),
        ("smallint", Type::INT2),
        ("integer", Type::INT4),
        ("bigint", Type::INT8),
        ("oid", Type::OID),
        ("real", Type::FLOAT4),
        ("double precision", Type::FLOAT8),
        ("numeric(8,4)", Type::NUMERIC),
        ("bytea", Type::BYTEA),
        ("text", Type::TEXT),
        ("varchar(12)", Type::VARCHAR),
        ("char(6)", Type::BPCHAR),
        ("name", Type::NAME),
        ("json", Type::JSON),
        ("jsonb", Type::JSONB),
        ("date", Type::DATE),
        ("time(3)", Type::TIME),
        ("timestamp(4)", Type::TIMESTAMP),
        ("timestamptz(5)", Type::TIMESTAMPTZ),
    ];
    let projection = cases
        .iter()
        .enumerate()
        .map(|(index, (name, _))| format!("${}::{name} AS \"slot.{index}\"", index + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let statement = client
        .prepare(&format!("SELECT {projection} WHERE false"))
        .await
        .unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    assert!(std::ptr::eq(description.statement(), &statement));
    assert_eq!(statement.params().len(), cases.len());
    assert_eq!(statement.columns().len(), cases.len());
    for (index, (_, ty)) in cases.iter().enumerate() {
        assert_eq!(&statement.params()[index], ty);
        let column = &statement.columns()[index];
        assert_eq!(column.type_(), ty);
        assert_eq!(column.name(), format!("slot.{index}"));
        assert_eq!(column.table_oid(), None);
        assert_eq!(column.column_id(), None);
    }
    assert_eq!(statement.columns()[7].type_modifier(), 4 + (8 << 16) + 4);
    assert_eq!(statement.columns()[10].type_modifier(), 16);
    assert_eq!(statement.columns()[11].type_modifier(), 10);
    assert_eq!(statement.columns()[16].type_modifier(), 3);
    assert_eq!(statement.columns()[17].type_modifier(), 4);
    assert_eq!(statement.columns()[18].type_modifier(), 5);
    let values = vec![Value::Null; cases.len()];
    let bindings = description.bind(&values).unwrap();
    assert_eq!(bindings.parameters().len(), cases.len());
    {
        let mut stream = pin!(
            client
                .query_raw(bindings.statement(), bindings.parameters())
                .await
                .unwrap()
        );
        assert!(stream.next().await.is_none());
        assert_eq!(stream.rows_affected(), Some(0));
    }
    drop(statement);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn unsupported_outputs_fail_before_empty_reads_or_returning_writes() {
    let (client, connection) = client().await;
    for (sql, oid) in [
        ("SELECT 1, NULL::uuid WHERE false", Type::UUID.oid()),
        (
            "SELECT 1, NULL::integer[] WHERE false",
            Type::INT4_ARRAY.oid(),
        ),
        ("SELECT 1, NULL::interval WHERE false", Type::INTERVAL.oid()),
    ] {
        let statement = client.prepare(sql).await.unwrap();
        assert_eq!(
            NativeStatementUtc::new(&statement).unwrap_err(),
            NativeStatementError::UnsupportedColumn { column: 1, oid }
        );
    }
    client
        .batch_execute(
            "CREATE TEMP TABLE preflight_write (
                id integer DEFAULT 1,
                value uuid DEFAULT '00000000-0000-0000-0000-000000000001'
            )",
        )
        .await
        .unwrap();
    let statement = client
        .prepare("INSERT INTO preflight_write DEFAULT VALUES RETURNING id, value")
        .await
        .unwrap();
    assert_eq!(
        NativeStatementUtc::new(&statement).unwrap_err(),
        NativeStatementError::UnsupportedColumn {
            column: 1,
            oid: Type::UUID.oid()
        }
    );
    // Preflight did not send Bind/Execute; preparation did not insert a row.
    let row = client
        .query_one("SELECT count(*) FROM preflight_write", &[])
        .await
        .unwrap();
    assert_eq!(row.get::<_, i64>(0), 0);
    let no_data = client
        .prepare("INSERT INTO preflight_write DEFAULT VALUES")
        .await
        .unwrap();
    let no_data_description = NativeStatementUtc::new(&no_data).unwrap();
    assert!(no_data_description.statement().columns().is_empty());
    assert_eq!(no_data_description.bind(&[]).unwrap().parameters().len(), 0);
    drop(no_data);
    drop(statement);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn declared_domain_parameters_are_distinct_from_reported_base_results() {
    let (client, connection) = client().await;
    client
        .batch_execute(
            "CREATE DOMAIN pg_temp.inner_decimal AS numeric(8,4);
             CREATE DOMAIN pg_temp.outer_decimal AS pg_temp.inner_decimal;
             CREATE DOMAIN pg_temp.array_domain AS integer[];
             CREATE TYPE pg_temp.numeric AS ENUM ('native')",
        )
        .await
        .unwrap();
    let statement = client
        .prepare("SELECT 12.3400::pg_temp.outer_decimal AS \"domain.value\"")
        .await
        .unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    assert_eq!(statement.columns()[0].type_(), &Type::NUMERIC);
    assert_eq!(statement.columns()[0].type_modifier(), 4 + (8 << 16) + 4);
    let row = client.query_one(&statement, &[]).await.unwrap();
    assert_eq!(
        description.decode_row(&row).unwrap(),
        vec![Value::Decimal("12.3400".into())]
    );
    for sql in [
        "SELECT $1::pg_temp.outer_decimal",
        "SELECT $1::pg_temp.numeric",
        "SELECT $1::uuid",
    ] {
        let statement = client.prepare(sql).await.unwrap();
        assert_eq!(
            NativeStatementUtc::new(&statement).unwrap_err(),
            NativeStatementError::UnsupportedParameter {
                parameter: 0,
                oid: statement.params()[0].oid()
            }
        );
    }
    let mixed = client
        .prepare("SELECT NULL::uuid, $1::integer, $2::uuid")
        .await
        .unwrap();
    assert_eq!(
        NativeStatementUtc::new(&mixed).unwrap_err(),
        NativeStatementError::UnsupportedParameter {
            parameter: 1,
            oid: Type::UUID.oid()
        }
    );
    drop(mixed);
    for sql in [
        "SELECT NULL::pg_temp.numeric WHERE false",
        "SELECT NULL::pg_temp.array_domain WHERE false",
    ] {
        let statement = client.prepare(sql).await.unwrap();
        assert_eq!(
            NativeStatementUtc::new(&statement).unwrap_err(),
            NativeStatementError::UnsupportedColumn {
                column: 0,
                oid: statement.columns()[0].type_().oid()
            }
        );
    }
    drop(statement);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn checked_bindings_use_dense_backend_arity_and_native_encoders() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE preflight_values (i integer, b boolean, v varchar(20))")
        .await
        .unwrap();
    let statement = client
        .prepare(
            "INSERT INTO preflight_values VALUES ($1, $2, $3) RETURNING v AS \"Label.λ\", i, b",
        )
        .await
        .unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    for actual in [0, 2, 4] {
        assert_eq!(
            description.bind(&vec![Value::Null; actual]).unwrap_err(),
            NativeStatementError::ParameterCount {
                expected: 3,
                actual
            }
        );
    }
    let values = [
        Value::Int(7),
        Value::Bool(true),
        Value::String("native".into()),
    ];
    let bindings = description.bind(&values).unwrap();
    {
        let mut stream = pin!(
            client
                .query_raw(bindings.statement(), bindings.parameters())
                .await
                .unwrap()
        );
        let row = stream.next().await.unwrap().unwrap();
        assert!(std::ptr::eq(statement.columns(), row.columns()));
        assert_eq!(row.columns()[0].name(), "Label.λ");
        assert_eq!(
            description.decode_row(&row).unwrap(),
            vec![
                Value::String("native".into()),
                Value::Int(7),
                Value::Bool(true)
            ]
        );
        assert!(stream.next().await.is_none());
        assert_eq!(stream.rows_affected(), Some(1));
    }
    let invalid_values = [Value::Int(i64::MAX), Value::Bool(false), Value::Null];
    let bindings = description.bind(&invalid_values).unwrap();
    let error = client
        .query_raw(bindings.statement(), bindings.parameters())
        .await
        .err()
        .unwrap();
    let cause = std::error::Error::source(&error).unwrap();
    assert!(matches!(
        cause.downcast_ref::<NativeParamError>(),
        Some(NativeParamError::OutOfRange { .. })
    ));
    let count = client
        .query_one("SELECT count(*) FROM preflight_values", &[])
        .await
        .unwrap();
    assert_eq!(count.get::<_, i64>(0), 1);
    drop(statement);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn driver_description_mismatches_fail_before_value_decoding() {
    let (client, connection) = client().await;
    client
        .batch_execute(
            "CREATE TEMP TABLE preflight_a (x integer, y integer);
             CREATE TEMP TABLE preflight_b (x integer);
             INSERT INTO preflight_a VALUES (1, 2);
             INSERT INTO preflight_b VALUES (3)",
        )
        .await
        .unwrap();
    for (expected, actual, field) in [
        ("SELECT 1 AS x", "SELECT 1 AS y", "name"),
        ("SELECT 1::integer AS x", "SELECT 1::bigint AS x", "type"),
        (
            "SELECT 'ab'::varchar(2) AS x",
            "SELECT 'ab'::varchar(3) AS x",
            "type modifier",
        ),
        (
            "SELECT x FROM preflight_a",
            "SELECT x FROM preflight_b",
            "relation origin",
        ),
        (
            "SELECT x FROM preflight_a",
            "SELECT y AS x FROM preflight_a",
            "attribute origin",
        ),
        (
            "SELECT 1 AS x",
            "SELECT x FROM preflight_a",
            "relation origin",
        ),
    ] {
        let statement = client.prepare(expected).await.unwrap();
        let description = NativeStatementUtc::new(&statement).unwrap();
        let row = client.query_one(actual, &[]).await.unwrap();
        assert_eq!(
            description.decode_row(&row).unwrap_err(),
            NativeStatementError::ColumnDescription { column: 0, field }
        );
    }
    let statement = client.prepare("SELECT 1 AS x").await.unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    let row = client
        .query_one("SELECT 1 AS x, 2 AS y", &[])
        .await
        .unwrap();
    assert_eq!(
        description.decode_row(&row).unwrap_err(),
        NativeStatementError::ColumnCount {
            expected: 1,
            actual: 2
        }
    );
    // Matching descriptions describe a layout, not statement identity.
    let row = client.query_one("SELECT 2 AS x", &[]).await.unwrap();
    assert_eq!(description.decode_row(&row).unwrap(), vec![Value::Int(2)]);
    let empty_statement = client.prepare("SELECT FROM preflight_a").await.unwrap();
    let empty_description = NativeStatementUtc::new(&empty_statement).unwrap();
    let empty_row = client.query_one(&empty_statement, &[]).await.unwrap();
    assert!(empty_description.decode_row(&empty_row).unwrap().is_empty());
    drop(empty_row);
    drop(empty_statement);
    drop(statement);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn supported_type_does_not_suppress_native_value_range_errors() {
    let (client, connection) = client().await;
    for sql in [
        "SELECT 'Infinity'::double precision",
        "SELECT 'NaN'::numeric",
        "SELECT repeat('9', 66)::numeric",
        "SELECT '10000-01-01'::date",
    ] {
        let statement = client.prepare(sql).await.unwrap();
        let description = NativeStatementUtc::new(&statement).unwrap();
        let row = client.query_one(&statement, &[]).await.unwrap();
        assert!(matches!(
            description.decode_row(&row),
            Err(NativeStatementError::Value(_))
        ));
    }
    let statement = client
        .prepare("SELECT 12.3400::numeric AS unconstrained, 123::numeric(4,-1) AS rounded")
        .await
        .unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    assert_eq!(statement.columns()[0].type_modifier(), -1);
    assert_eq!(statement.columns()[1].type_modifier(), 4 + (4 << 16) + 2047);
    let row = client.query_one(&statement, &[]).await.unwrap();
    assert_eq!(
        description.decode_row(&row).unwrap(),
        vec![
            Value::Decimal("12.3400".into()),
            Value::Decimal("120".into())
        ]
    );
    drop(row);
    drop(statement);
    drop(client);
    connection.await.unwrap().unwrap();
}
