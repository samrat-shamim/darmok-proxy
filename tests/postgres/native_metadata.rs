use bytes::{Bytes, BytesMut};
use darmok_execute::{
    NativeMetadataError, NativeResultError, NativeResultUtc, NativeRowFormat, NativeStatementUtc,
    NativeTypeMetadataUtc,
};
use darmok_protocol::ColumnDefinition;
use darmok_types::mysql_const::{column_flag as flag, field_type as ty};
use futures_util::StreamExt;
use tokio_postgres::{Client, Column, NoTls, QueryEvent, TransactionState, types::Type};

async fn client() -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

fn definitions(columns: &[Column], text_collation: u16) -> Vec<ColumnDefinition> {
    columns
        .iter()
        .map(|column| {
            let metadata = NativeTypeMetadataUtc::from_column(column, text_collation).unwrap();
            // These fixture expressions have no admitted semantic origin or key
            // flags. Representation construction must not invent either of them.
            ColumnDefinition {
                catalog: Bytes::from_static(b"def"),
                schema: Bytes::new(),
                table: Bytes::new(),
                org_table: Bytes::new(),
                name: Bytes::copy_from_slice(column.name().as_bytes()),
                org_name: Bytes::new(),
                column_type: metadata.column_type(),
                character_set: metadata.character_set(),
                column_length: metadata.column_length(),
                decimals: metadata.decimals(),
                flags: metadata.flags(),
            }
        })
        .collect()
}

fn text_payload(cells: &[Option<&[u8]>]) -> Vec<u8> {
    let mut result = Vec::new();
    for cell in cells {
        match cell {
            None => result.push(0xfb),
            Some(value) => {
                assert!(value.len() < 251);
                result.push(value.len() as u8);
                result.extend_from_slice(value);
            }
        }
    }
    result
}

async fn execute_payloads(
    client: &Client,
    sql: &str,
    text_collation: u16,
) -> (Vec<ColumnDefinition>, Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let statement = client.prepare(sql).await.unwrap();
    let checked = NativeStatementUtc::new(&statement).unwrap();
    // Both descriptions independently produce the same representation facts;
    // only the actual bound one is used to encode the Execute rows.
    let prepared_definitions = definitions(statement.columns(), text_collation);
    let bindings = checked.bind(&[]).unwrap();
    let portal = client
        .bind_described_builtin(bindings.statement(), bindings.parameters())
        .await
        .unwrap();
    let bound = checked.check_portal(&portal).unwrap();
    let columns = definitions(bound.columns().unwrap(), text_collation);
    assert_eq!(prepared_definitions, columns);
    let result = NativeResultUtc::from_portal(bound, &columns).unwrap();
    let mut events = client.query_portal_events(&portal, 0).unwrap();
    let mut text = Vec::new();
    let mut binary = Vec::new();
    let mut tag = None;
    let mut ready = false;
    while let Some(event) = events.next().await {
        assert!(!ready);
        match event.unwrap() {
            QueryEvent::Row(row) => {
                assert!(tag.is_none());
                for (format, output) in [
                    (NativeRowFormat::Text, &mut text),
                    (NativeRowFormat::Binary, &mut binary),
                ] {
                    let mut buffer = BytesMut::from(b"prefix".as_slice());
                    result.encode_row(&row, format, &mut buffer).unwrap();
                    assert_eq!(&buffer[..6], b"prefix");
                    output.push(buffer[6..].to_vec());
                }
            }
            QueryEvent::CommandComplete(value) => {
                assert!(tag.replace(value).is_none());
            }
            QueryEvent::ReadyForQuery(state) => {
                assert_eq!(state, TransactionState::Transaction);
                ready = true;
            }
            other => panic!("unexpected native result event: {other:?}"),
        }
    }
    assert!(ready);
    assert_eq!(
        tag.as_deref(),
        Some(format!("SELECT {}", text.len()).as_str())
    );
    assert_eq!(text.len(), binary.len());
    (columns, text, binary)
}

