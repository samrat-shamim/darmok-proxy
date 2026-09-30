use bytes::{Bytes, BytesMut};
use darmok_execute::{NativeValueError, decode_native_row_utc};
use darmok_protocol::resultset::encode_binary_row;
use darmok_types::mysql_const::{charset, column_flag, field_type};
use darmok_types::{ColumnMeta, Value};
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
async fn native_scalars_preserve_exact_values_without_sentinels_or_storage_escapes() {
    let (client, connection) = client().await;
    client
        .batch_execute("SET TIME ZONE 'Asia/Dhaka'")
        .await
        .unwrap();
    let row = client
        .query_one(
            "SELECT true, '-32768'::smallint, '-2147483648'::integer,
        '-9223372036854775808'::bigint, '4294967295'::oid,
        1.25::real, -0.125::double precision,
        '12345678901234567890123456789012345.123456789012345678901234567890'::numeric(65,30),
        'native \u{e000}0 \u{e000}E'::text, decode('0001e08000ff','hex'),
        '0001-01-01'::date, '0001-01-01 00:00:00.123456'::timestamp,
        '24:00:00'::time, '23:59:59.999999'::time,
        '2024-06-01 12:34:56.123456+05:45'::timestamptz,
        '{\"k\":1,\"k\":2}'::json, '{\"k\":1,\"k\":2}'::jsonb",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        decode_native_row_utc(&row).unwrap(),
        vec![
            Value::Bool(true),
            Value::Int(-32768),
            Value::Int(-2147483648),
            Value::Int(i64::MIN),
            Value::UInt(u64::from(u32::MAX)),
            Value::Float(1.25),
            Value::Float(-0.125),
            Value::Decimal(
                "12345678901234567890123456789012345.123456789012345678901234567890".into()
            ),
            Value::String("native \u{e000}0 \u{e000}E".into()),
            Value::Bytes(Bytes::from_static(&[0, 1, 0xe0, 0x80, 0, 0xff])),
            Value::Date {
                year: 1,
                month: 1,
                day: 1
            },
            Value::DateTime {
                year: 1,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
                micros: 123456
            },
            Value::Time {
                negative: false,
                days: 1,
                hours: 0,
                minutes: 0,
                seconds: 0,
                micros: 0
            },
            Value::Time {
                negative: false,
                days: 0,
                hours: 23,
                minutes: 59,
                seconds: 59,
                micros: 999999
            },
            Value::DateTime {
                year: 2024,
                month: 6,
                day: 1,
                hour: 6,
                minute: 49,
                second: 56,
                micros: 123456
            },
            Value::String("{\"k\":1,\"k\":2}".into()),
            Value::String("{\"k\": 2}".into()),
        ]
    );
    let row = client
        .query_one("SELECT ''::text, ''::bytea", &[])
        .await
        .unwrap();
    assert_eq!(
        decode_native_row_utc(&row).unwrap(),
        vec![Value::String("".into()), Value::Bytes(Bytes::new()),]
    );
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_text_padding_nulls_and_temporal_endpoints_remain_exact() {
    let (client, connection) = client().await;
    let row = client
        .query_one(
            "SELECT false, 'native'::varchar(12), 'pad'::char(6), 'NativeName'::name,
        '00:00:00'::time, '9999-12-31'::date,
        '1999-12-31 23:59:59.999999'::timestamp,
        '9999-12-31 23:59:59.999999'::timestamp,
        '9999-12-31 23:59:59.999999+00'::timestamptz,
        '-0'::real, '-0'::double precision",
            &[],
        )
        .await
        .unwrap();
    let values = decode_native_row_utc(&row).unwrap();
    assert_eq!(
        &values[..9],
        &[
            Value::Bool(false),
            Value::String("native".into()),
            Value::String("pad   ".into()),
            Value::String("NativeName".into()),
            Value::Time {
                negative: false,
                days: 0,
                hours: 0,
                minutes: 0,
                seconds: 0,
                micros: 0,
            },
            Value::Date {
                year: 9999,
                month: 12,
                day: 31,
            },
            Value::DateTime {
                year: 1999,
                month: 12,
                day: 31,
                hour: 23,
                minute: 59,
                second: 59,
                micros: 999999,
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
            Value::DateTime {
                year: 9999,
                month: 12,
                day: 31,
                hour: 23,
                minute: 59,
                second: 59,
                micros: 999999,
            },
        ]
    );
    for value in &values[9..] {
        let Value::Float(value) = value else {
            panic!("float expected")
        };
        assert_eq!(*value, 0.0);
        assert!(value.is_sign_negative());
    }
    let row = client
        .query_one(
            "SELECT NULL::bool, NULL::smallint, NULL::integer, NULL::bigint,
        NULL::oid, NULL::real, NULL::double precision, NULL::numeric,
        NULL::text, NULL::varchar, NULL::char, NULL::name, NULL::bytea,
        NULL::json, NULL::jsonb, NULL::date, NULL::time,
        NULL::timestamp, NULL::timestamptz",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(decode_native_row_utc(&row).unwrap(), vec![Value::Null; 19]);
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn exact_numeric_binary_decoding_matches_postgres_text_with_scale_intact() {
    let (client, connection) = client().await;
    for input in [
        "0",
        "-0.000000",
        "0.0000",
        "1",
        "-1",
        "10000",
        "100000000",
        "0.1",
        "0.0001",
        "0.00001",
        "12345.6000",
        "-9876543210987654321098765432109876543.123456789",
        "1e-30",
        "-1e-30",
        "1e30",
        "1.2300e3",
        "99999999999999999999999999999999999999999999999999999999999999999",
        "12345678901234567890123456789012345.123456789012345678901234567890",
    ] {
        let row = client
            .query_one(
                "SELECT $1::text::numeric, ($1::text::numeric)::text",
                &[&input],
            )
            .await
            .unwrap();
        let values = decode_native_row_utc(&row).unwrap();
        let Value::Decimal(actual) = &values[0] else {
            panic!("decimal expected")
        };
        let Value::String(expected) = &values[1] else {
            panic!("text expected")
        };
        assert_eq!(actual, expected, "input {input}");
    }
    for sql in [
        "WITH n AS (SELECT 12345::numeric(3,-2) AS value) SELECT value, value::text FROM n",
        "WITH n AS (SELECT 0.00123::numeric(3,5) AS value) SELECT value, value::text FROM n",
    ] {
        let row = client.query_one(sql, &[]).await.unwrap();
        let values = decode_native_row_utc(&row).unwrap();
        let [Value::Decimal(actual), Value::String(expected)] = &values[..] else {
            panic!("decimal and independent backend text expected")
        };
        assert_eq!(actual, expected, "query {sql}");
    }
    let row = client
        .query_one(
            "SELECT '123456789012345678901234567890123456.123456789012345678901234567890'::numeric",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        decode_native_row_utc(&row),
        Err(NativeValueError::ColumnValue {
            column: 0,
            cause: Box::new(NativeValueError::OutOfRange {
                kind: "NUMERIC precision <= 65",
            }),
        })
    );
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn unsupported_native_representations_return_errors_instead_of_text_or_wrapped_values() {
    let (client, connection) = client().await;
    for sql in [
        "SELECT NULL::uuid",
        "SELECT ARRAY[1,2]",
        "SELECT '12:00:00+01'::timetz",
        "SELECT '1 month'::interval",
        "SELECT 'NaN'::numeric",
        "SELECT 'Infinity'::numeric",
        "SELECT 'NaN'::double precision",
        "SELECT 'Infinity'::real",
        "SELECT 'infinity'::date",
        "SELECT '0001-01-01 BC'::date",
        "SELECT '10000-01-01'::timestamp",
        "SELECT 1e-31::numeric",
        "SELECT 1e65::numeric",
    ] {
        let row = client.query_one(sql, &[]).await.unwrap();
        assert!(decode_native_row_utc(&row).is_err(), "accepted {sql}");
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_values_encode_to_declared_mysql_binary_types_without_precision_loss() {
    let (client, connection) = client().await;
    let row = client
        .query_one(
            "SELECT '-32768'::smallint, 12.3400::numeric(10,4), '0001-01-01'::date,
        '24:00:00'::time, decode('00ff','hex'), NULL::bigint",
            &[],
        )
        .await
        .unwrap();
    let values = decode_native_row_utc(&row).unwrap();
    // Fixed fixture metadata exercises encoding only. Catalog-based metadata
    // construction is a separate gate and is not implemented by this decoder.
    let columns: Vec<_> = [
        (field_type::SHORT, column_flag::NUM, 6, 0),
        (field_type::NEWDECIMAL, column_flag::NUM, 12, 4),
        (field_type::DATE, 0, 10, 0),
        (field_type::TIME, 0, 10, 0),
        (
            field_type::BLOB,
            column_flag::BINARY | column_flag::BLOB,
            2,
            0,
        ),
        (field_type::LONGLONG, column_flag::NUM, 20, 0),
    ]
    .into_iter()
    .map(|(mysql_type, flags, max_length, decimals)| ColumnMeta {
        name: "fixture".into(),
        table: None,
        source: None,
        mysql_type,
        flags,
        charset_id: charset::BINARY,
        max_length,
        decimals,
    })
    .collect();
    let mut bytes = BytesMut::new();
    encode_binary_row(&values, &columns, &mut bytes).unwrap();
    let mut expected = vec![0, 0x80, 0, 0x80, 7]; // header, NULL bitmap, signed short, decimal length
    expected.extend_from_slice(b"12.3400");
    expected.extend_from_slice(&[4, 1, 0, 1, 1]); // year one date
    expected.extend_from_slice(&[8, 0, 1, 0, 0, 0, 0, 0, 0]); // 24-hour duration
    expected.extend_from_slice(&[2, 0, 0xff]);
    assert_eq!(&bytes[..], &expected);
    drop(client);
    connection.await.unwrap().unwrap();
}
