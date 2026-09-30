use bytes::Bytes;
use darmok_execute::{NativeParamError, NativeParamUtc, decode_native_row_utc};
use darmok_types::Value;
use tokio_postgres::types::ToSql;
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
async fn typed_native_parameters_match_independent_backend_values() {
    let (client, connection) = client().await;
    client
        .batch_execute("SET TIME ZONE 'Asia/Dhaka'")
        .await
        .unwrap();
    let cases = [
        ("SELECT $1::bool, false", Value::Bool(false)),
        (
            "SELECT $1::smallint, '-32768'::smallint",
            Value::Int(-32768),
        ),
        (
            "SELECT $1::integer, '2147483647'::integer",
            Value::UInt(i32::MAX as u64),
        ),
        (
            "SELECT $1::bigint, '-9223372036854775808'::bigint",
            Value::Int(i64::MIN),
        ),
        (
            "SELECT $1::oid, '4294967295'::oid",
            Value::UInt(u64::from(u32::MAX)),
        ),
        ("SELECT $1::real, 1.25::real", Value::Float(1.25)),
        (
            "SELECT $1::double precision, -0.125::double precision",
            Value::Float(-0.125),
        ),
        (
            "SELECT $1::text, 'native \u{e000}0 \u{e000}E'::text",
            Value::String("native \u{e000}0 \u{e000}E".into()),
        ),
        (
            "SELECT $1::varchar(12), 'native'::varchar(12)",
            Value::String("native".into()),
        ),
        (
            "SELECT $1::char(6), 'pad'::char(6)",
            Value::String("pad".into()),
        ),
        (
            "SELECT $1::name, 'NativeName'::name",
            Value::String("NativeName".into()),
        ),
        (
            "SELECT $1::bytea, decode('0001ff','hex')",
            Value::Bytes(Bytes::from_static(&[0, 1, 0xff])),
        ),
        (
            "SELECT $1::json, '{\"k\":1,\"k\":2}'::json",
            Value::String("{\"k\":1,\"k\":2}".into()),
        ),
        (
            "SELECT $1::jsonb, '{\"k\":1,\"k\":2}'::jsonb",
            Value::String("{\"k\":1,\"k\":2}".into()),
        ),
        (
            "SELECT $1::date, '0001-01-01'::date",
            Value::Date {
                year: 1,
                month: 1,
                day: 1,
            },
        ),
        (
            "SELECT $1::date, '2024-02-29'::date",
            Value::Date {
                year: 2024,
                month: 2,
                day: 29,
            },
        ),
        (
            "SELECT $1::date, '9999-12-31'::date",
            Value::Date {
                year: 9999,
                month: 12,
                day: 31,
            },
        ),
        (
            "SELECT $1::time, '00:00:00'::time",
            Value::Time {
                negative: false,
                days: 0,
                hours: 0,
                minutes: 0,
                seconds: 0,
                micros: 0,
            },
        ),
        (
            "SELECT $1::time, '24:00:00'::time",
            Value::Time {
                negative: false,
                days: 1,
                hours: 0,
                minutes: 0,
                seconds: 0,
                micros: 0,
            },
        ),
        (
            "SELECT $1::time, '23:59:59.999999'::time",
            Value::Time {
                negative: false,
                days: 0,
                hours: 23,
                minutes: 59,
                seconds: 59,
                micros: 999999,
            },
        ),
        (
            "SELECT $1::timestamp, '0001-01-01 00:00:00.123456'::timestamp",
            Value::DateTime {
                year: 1,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
                micros: 123456,
            },
        ),
        (
            "SELECT $1::timestamp, '1999-12-31 23:59:59.999999'::timestamp",
            Value::DateTime {
                year: 1999,
                month: 12,
                day: 31,
                hour: 23,
                minute: 59,
                second: 59,
                micros: 999999,
            },
        ),
        (
            "SELECT $1::timestamp, '9999-12-31 23:59:59.999999'::timestamp",
            Value::DateTime {
                year: 9999,
                month: 12,
                day: 31,
                hour: 23,
                minute: 59,
                second: 59,
                micros: 999999,
            },
        ),
        (
            "SELECT $1::timestamptz, '2024-06-01 06:49:56.123456+00'::timestamptz",
            Value::DateTime {
                year: 2024,
                month: 6,
                day: 1,
                hour: 6,
                minute: 49,
                second: 56,
                micros: 123456,
            },
        ),
    ];
    for (sql, value) in cases {
        let parameter = NativeParamUtc::new(&value);
        let row = client.query_one(sql, &[&parameter]).await.unwrap();
        let values = decode_native_row_utc(&row).unwrap();
        assert_eq!(values[0], values[1], "query {sql}");
    }
    for sql in ["SELECT $1::real", "SELECT $1::double precision"] {
        let value = Value::Float(-0.0);
        let row = client
            .query_one(sql, &[&NativeParamUtc::new(&value)])
            .await
            .unwrap();
        let values = decode_native_row_utc(&row).unwrap();
        let Value::Float(actual) = values[0] else {
            panic!("float expected")
        };
        assert_eq!(actual, 0.0);
        assert!(actual.is_sign_negative());
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn numeric_parameters_preserve_scale_and_full_integer_range() {
    let (client, connection) = client().await;
    for input in [
        "0",
        "-0.000000",
        "+000123.4500",
        "-0000.00100",
        "0.000000000000000000000000000001",
        "99999999999999999999999999999999999999999999999999999999999999999",
        "12345678901234567890123456789012345.123456789012345678901234567890",
    ] {
        let value = Value::Decimal(input.into());
        let row = client
            .query_one(
                "SELECT $1::numeric, $2::text::numeric",
                &[&NativeParamUtc::new(&value), &input],
            )
            .await
            .unwrap();
        let values = decode_native_row_utc(&row).unwrap();
        assert_eq!(values[0], values[1], "decimal {input}");
    }
    for (value, input) in [
        (Value::Int(i64::MIN), "-9223372036854775808"),
        (Value::UInt(u64::MAX), "18446744073709551615"),
    ] {
        let row = client
            .query_one(
                "SELECT $1::numeric, $2::text::numeric",
                &[&NativeParamUtc::new(&value), &input],
            )
            .await
            .unwrap();
        let values = decode_native_row_utc(&row).unwrap();
        assert_eq!(values[0], values[1]);
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_parameters_preserve_supported_nulls_and_empty_values() {
    let (client, connection) = client().await;
    for target in [
        "bool",
        "smallint",
        "integer",
        "bigint",
        "oid",
        "real",
        "double precision",
        "numeric",
        "text",
        "varchar",
        "char",
        "name",
        "bytea",
        "json",
        "jsonb",
        "date",
        "time",
        "timestamp",
        "timestamptz",
    ] {
        let row = client
            .query_one(
                &format!("SELECT $1::{target}"),
                &[&NativeParamUtc::new(&Value::Null)],
            )
            .await
            .unwrap();
        assert_eq!(decode_native_row_utc(&row).unwrap(), vec![Value::Null]);
    }
    for (sql, value) in [
        ("SELECT $1::text", Value::String("".into())),
        ("SELECT $1::bytea", Value::Bytes(Bytes::new())),
    ] {
        let row = client
            .query_one(sql, &[&NativeParamUtc::new(&value)])
            .await
            .unwrap();
        assert_eq!(decode_native_row_utc(&row).unwrap(), vec![value]);
    }
    for target in ["uuid", "integer[]", "interval", "timetz"] {
        assert!(
            client
                .query_one(
                    &format!("SELECT $1::{target}"),
                    &[&NativeParamUtc::new(&Value::Null)],
                )
                .await
                .is_err()
        );
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn mixed_native_formats_and_native_numeric_typmods_keep_backend_semantics() {
    let (client, connection) = client().await;
    client
        .batch_execute("SET TIME ZONE 'Asia/Dhaka'")
        .await
        .unwrap();
    let values = [
        Value::Bool(true),
        Value::Decimal("+00012.3400".into()),
        Value::String("null".into()),
        Value::String("null".into()),
        Value::Bytes(Bytes::from_static(&[0, 0xff])),
        Value::Date {
            year: 1,
            month: 1,
            day: 1,
        },
        Value::DateTime {
            year: 9999,
            month: 12,
            day: 31,
            hour: 23,
            minute: 59,
            second: 59,
            micros: 999999,
        },
        Value::Null,
        Value::Null,
        Value::Null,
    ];
    let parameters: Vec<_> = values.iter().map(NativeParamUtc::new).collect();
    let references: Vec<&(dyn ToSql + Sync)> = parameters
        .iter()
        .map(|parameter| parameter as &(dyn ToSql + Sync))
        .collect();
    let actual = client
        .query_one(
            "SELECT $1::bool, $2::numeric(8,4), $3::jsonb, $4::json, $5::bytea,
        $6::date, $7::timestamptz, $8::numeric, $9::jsonb, $10::bigint",
            &references,
        )
        .await
        .unwrap();
    let expected = client
        .query_one(
            "SELECT true, 12.3400::numeric(8,4), 'null'::jsonb, 'null'::json,
        decode('00ff','hex'), '0001-01-01'::date,
        '9999-12-31 23:59:59.999999+00'::timestamptz,
        NULL::numeric, NULL::jsonb, NULL::bigint",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        decode_native_row_utc(&actual).unwrap(),
        decode_native_row_utc(&expected).unwrap()
    );

    for (sql, value, expected) in [
        ("SELECT $1::numeric(3,-2)", "12345", "12300"),
        ("SELECT $1::numeric(3,5)", "0.001234", "0.00123"),
        ("SELECT $1::numeric(3,1)", "1.25", "1.3"),
    ] {
        let value = Value::Decimal(value.into());
        let row = client
            .query_one(sql, &[&NativeParamUtc::new(&value)])
            .await
            .unwrap();
        assert_eq!(
            decode_native_row_utc(&row).unwrap(),
            vec![Value::Decimal(expected.into())]
        );
    }
    let value = Value::Decimal("999.9".into());
    let error = client
        .query_one("SELECT $1::numeric(3,1)", &[&NativeParamUtc::new(&value)])
        .await
        .unwrap_err();
    assert_eq!(error.code().unwrap().code(), "22003");

    for reference in [f32::MIN_POSITIVE, f32::MAX, f32::from_bits(0x3f80_0001)] {
        let value = Value::Float(f64::from(reference));
        let row = client
            .query_one(
                "SELECT $1::real, $2::real",
                &[&NativeParamUtc::new(&value), &reference],
            )
            .await
            .unwrap();
        let values = decode_native_row_utc(&row).unwrap();
        assert_eq!(values[0], values[1]);
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn ordinary_parameter_mismatches_fail_before_statement_effects() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE native_parameter_effects (value integer)")
        .await
        .unwrap();
    for (expression, value, expected) in [
        (
            "$1::smallint",
            Value::Int(32768),
            NativeParamError::OutOfRange {
                kind: "integer target range",
            },
        ),
        (
            "$1::bigint",
            Value::UInt(u64::MAX),
            NativeParamError::OutOfRange {
                kind: "integer target range",
            },
        ),
        (
            "$1::oid",
            Value::Int(-1),
            NativeParamError::OutOfRange {
                kind: "integer target range",
            },
        ),
        (
            "$1::bool",
            Value::Int(1),
            NativeParamError::TypeMismatch { oid: 16 },
        ),
        (
            "$1::integer",
            Value::String("123".into()),
            NativeParamError::TypeMismatch { oid: 23 },
        ),
        (
            "$1::real",
            Value::Float(1.1),
            NativeParamError::OutOfRange {
                kind: "exact REAL representation",
            },
        ),
        (
            "$1::double precision",
            Value::Float(f64::INFINITY),
            NativeParamError::OutOfRange {
                kind: "finite floating point",
            },
        ),
        (
            "$1::numeric",
            Value::Decimal("1e2".into()),
            NativeParamError::InvalidValue {
                kind: "plain decimal literal",
            },
        ),
        (
            "$1::numeric",
            Value::Decimal("0.0000000000000000000000000000000".into()),
            NativeParamError::OutOfRange {
                kind: "NUMERIC scale <= 30",
            },
        ),
        (
            "$1::numeric",
            Value::Decimal(
                "123456789012345678901234567890123456.123456789012345678901234567890".into(),
            ),
            NativeParamError::OutOfRange {
                kind: "NUMERIC precision <= 65",
            },
        ),
        (
            "$1::text",
            Value::String("has\0nul".into()),
            NativeParamError::InvalidValue {
                kind: "PostgreSQL text contains NUL",
            },
        ),
        (
            "$1::date",
            Value::Date {
                year: 0,
                month: 0,
                day: 0,
            },
            NativeParamError::OutOfRange {
                kind: "date year 1..9999",
            },
        ),
        (
            "$1::date",
            Value::Date {
                year: 2023,
                month: 2,
                day: 29,
            },
            NativeParamError::InvalidValue {
                kind: "Gregorian date",
            },
        ),
        (
            "$1::time",
            Value::Time {
                negative: true,
                days: 0,
                hours: 1,
                minutes: 0,
                seconds: 0,
                micros: 0,
            },
            NativeParamError::OutOfRange {
                kind: "native TIME fields",
            },
        ),
        (
            "$1::time",
            Value::Time {
                negative: false,
                days: 1,
                hours: 0,
                minutes: 0,
                seconds: 0,
                micros: 1,
            },
            NativeParamError::OutOfRange {
                kind: "TIME 00:00:00..24:00:00",
            },
        ),
        (
            "$1::timestamp",
            Value::DateTime {
                year: 2024,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 59,
                micros: 1_000_000,
            },
            NativeParamError::OutOfRange {
                kind: "datetime microseconds < 1000000",
            },
        ),
    ] {
        let sql =
            format!("INSERT INTO native_parameter_effects SELECT 1 WHERE {expression} IS NOT NULL");
        let error = client
            .execute(&sql, &[&NativeParamUtc::new(&value)])
            .await
            .unwrap_err();
        let cause = std::error::Error::source(&error).unwrap();
        assert_eq!(
            cause.downcast_ref::<NativeParamError>(),
            Some(&expected),
            "expression {expression}"
        );
    }
    let row = client
        .query_one("SELECT count(*) FROM native_parameter_effects", &[])
        .await
        .unwrap();
    assert_eq!(row.get::<_, i64>(0), 0);
    drop(client);
    connection.await.unwrap().unwrap();
}