#[tokio::test]
async fn generated_scalar_metadata_encodes_all_nineteen_native_types_and_nulls() {
    let (client, connection) = client().await;
    client
        .batch_execute("SET TIME ZONE 'Asia/Dhaka'; BEGIN")
        .await
        .unwrap();
    let (columns, text, binary) = execute_payloads(
        &client,
        "SELECT true AS \"Label.λ\", '-32768'::smallint, '-2147483648'::integer,
         '-9223372036854775808'::bigint, '4294967295'::oid, 1.25::real,
         -0.125::double precision, '123.450'::numeric(6,3),
         decode('ff0080','hex'), 'text'::text, 'v'::varchar(5), 'pad'::char(4),
         'Name'::name, '{\"k\":1,\"k\":2}'::json, '{\"k\":1,\"k\":2}'::jsonb,
         '0001-01-01'::date, '24:00:00'::time,
         '0001-01-01 00:00:00.123456'::timestamp,
         '2024-06-01 12:34:56.123456+05:45'::timestamptz",
        45,
    )
    .await;
    assert_eq!(columns.len(), 19);
    assert_eq!(columns[0].name.as_ref(), "Label.λ".as_bytes());
    let expected = [
        (ty::TINY, 63, 1, 0, flag::BINARY),
        (ty::SHORT, 63, 6, 0, flag::BINARY),
        (ty::LONG, 63, 11, 0, flag::BINARY),
        (ty::LONGLONG, 63, 20, 0, flag::BINARY),
        (ty::LONG, 63, 10, 0, flag::BINARY | flag::UNSIGNED),
        (ty::FLOAT, 63, 12, 31, flag::BINARY),
        (ty::DOUBLE, 63, 22, 31, flag::BINARY),
        (ty::NEWDECIMAL, 63, 8, 3, flag::BINARY),
        (ty::BLOB, 63, u32::MAX, 0, flag::BINARY | flag::BLOB),
        (ty::BLOB, 45, u32::MAX, 0, flag::BLOB),
        (ty::VAR_STRING, 45, 20, 0, 0),
        (ty::VAR_STRING, 45, 16, 0, 0),
        (ty::VAR_STRING, 45, u32::MAX, 0, 0),
        (ty::JSON, 63, u32::MAX, 0, flag::BINARY | flag::BLOB),
        (ty::JSON, 63, u32::MAX, 0, flag::BINARY | flag::BLOB),
        (ty::DATE, 63, 10, 0, flag::BINARY),
        (ty::TIME, 63, 17, 6, flag::BINARY),
        (ty::DATETIME, 63, 26, 6, flag::BINARY),
        (ty::DATETIME, 63, 26, 6, flag::BINARY),
    ];
    for (column, expected) in columns.iter().zip(expected) {
        assert_eq!(
            (
                column.column_type,
                column.character_set,
                column.column_length,
                column.decimals,
                column.flags
            ),
            expected
        );
        assert_eq!(
            column.flags & (flag::NOT_NULL | flag::PRI_KEY | flag::AUTO_INCREMENT),
            0
        );
    }
    assert_eq!(
        text,
        [text_payload(&[
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
            Some(b"24:00:00.000000"),
            Some(b"0001-01-01 00:00:00.123456"),
            Some(b"2024-06-01 06:49:56.123456"),
        ])]
    );
    assert_eq!(
        binary,
        [vec![
            0, 0, 0, 0, 1, 0, 0x80, 0, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0xff, 0xff,
            0xff, 0, 0, 0xa0, 0x3f, 0, 0, 0, 0, 0, 0, 0xc0, 0xbf, 7, b'1', b'2', b'3', b'.', b'4',
            b'5', b'0', 3, 0xff, 0, 0x80, 4, b't', b'e', b'x', b't', 1, b'v', 4, b'p', b'a', b'd',
            b' ', 4, b'N', b'a', b'm', b'e', 13, b'{', b'"', b'k', b'"', b':', b'1', b',', b'"',
            b'k', b'"', b':', b'2', b'}', 8, b'{', b'"', b'k', b'"', b':', b' ', b'2', b'}', 4, 1,
            0, 1, 1, 8, 0, 1, 0, 0, 0, 0, 0, 0, 11, 1, 0, 1, 1, 0, 0, 0, 0x40, 0xe2, 1, 0, 11,
            0xe8, 7, 6, 1, 6, 49, 56, 0x40, 0xe2, 1, 0,
        ]]
    );
    let (_, nulls, null_binary) = execute_payloads(
        &client,
        "SELECT NULL::bool, NULL::smallint, NULL::integer, NULL::bigint, NULL::oid,
         NULL::real, NULL::double precision, NULL::numeric(6,3), NULL::bytea,
         NULL::text, NULL::varchar(5), NULL::char(4), NULL::name, NULL::json,
         NULL::jsonb, NULL::date, NULL::time, NULL::timestamp, NULL::timestamptz",
        45,
    )
    .await;
    assert_eq!(nulls, [vec![0xfb; 19]]);
    assert_eq!(null_binary, [vec![0, 0xfc, 0xff, 0x1f]]);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn numeric_declarations_fail_before_empty_results_or_returning_writes() {
    let (client, connection) = client().await;
    client
        .batch_execute("CREATE TEMP TABLE metadata_writes(n numeric); BEGIN")
        .await
        .unwrap();
    for (declaration, precision, scale) in [
        ("numeric(66,0)", 66, 0),
        ("numeric(65,31)", 65, 31),
        ("numeric(2,3)", 2, 3),
        ("numeric(4,-2)", 4, -2),
    ] {
        for sql in [
            format!("SELECT NULL::{declaration} WHERE false"),
            format!("INSERT INTO metadata_writes VALUES(1) RETURNING n::{declaration}"),
        ] {
            let statement = client.prepare(&sql).await.unwrap();
            NativeStatementUtc::new(&statement).unwrap();
            assert_eq!(
                NativeTypeMetadataUtc::from_column(&statement.columns()[0], 45),
                Err(NativeMetadataError::NumericDeclaration { precision, scale })
            );
        }
    }
    let statement = client
        .prepare("INSERT INTO metadata_writes VALUES(1) RETURNING n")
        .await
        .unwrap();
    assert_eq!(
        NativeTypeMetadataUtc::from_column(&statement.columns()[0], 45),
        Err(NativeMetadataError::UnconstrainedNumeric)
    );
    drop(statement);
    let count: i64 = client
        .query_one("SELECT count(*) FROM metadata_writes", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    let (columns, text, _) = execute_payloads(
        &client,
        "SELECT '-0.12'::numeric(2,2), '-1'::numeric(1,0),
         '12345678901234567890123456789012345.123456789012345678901234567890'::numeric(65,30)",
        45,
    )
    .await;
    assert_eq!((columns[0].column_length, columns[0].decimals), (4, 2));
    assert_eq!((columns[1].column_length, columns[1].decimals), (2, 0));
    assert_eq!((columns[2].column_length, columns[2].decimals), (67, 30));
    assert_eq!(
        text,
        [text_payload(&[
            Some(b"-0.12"),
            Some(b"-1"),
            Some(b"12345678901234567890123456789012345.123456789012345678901234567890")
        ])]
    );
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_character_widths_preserve_utf8_binary_and_padding() {
    let (client, connection) = client().await;
    client.batch_execute("BEGIN").await.unwrap();
    for collation in [45, 46, 63] {
        let (columns, text, binary) = execute_payloads(
            &client,
            "SELECT '🖖λ'::varchar(2), 'x'::char(2), 'Ω'::name, '🖖'::varchar, 'a'::bpchar",
            collation,
        )
        .await;
        assert_eq!(columns[0].column_length, 8);
        assert_eq!(columns[1].column_length, 8);
        assert_eq!(columns[2].column_length, u32::MAX);
        assert_eq!(columns[3].column_length, u32::MAX);
        assert_eq!(columns[4].column_length, u32::MAX);
        assert_eq!(
            text,
            [text_payload(&[
                Some("🖖λ".as_bytes()),
                Some(b"x "),
                Some("Ω".as_bytes()),
                Some("🖖".as_bytes()),
                Some(b"a")
            ])]
        );
        let mut expected = vec![0, 0];
        expected.extend_from_slice(&text[0]);
        assert_eq!(binary, [expected]);
    }
    let (columns, text, _) = execute_payloads(&client, "SELECT 'λΩ'::varchar(2)", 33).await;
    assert_eq!(columns[0].column_length, 6);
    assert_eq!(text, [text_payload(&[Some("λΩ".as_bytes())])]);
    let statement = client.prepare("SELECT '🖖'::varchar(1)").await.unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    let bindings = description.bind(&[]).unwrap();
    let portal = client
        .bind_described_builtin(bindings.statement(), bindings.parameters())
        .await
        .unwrap();
    let bound = description.check_portal(&portal).unwrap();
    let columns = definitions(bound.columns().unwrap(), 33);
    let result = NativeResultUtc::from_portal(bound, &columns).unwrap();
    let mut events = client.query_portal_events(&portal, 0).unwrap();
    let QueryEvent::Row(row) = events.next().await.unwrap().unwrap() else {
        panic!("expected row")
    };
    for format in [NativeRowFormat::Text, NativeRowFormat::Binary] {
        let mut buffer = BytesMut::from(b"unchanged".as_slice());
        assert!(matches!(
            result.encode_row(&row, format, &mut buffer),
            Err(NativeResultError::Encoding { column: 0, .. })
        ));
        assert_eq!(buffer.as_ref(), b"unchanged");
    }
    assert!(
        matches!(events.next().await.unwrap().unwrap(), QueryEvent::CommandComplete(tag) if tag == "SELECT 1")
    );
    assert!(matches!(
        events.next().await.unwrap().unwrap(),
        QueryEvent::ReadyForQuery(TransactionState::Transaction)
    ));
    assert!(events.next().await.is_none());
    drop(events);
    drop(portal);
    drop(statement);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_temporal_precision_is_observed_and_not_replaced_by_zero() {
    let (client, connection) = client().await;
    client
        .batch_execute("SET TIME ZONE 'Asia/Dhaka'; BEGIN")
        .await
        .unwrap();
    for precision in 0..=6 {
        let sql = format!(
            "SELECT '12:34:56.123456'::time({precision}),
            '2024-01-02 12:34:56.123456'::timestamp({precision}),
            '2024-01-02 12:34:56.123456+05:45'::timestamptz({precision})"
        );
        let (columns, text, _) = execute_payloads(&client, &sql, 45).await;
        assert!(columns.iter().all(|column| column.decimals == precision));
        let fractions = ["", ".1", ".12", ".123", ".1235", ".12346", ".123456"];
        let cells = [
            format!("12:34:56{}", fractions[precision as usize]),
            format!("2024-01-02 12:34:56{}", fractions[precision as usize]),
            format!("2024-01-02 06:49:56{}", fractions[precision as usize]),
        ];
        assert_eq!(
            text,
            [text_payload(
                &cells
                    .iter()
                    .map(|cell| Some(cell.as_bytes()))
                    .collect::<Vec<_>>()
            )]
        );
    }
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_domains_and_later_ddl_use_reported_representation_facts() {
    let (client, connection) = client().await;
    client
        .batch_execute(
            "CREATE DOMAIN pg_temp.metadata_domain AS numeric(8,4);
        CREATE TEMP TABLE metadata_definitions(n pg_temp.metadata_domain, v varchar(2));
        INSERT INTO metadata_definitions VALUES(12.3400, 'λ'); BEGIN",
        )
        .await
        .unwrap();
    let (before, rows, _) =
        execute_payloads(&client, "SELECT n, v FROM metadata_definitions", 45).await;
    assert_eq!(
        (
            before[0].column_type,
            before[0].column_length,
            before[0].decimals
        ),
        (ty::NEWDECIMAL, 10, 4)
    );
    assert_eq!(before[1].column_length, 8);
    assert_eq!(
        rows,
        [text_payload(&[Some(b"12.3400"), Some("λ".as_bytes())])]
    );
    client
        .batch_execute(
            "COMMIT;
        ALTER TABLE metadata_definitions ALTER COLUMN n TYPE numeric(6,2),
        ALTER COLUMN v TYPE varchar(9); BEGIN",
        )
        .await
        .unwrap();
    let (after, rows, _) =
        execute_payloads(&client, "SELECT n, v FROM metadata_definitions", 45).await;
    assert_eq!(
        (
            after[0].column_type,
            after[0].column_length,
            after[0].decimals
        ),
        (ty::NEWDECIMAL, 8, 2)
    );
    assert_eq!(after[1].column_length, 36);
    assert_eq!(
        rows,
        [text_payload(&[Some(b"12.34"), Some("λ".as_bytes())])]
    );
    let statement = client
        .prepare("SELECT n + 1 FROM metadata_definitions")
        .await
        .unwrap();
    assert_eq!(statement.columns()[0].type_(), &Type::NUMERIC);
    assert_eq!(statement.columns()[0].type_modifier(), -1);
    assert_eq!(
        NativeTypeMetadataUtc::from_column(&statement.columns()[0], 45),
        Err(NativeMetadataError::UnconstrainedNumeric)
    );
    drop(statement);
    client
        .batch_execute("CREATE TYPE pg_temp.varchar AS ENUM ('native')")
        .await
        .unwrap();
    let custom = client
        .prepare("SELECT 'native'::pg_temp.varchar")
        .await
        .unwrap();
    assert_eq!(custom.columns()[0].type_().name(), "varchar");
    assert_eq!(
        NativeTypeMetadataUtc::from_column(&custom.columns()[0], 45),
        Err(NativeMetadataError::UnsupportedType {
            oid: custom.columns()[0].type_().oid()
        })
    );
    drop(custom);
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}
