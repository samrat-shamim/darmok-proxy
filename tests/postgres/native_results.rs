use bytes::{Bytes, BytesMut};
use darmok_execute::{NativeResultError, NativeResultUtc, NativeRowFormat, NativeStatementUtc};
use darmok_protocol::ColumnDefinition;
use darmok_types::mysql_const::{charset, column_flag, field_type};
use tokio_postgres::{Client, NoTls, Row};

async fn client() -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

fn column(kind: u8, length: u32, decimals: u8, unsigned: bool) -> ColumnDefinition {
    ColumnDefinition {
        catalog: Bytes::from_static(b"def"),
        schema: Bytes::new(),
        table: Bytes::new(),
        org_table: Bytes::new(),
        name: Bytes::from_static(b"output"),
        org_name: Bytes::new(),
        character_set: charset::BINARY,
        column_length: length,
        column_type: kind,
        flags: if unsigned { column_flag::UNSIGNED } else { 0 },
        decimals,
    }
}

fn text_column(kind: u8) -> ColumnDefinition {
    let mut column = column(kind, 4096, 0, false);
    column.character_set = charset::UTF8MB4_GENERAL_CI;
    column
}

// Independent fixture assembly from literal expected text cells. It never
// calls the production row writer, decoder, formatter or metadata checker.
fn text_payload(cells: &[Option<&[u8]>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for cell in cells {
        match cell {
            None => bytes.push(0xfb),
            Some(value) => {
                if value.len() < 251 {
                    bytes.push(value.len() as u8);
                } else {
                    bytes.push(0xfc);
                    bytes.extend_from_slice(&u16::try_from(value.len()).unwrap().to_le_bytes());
                }
                bytes.extend_from_slice(value);
            }
        }
    }
    bytes
}

fn assert_payloads(
    result: &NativeResultUtc<'_, '_>,
    row: &Row,
    text_cells: &[Option<&[u8]>],
    binary: &[u8],
) {
    for (format, expected) in [
        (NativeRowFormat::Text, text_payload(text_cells)),
        (NativeRowFormat::Binary, binary.to_vec()),
    ] {
        let mut dst = BytesMut::from(b"prefix".as_slice());
        result.encode_row(row, format, &mut dst).unwrap();
        assert_eq!(&dst[..6], b"prefix");
        assert_eq!(&dst[6..], expected, "{format:?}");
    }
}

fn errors_preserve_buffer(
    result: &NativeResultUtc<'_, '_>,
    row: &Row,
    expected_column: usize,
    reason: &str,
) {
    for format in [NativeRowFormat::Text, NativeRowFormat::Binary] {
        let mut dst = BytesMut::from(b"prefix\0\xff".as_slice());
        let error = result.encode_row(row, format, &mut dst).unwrap_err();
        match error {
            NativeResultError::Encoding {
                column,
                reason: actual,
            } => {
                assert_eq!(column, expected_column);
                assert!(actual.contains(reason), "{actual}");
            }
            other => panic!("expected encoding failure, got {other:?}"),
        }
        assert_eq!(dst.as_ref(), b"prefix\0\xff");
    }
}

#[tokio::test]
async fn all_nineteen_native_scalar_types_have_exact_text_and_binary_payloads() {
    let (client, connection) = client().await;
    client
        .batch_execute("SET TIME ZONE 'Asia/Dhaka'")
        .await
        .unwrap();
    let statement = client
        .prepare(
            "SELECT true, '-32768'::smallint, '-2147483648'::integer,
             '-9223372036854775808'::bigint, '4294967295'::oid,
             1.25::real, -0.125::double precision, '123.450'::numeric(6,3),
             decode('ff0080','hex'), 'text'::text, 'v'::varchar(5),
             'pad'::char(4), 'Name'::name, '{\"k\":1,\"k\":2}'::json,
             '{\"k\":1,\"k\":2}'::jsonb, '0001-01-01'::date,
             '24:00:00'::time, '0001-01-01 00:00:00.123456'::timestamp,
             '2024-06-01 12:34:56.123456+05:45'::timestamptz",
        )
        .await
        .unwrap();
    let columns = vec![
        column(field_type::TINY, 1, 0, false),
        column(field_type::SHORT, 6, 0, false),
        column(field_type::LONG, 11, 0, false),
        column(field_type::LONGLONG, 20, 0, false),
        column(field_type::LONG, 10, 0, true),
        column(field_type::FLOAT, 12, 31, false),
        column(field_type::DOUBLE, 22, 31, false),
        column(field_type::NEWDECIMAL, 8, 3, false),
        column(field_type::BLOB, 3, 0, false),
        text_column(field_type::VAR_STRING),
        text_column(field_type::VARCHAR),
        text_column(field_type::STRING),
        text_column(field_type::VAR_STRING),
        column(field_type::JSON, 4096, 0, false),
        column(field_type::JSON, 4096, 0, false),
        column(field_type::DATE, 10, 0, false),
        column(field_type::TIME, 10, 0, false),
        column(field_type::DATETIME, 26, 6, false),
        column(field_type::DATETIME, 26, 6, false),
    ];
    let result =
        NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
    assert!(std::ptr::eq(result.columns(), columns.as_slice()));
    let row = client.query_one(&statement, &[]).await.unwrap();
    assert_payloads(
        &result,
        &row,
        &[
            Some(b"1"),
            Some(b"-32768"),
            Some(b"-2147483648"),
            Some(b"-9223372036854775808"),
            Some(b"4294967295"),
            Some(b"1.25"),
            Some(b"-0.125"),
            Some(b"123.450"),
            Some(&[0xff, 0, 0x80]),
            Some(b"text"),
            Some(b"v"),
            Some(b"pad "),
            Some(b"Name"),
            Some(b"{\"k\":1,\"k\":2}"),
            Some(b"{\"k\": 2}"),
            Some(b"0001-01-01"),
            Some(b"24:00:00"),
            Some(b"0001-01-01 00:00:00.123456"),
            Some(b"2024-06-01 06:49:56.123456"),
        ],
        &[
            0, 0, 0, 0, // Header and three NULL bitmap bytes for nineteen columns.
            1, 0, 0x80, 0, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0xff, 0xff, 0xff, 0, 0,
            0xa0, 0x3f, 0, 0, 0, 0, 0, 0, 0xc0, 0xbf, 7, b'1', b'2', b'3', b'.', b'4', b'5', b'0',
            3, 0xff, 0, 0x80, 4, b't', b'e', b'x', b't', 1, b'v', 4, b'p', b'a', b'd', b' ', 4,
            b'N', b'a', b'm', b'e', 13, b'{', b'"', b'k', b'"', b':', b'1', b',', b'"', b'k', b'"',
            b':', b'2', b'}', 8, b'{', b'"', b'k', b'"', b':', b' ', b'2', b'}', 4, 1, 0, 1, 1, 8,
            0, 1, 0, 0, 0, 0, 0, 0, 11, 1, 0, 1, 1, 0, 0, 0, 0x40, 0xe2, 1, 0, 11, 0xe8, 7, 6, 1,
            6, 49, 56, 0x40, 0xe2, 1, 0,
        ],
    );
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn empty_and_null_results_cannot_hide_unsupported_metadata() {
    let (client, connection) = client().await;
    let statement = client
        .prepare("SELECT 1::integer WHERE false")
        .await
        .unwrap();
    let checked = NativeStatementUtc::new(&statement).unwrap();
    assert!(matches!(
        NativeResultUtc::new(checked, &[]),
        Err(NativeResultError::ColumnCount {
            expected: 1,
            actual: 0
        })
    ));
    let no_result = client.prepare("DO $$ BEGIN END $$").await.unwrap();
    assert!(matches!(
        NativeResultUtc::new(NativeStatementUtc::new(&no_result).unwrap(), &[]),
        Err(NativeResultError::EmptyDescription)
    ));
    let mut bad_charset = text_column(field_type::VAR_STRING);
    bad_charset.character_set = charset::LATIN1_SWEDISH_CI;
    let mut missing_charset = bad_charset.clone();
    missing_charset.character_set = 9999;
    let mut bytea_as_text = text_column(field_type::BLOB);
    bytea_as_text.decimals = 31;
    let mut zerofill = column(field_type::LONG, 11, 0, true);
    zerofill.flags |= column_flag::ZEROFILL;
    let mut enum_string = text_column(field_type::STRING);
    enum_string.flags |= column_flag::ENUM;
    let mut unsigned_float = column(field_type::DOUBLE, 22, 31, true);
    unsigned_float.flags |= column_flag::UNSIGNED;
    let mut not_null_null = column(field_type::NULL, 0, 0, false);
    not_null_null.flags |= column_flag::NOT_NULL;
    let cases = [
        ("integer", column(0xff, 0, 0, false)),
        ("text", column(field_type::BIT, 10, 0, false)),
        ("text", column(field_type::JSON, 10, 0, false)),
        ("text", bad_charset),
        ("text", missing_charset),
        ("bytea", bytea_as_text),
        ("real", column(field_type::FLOAT, 12, 0, false)),
        ("double precision", unsigned_float),
        ("numeric", column(field_type::NEWDECIMAL, 1, 2, false)),
        ("numeric", column(field_type::NEWDECIMAL, 68, 30, false)),
        ("numeric", column(field_type::DECIMAL, 33, 31, true)),
        ("date", column(field_type::DATE, 10, 1, false)),
        ("time", column(field_type::TIME, 17, 7, false)),
        ("timestamp", column(field_type::TIMESTAMP, 26, 6, false)),
        ("integer", zerofill),
        ("text", enum_string),
        ("integer", not_null_null),
    ];
    for (ty, metadata) in cases {
        // Identical descriptions with empty and NULL-only output must reject
        // at construction, before querying any rows.
        for empty in [false, true] {
            let statement = client
                .prepare(&format!(
                    "SELECT 7::integer, NULL::{ty}{}",
                    if empty { " WHERE false" } else { "" }
                ))
                .await
                .unwrap();
            let columns = [column(field_type::LONG, 11, 0, false), metadata.clone()];
            assert!(matches!(
                NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns),
                Err(NativeResultError::Metadata { column: 1, .. })
            ));
        }
    }
    let columns = [column(field_type::LONG, 11, 0, false)];
    NativeResultUtc::new(checked, &columns).unwrap();
    assert!(client.query(&statement, &[]).await.unwrap().is_empty());
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_description_and_decoder_errors_preserve_existing_output() {
    let (client, connection) = client().await;
    let statement = client
        .prepare("SELECT 7::integer AS first, 1::integer AS second")
        .await
        .unwrap();
    let columns = [
        column(field_type::LONG, 11, 0, false),
        column(field_type::LONG, 11, 0, false),
    ];
    let result =
        NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
    for sql in [
        "SELECT 7::integer AS first",
        "SELECT 7::integer AS first, 1::integer AS renamed",
        "SELECT 7::integer AS first, 1::bigint AS second",
    ] {
        let row = client.query_one(sql, &[]).await.unwrap();
        for format in [NativeRowFormat::Text, NativeRowFormat::Binary] {
            let mut dst = BytesMut::from(b"prefix".as_slice());
            assert!(matches!(
                result.encode_row(&row, format, &mut dst),
                Err(NativeResultError::Statement(_))
            ));
            assert_eq!(dst.as_ref(), b"prefix");
        }
    }
    for (expression, metadata) in [
        (
            "'NaN'::numeric",
            column(field_type::NEWDECIMAL, 66, 0, false),
        ),
        (
            "'Infinity'::numeric",
            column(field_type::NEWDECIMAL, 66, 0, false),
        ),
        (
            "1e65::numeric",
            column(field_type::NEWDECIMAL, 66, 0, false),
        ),
        (
            "'0.0000000000000000000000000000001'::numeric",
            column(field_type::NEWDECIMAL, 67, 30, false),
        ),
        ("'NaN'::real", column(field_type::FLOAT, 12, 31, false)),
        (
            "'-Infinity'::double precision",
            column(field_type::DOUBLE, 22, 31, false),
        ),
        (
            "'10000-01-01'::date",
            column(field_type::DATE, 10, 0, false),
        ),
        (
            "'infinity'::timestamp",
            column(field_type::DATETIME, 26, 6, false),
        ),
    ] {
        let statement = client
            .prepare(&format!("SELECT 7::integer, {expression}"))
            .await
            .unwrap();
        let columns = [column(field_type::LONG, 11, 0, false), metadata];
        let result =
            NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
        let row = client.query_one(&statement, &[]).await.unwrap();
        for format in [NativeRowFormat::Text, NativeRowFormat::Binary] {
            let mut dst = BytesMut::from(b"prefix".as_slice());
            let error = result.encode_row(&row, format, &mut dst).unwrap_err();
            assert!(
                matches!(error, NativeResultError::Statement(_)),
                "{error:?}"
            );
            assert_eq!(dst.as_ref(), b"prefix");
        }
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn integer_ranges_include_int24_and_year_without_display_width_truncation() {
    let (client, connection) = client().await;
    let statement = client
        .prepare("SELECT 7::integer, $1::bigint")
        .await
        .unwrap();
    for (kind, unsigned, valid, binary_value, invalid) in [
        (field_type::TINY, false, -128_i64, vec![0x80], 128),
        (field_type::TINY, true, 255, vec![0xff], -1),
        (field_type::SHORT, false, -32768, vec![0, 0x80], 32768),
        (field_type::SHORT, true, 65535, vec![0xff, 0xff], 65536),
        (
            field_type::INT24,
            false,
            -8388608,
            vec![0, 0, 0x80, 0xff],
            8388608,
        ),
        (
            field_type::INT24,
            true,
            16777215,
            vec![0xff, 0xff, 0xff, 0],
            16777216,
        ),
        (
            field_type::LONG,
            false,
            -2147483648,
            vec![0, 0, 0, 0x80],
            2147483648,
        ),
        (
            field_type::LONG,
            true,
            4294967295,
            vec![0xff, 0xff, 0xff, 0xff],
            -1,
        ),
        (
            field_type::LONGLONG,
            false,
            i64::MIN,
            vec![0, 0, 0, 0, 0, 0, 0, 0x80],
            0,
        ),
        (
            field_type::LONGLONG,
            true,
            i64::MAX,
            vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f],
            -1,
        ),
        (field_type::YEAR, false, 0, vec![0, 0], 1900),
        (field_type::YEAR, true, 2155, vec![0x6b, 8], 2156),
    ] {
        // Display width is deliberately one; integer representation is still
        // governed by its type, never truncated to that width.
        let columns = [
            column(field_type::LONG, 11, 0, false),
            column(kind, 1, 0, unsigned),
        ];
        let result =
            NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
        let row = client.query_one(&statement, &[&valid]).await.unwrap();
        let text = if kind == field_type::YEAR {
            format!("{valid:04}")
        } else {
            valid.to_string()
        };
        let mut binary = vec![0, 0, 7, 0, 0, 0];
        binary.extend_from_slice(&binary_value);
        assert_payloads(&result, &row, &[Some(b"7"), Some(text.as_bytes())], &binary);
        if kind != field_type::LONGLONG || unsigned {
            let row = client.query_one(&statement, &[&invalid]).await.unwrap();
            errors_preserve_buffer(
                &result,
                &row,
                1,
                if kind == field_type::YEAR {
                    "YEAR"
                } else {
                    "width or signedness"
                },
            );
        }
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn decimal_scale_precision_sign_and_fraction_only_values_are_exact() {
    let (client, connection) = client().await;
    for (expression, metadata, expected) in [
        (
            "'-99.90'::numeric",
            column(field_type::NEWDECIMAL, 6, 2, false),
            Some(b"-99.90".as_slice()),
        ),
        (
            "'0.12'::numeric",
            column(field_type::DECIMAL, 4, 2, false),
            Some(b"0.12".as_slice()),
        ),
        (
            "'0.00'::numeric",
            column(field_type::NEWDECIMAL, 3, 2, true),
            Some(b"0.00".as_slice()),
        ),
        (
            "'999'::numeric",
            column(field_type::NEWDECIMAL, 3, 0, true),
            Some(b"999".as_slice()),
        ),
        (
            "'-0.12'::numeric",
            column(field_type::NEWDECIMAL, 3, 2, true),
            None,
        ),
        (
            "'123.45'::numeric",
            column(field_type::NEWDECIMAL, 6, 2, false),
            None,
        ),
        (
            "'1.2'::numeric",
            column(field_type::NEWDECIMAL, 6, 2, false),
            None,
        ),
        (
            "'1.230'::numeric",
            column(field_type::NEWDECIMAL, 6, 2, false),
            None,
        ),
    ] {
        let statement = client
            .prepare(&format!("SELECT 7::integer, {expression}"))
            .await
            .unwrap();
        let columns = [column(field_type::LONG, 11, 0, false), metadata];
        let result =
            NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
        let row = client.query_one(&statement, &[]).await.unwrap();
        if let Some(expected) = expected {
            let mut binary = vec![0, 0, 7, 0, 0, 0, expected.len() as u8];
            binary.extend_from_slice(expected);
            assert_payloads(&result, &row, &[Some(b"7"), Some(expected)], &binary);
        } else {
            errors_preserve_buffer(&result, &row, 1, "decimal");
        }
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn float_narrowing_is_lossless_and_extreme_text_round_trips_preserve_bits() {
    let (client, connection) = client().await;
    let statement = client
        .prepare("SELECT 7::integer, $1::double precision")
        .await
        .unwrap();
    let columns = [
        column(field_type::LONG, 11, 0, false),
        column(field_type::FLOAT, 12, 31, false),
    ];
    let result =
        NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
    for (value, text, bytes) in [
        (1.5_f64, b"1.5".as_slice(), [0, 0, 0xc0, 0x3f]),
        (-0.0, b"-0".as_slice(), [0, 0, 0, 0x80]),
    ] {
        let row = client.query_one(&statement, &[&value]).await.unwrap();
        let mut binary = vec![0, 0, 7, 0, 0, 0];
        binary.extend_from_slice(&bytes);
        assert_payloads(&result, &row, &[Some(b"7"), Some(text)], &binary);
    }
    for value in [0.1_f64, f64::MAX, f64::from_bits(1)] {
        let row = client.query_one(&statement, &[&value]).await.unwrap();
        errors_preserve_buffer(&result, &row, 1, "change the native value");
    }
    for kind in [field_type::FLOAT, field_type::DOUBLE] {
        let statement = client
            .prepare(if kind == field_type::FLOAT {
                "SELECT $1::real"
            } else {
                "SELECT $1::double precision"
            })
            .await
            .unwrap();
        let columns = [column(kind, 1, 31, false)];
        let result =
            NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
        let cases = if kind == field_type::FLOAT {
            [
                f64::from(f32::from_bits(1)),
                f64::from(f32::MAX),
                f64::from(f32::MIN_POSITIVE),
                -0.0,
            ]
        } else {
            [f64::from_bits(1), -f64::from_bits(1), f64::MAX, -0.0]
        };
        for value in cases {
            let row = if kind == field_type::FLOAT {
                client
                    .query_one(&statement, &[&(value as f32)])
                    .await
                    .unwrap()
            } else {
                client.query_one(&statement, &[&value]).await.unwrap()
            };
            let mut dst = BytesMut::new();
            result
                .encode_row(&row, NativeRowFormat::Text, &mut dst)
                .unwrap();
            let mut payload = &dst[..];
            let length = darmok_protocol::read_lenenc_int(&mut payload).unwrap() as usize;
            assert_eq!(length, payload.len());
            let text = std::str::from_utf8(payload).unwrap();
            let recovered = if kind == field_type::FLOAT {
                f64::from(text.parse::<f32>().unwrap())
            } else {
                text.parse::<f64>().unwrap()
            };
            assert_eq!(recovered.to_bits(), value.to_bits());
            let mut dst = BytesMut::new();
            result
                .encode_row(&row, NativeRowFormat::Binary, &mut dst)
                .unwrap();
            assert_eq!(&dst[..2], &[0, 0]);
            if kind == field_type::FLOAT {
                assert_eq!(&dst[2..], &(value as f32).to_le_bytes());
            } else {
                assert_eq!(&dst[2..], &value.to_le_bytes());
            }
        }
    }
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn text_binary_json_padding_and_utf8_contracts_preserve_native_bytes() {
    let (client, connection) = client().await;
    let mut utf8mb3 = text_column(field_type::VAR_STRING);
    utf8mb3.character_set = charset::UTF8MB3_BIN;
    let mut short = utf8mb3.clone();
    short.column_length = 2;
    for (expression, metadata, expected) in [
        (
            "'\u{e000}'::text",
            utf8mb3.clone(),
            Some("\u{e000}".as_bytes()),
        ),
        ("'\u{1f680}'::text", utf8mb3, None),
        ("'\u{e000}'::text", short, None),
        (
            "'\u{1f680}'::text",
            text_column(field_type::BLOB),
            Some("\u{1f680}".as_bytes()),
        ),
        (
            "'native \u{e000}0'::text",
            column(field_type::BLOB, 10, 0, false),
            None,
        ),
        (
            "decode('ff00','hex')",
            column(field_type::VAR_STRING, 2, 31, false),
            Some(b"\xff\0".as_slice()),
        ),
        (
            "decode('ff00','hex')",
            column(field_type::BLOB, 1, 0, false),
            None,
        ),
        (
            "''::text",
            text_column(field_type::STRING),
            Some(b"".as_slice()),
        ),
        (
            "''::bytea",
            column(field_type::BLOB, 0, 0, false),
            Some(b"".as_slice()),
        ),
        (
            "'pad'::char(6)",
            text_column(field_type::STRING),
            Some(b"pad   ".as_slice()),
        ),
        (
            "'{ \"x\" : 1, \"x\" : 2 }'::json",
            text_column(field_type::VAR_STRING),
            Some(b"{ \"x\" : 1, \"x\" : 2 }".as_slice()),
        ),
        (
            "'{ \"x\" : 1, \"x\" : 2 }'::jsonb",
            column(field_type::JSON, 8, 0, false),
            Some(b"{\"x\": 2}".as_slice()),
        ),
    ] {
        let statement = client
            .prepare(&format!("SELECT 7::integer, {expression}"))
            .await
            .unwrap();
        let columns = [column(field_type::LONG, 11, 0, false), metadata];
        let result =
            NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
        let row = client.query_one(&statement, &[]).await.unwrap();
        if let Some(expected) = expected {
            let mut binary = vec![0, 0, 7, 0, 0, 0, expected.len() as u8];
            binary.extend_from_slice(expected);
            assert_payloads(&result, &row, &[Some(b"7"), Some(expected)], &binary);
        } else {
            for format in [NativeRowFormat::Text, NativeRowFormat::Binary] {
                let mut dst = BytesMut::from(b"prefix".as_slice());
                assert!(matches!(
                    result.encode_row(&row, format, &mut dst),
                    Err(NativeResultError::Encoding { column: 1, .. })
                ));
                assert_eq!(dst.as_ref(), b"prefix");
            }
        }
    }
    // A declared non-default utf8mb4 collation comes from the same profile,
    // not a hardcoded list restricted to the default IDs.
    let statement = client.prepare("SELECT '\u{1f680}'::text").await.unwrap();
    let mut metadata = text_column(field_type::VAR_STRING);
    metadata.character_set = 246; // utf8mb4_unicode_520_ci, profile-declared.
    let columns = [metadata];
    let result =
        NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
    assert_payloads(
        &result,
        &client.query_one(&statement, &[]).await.unwrap(),
        &[Some("\u{1f680}".as_bytes())],
        &[0, 0, 4, 0xf0, 0x9f, 0x9a, 0x80],
    );
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn temporal_fractional_precision_endpoints_and_utc_components_are_exact() {
    let (client, connection) = client().await;
    client
        .batch_execute("SET TIME ZONE 'Asia/Dhaka'")
        .await
        .unwrap();
    for (scale, time, micros) in [
        (0, "12:34:56", 0_u32),
        (1, "12:34:56.1", 100000),
        (2, "12:34:56.12", 120000),
        (3, "12:34:56.123", 123000),
        (4, "12:34:56.1234", 123400),
        (5, "12:34:56.12345", 123450),
        (6, "12:34:56.123456", 123456),
    ] {
        let statement = client
            .prepare(&format!(
                "SELECT '{time}'::time, '0001-01-01 {time}'::timestamp"
            ))
            .await
            .unwrap();
        let columns = [
            column(field_type::TIME, 17, scale, false),
            column(field_type::DATETIME, 26, scale, false),
        ];
        let result =
            NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
        let mut binary = vec![
            0,
            0,
            if micros == 0 { 8 } else { 12 },
            0,
            0,
            0,
            0,
            0,
            12,
            34,
            56,
        ];
        if micros != 0 {
            binary.extend_from_slice(&micros.to_le_bytes());
        }
        binary.extend_from_slice(&[if micros == 0 { 7 } else { 11 }, 1, 0, 1, 1, 12, 34, 56]);
        if micros != 0 {
            binary.extend_from_slice(&micros.to_le_bytes());
        }
        let datetime = format!("0001-01-01 {time}");
        assert_payloads(
            &result,
            &client.query_one(&statement, &[]).await.unwrap(),
            &[Some(time.as_bytes()), Some(datetime.as_bytes())],
            &binary,
        );
    }
    for (expression, kind) in [
        ("'12:34:56.123456'::time", field_type::TIME),
        (
            "'2024-01-01 12:34:56.123456'::timestamp",
            field_type::DATETIME,
        ),
        (
            "'2024-01-01 12:34:56.123456+00'::timestamptz",
            field_type::DATETIME,
        ),
    ] {
        let statement = client
            .prepare(&format!("SELECT 7::integer, {expression}"))
            .await
            .unwrap();
        for scale in 0..6 {
            let columns = [
                column(field_type::LONG, 11, 0, false),
                column(kind, 26, scale, false),
            ];
            let result =
                NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns)
                    .unwrap();
            errors_preserve_buffer(
                &result,
                &client.query_one(&statement, &[]).await.unwrap(),
                1,
                "fractional precision",
            );
        }
    }
    let statement = client.prepare("SELECT '00:00:00'::time, '24:00:00'::time, '9999-12-31'::date, '9999-12-31 23:59:59.999999'::timestamp").await.unwrap();
    let columns = [
        column(field_type::TIME, 17, 6, false),
        column(field_type::TIME, 17, 6, false),
        column(field_type::DATE, 10, 0, false),
        column(field_type::DATETIME, 26, 6, false),
    ];
    let result =
        NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
    assert_payloads(
        &result,
        &client.query_one(&statement, &[]).await.unwrap(),
        &[
            Some(b"00:00:00.000000"),
            Some(b"24:00:00.000000"),
            Some(b"9999-12-31"),
            Some(b"9999-12-31 23:59:59.999999"),
        ],
        &[
            0, 0, 0, 8, 0, 1, 0, 0, 0, 0, 0, 0, 4, 0x0f, 0x27, 12, 31, 11, 0x0f, 0x27, 12, 31, 23,
            59, 59, 0x3f, 0x42, 0x0f, 0,
        ],
    );
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn null_bitmap_crosses_byte_boundaries_and_not_null_is_checked_per_row() {
    let (client, connection) = client().await;
    let statement = client.prepare("SELECT NULL::integer, 1::integer, NULL::integer, 2::integer, NULL::integer, 3::integer, NULL::integer, 4::integer, NULL::integer, 5::integer").await.unwrap();
    let columns = vec![column(field_type::LONG, 11, 0, false); 10];
    let result =
        NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
    assert_payloads(
        &result,
        &client.query_one(&statement, &[]).await.unwrap(),
        &[
            None,
            Some(b"1"),
            None,
            Some(b"2"),
            None,
            Some(b"3"),
            None,
            Some(b"4"),
            None,
            Some(b"5"),
        ],
        &[
            0, 0x54, 0x05, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0,
        ],
    );
    let statement = client
        .prepare("SELECT 7::integer, $1::integer")
        .await
        .unwrap();
    let mut not_null = column(field_type::LONG, 11, 0, false);
    not_null.flags |= column_flag::NOT_NULL;
    let columns = [column(field_type::LONG, 11, 0, false), not_null];
    let result =
        NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
    assert_payloads(
        &result,
        &client.query_one(&statement, &[&Some(1_i32)]).await.unwrap(),
        &[Some(b"7"), Some(b"1")],
        &[0, 0, 7, 0, 0, 0, 1, 0, 0, 0],
    );
    errors_preserve_buffer(
        &result,
        &client.query_one(&statement, &[&None::<i32>]).await.unwrap(),
        1,
        "NOT NULL",
    );
    let columns = [
        column(field_type::LONG, 11, 0, false),
        column(field_type::NULL, 0, 0, false),
    ];
    let result =
        NativeResultUtc::new(NativeStatementUtc::new(&statement).unwrap(), &columns).unwrap();
    assert_payloads(
        &result,
        &client.query_one(&statement, &[&None::<i32>]).await.unwrap(),
        &[Some(b"7"), None],
        &[0, 8, 7, 0, 0, 0],
    );
    errors_preserve_buffer(
        &result,
        &client.query_one(&statement, &[&Some(1_i32)]).await.unwrap(),
        1,
        "field type",
    );
    drop(client);
    connection.await.unwrap().unwrap();
}
