use super::*;
use darmok_protocol::constants::{
    COM_STMT_CLOSE, COM_STMT_EXECUTE, COM_STMT_PREPARE, COM_STMT_RESET,
};

fn corpus() -> serde_json::Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/reference/mysql_prepared_local.json"
    )))
    .unwrap()
}

fn hex(value: &serde_json::Value) -> Vec<u8> {
    let value = value.as_str().unwrap();
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}

async fn expect_packets(input: &mut TcpStream, expected: &serde_json::Value) {
    for (index, expected) in expected.as_array().unwrap().iter().enumerate() {
        assert_eq!(packet(input, (index + 1) as u8).await, hex(expected));
    }
}

async fn execute(input: &mut TcpStream, bindings: &[u8]) {
    let mut body = vec![1, 0, 0, 0, 0, 1, 0, 0, 0];
    body.extend_from_slice(bindings);
    input
        .write_all(&encoded(COM_STMT_EXECUTE, &body))
        .await
        .unwrap();
}

async fn finish(mut input: TcpStream, owner: JoinHandle<FrontendReport>) -> FrontendReport {
    input.write_all(&encoded(COM_QUIT, b"")).await.unwrap();
    closed(&mut input).await;
    let result = owner.await.unwrap();
    report(&result, FrontendEnd::Quit, false);
    result
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_prepared_stock_literals_parameters_variables_and_frozen_parser_meaning() {
    let corpus = corpus();
    for (index, case) in corpus["cases"].as_array().unwrap().iter().enumerate() {
        let mut backend = connect_backend().await;
        let (mut session, globals) = fixture(false);
        prepare(&mut session, &mut backend, &globals, "SET sql_mode='' ").await;
        let (mut input, owner) = connection(session, backend, globals, BytesMut::new()).await;
        input
            .write_all(&encoded(
                COM_STMT_PREPARE,
                case["prepare"]["sql"].as_str().unwrap().as_bytes(),
            ))
            .await
            .unwrap();
        expect_packets(&mut input, &case["prepare"]["packets"]).await;
        for (execution_index, execution) in
            case["executions"].as_array().unwrap().iter().enumerate()
        {
            if execution_index == 1 && index != 1 && index != 4 {
                ok(
                    &command(
                        &mut input,
                        COM_QUERY,
                        b"SET sql_mode='ANSI_QUOTES,NO_BACKSLASH_ESCAPES'",
                    )
                    .await,
                    0x0202,
                );
                ok(
                    &command(&mut input, COM_QUERY, b"SET autocommit=0").await,
                    0x0200,
                );
            }
            if index == 1 && execution_index == 2 {
                assert_eq!(
                    command(&mut input, COM_STMT_RESET, &1u32.to_le_bytes()).await,
                    hex(&case["reset"])
                );
            }
            let bindings = execution.get("bindings").map(hex).unwrap_or_default();
            execute(&mut input, &bindings).await;
            let unsupported_int24 = case["binding_case"]
                .as_str()
                .is_some_and(|name| name.starts_with("int24-"));
            if unsupported_int24 || (index == 4 && matches!(execution_index, 2 | 5)) {
                let error = packet(&mut input, 1).await;
                assert_eq!(
                    &error[..9],
                    &[0xff, 0xd3, 0x04, b'#', b'4', b'2', b'0', b'0', b'0']
                );
            } else {
                expect_packets(&mut input, &execution["response"]["packets"]).await;
            }
        }
        input
            .write_all(&encoded(COM_STMT_CLOSE, &1u32.to_le_bytes()))
            .await
            .unwrap();
        // A response to CLOSE would appear here and break the error sequence.
        execute(&mut input, b"").await;
        let error = packet(&mut input, 1).await;
        assert_eq!(
            &error[..9],
            &[0xff, 0xdb, 0x04, b'#', b'H', b'Y', b'0', b'0', b'0']
        );
        let result = finish(input, owner).await;
        assert_eq!(result.session.statement_condition_count_u16(), 1);
    }
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_prepared_deprecated_eof_has_no_metadata_terminators() {
    let backend = connect_backend().await;
    let (session, globals) = fixture(true);
    let (mut input, owner) = connection(session, backend, globals, BytesMut::new()).await;
    let header = command(&mut input, COM_STMT_PREPARE, b"SELECT ? AS answer, NULL").await;
    assert_eq!(header, [0, 1, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0]);
    for sequence in 2..=4 {
        assert_eq!(packet(&mut input, sequence).await[0], 3);
    }
    execute(&mut input, &[0, 1, 8, 0, 42, 0, 0, 0, 0, 0, 0, 0]).await;
    assert_eq!(packet(&mut input, 1).await, [2]);
    assert_eq!(packet(&mut input, 2).await[0], 3);
    assert_eq!(packet(&mut input, 3).await[0], 3);
    assert_eq!(packet(&mut input, 4).await, [0, 8, 42, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(packet(&mut input, 5).await, [0xfe, 0, 0, 2, 0, 0, 0]);
    let result = finish(input, owner).await;
    assert_eq!(result.session.row_count, -1);
    assert_eq!(result.session.found_rows, 1);
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_prepared_capacity_close_and_diagnostics_are_owned_per_connection() {
    let backend = connect_backend().await;
    let (mut session, globals) = fixture(false);
    session.row_count = -1;
    session.found_rows = 2;
    session.last_insert_id = 29;
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let mut input = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (transport, _) = listener.accept().await.unwrap();
    let owner = FrontendConnection::new(
        transport,
        session,
        backend,
        globals,
        darmok_session::PreparedStatementLimits {
            max_statements: 1,
            max_sql_bytes: 8,
        },
        0,
        BytesMut::new(),
    );
    let owner = tokio::spawn(owner.run());
    let first = command(&mut input, COM_STMT_PREPARE, b"SELECT 1").await;
    assert_eq!(&first[1..5], &1u32.to_le_bytes());
    packet(&mut input, 2).await;
    packet(&mut input, 3).await;
    let capacity = command(&mut input, COM_STMT_PREPARE, b"SELECT 2").await;
    assert_eq!(capacity[0], 0xff);
    assert_eq!(u16::from_le_bytes([capacity[1], capacity[2]]), 1235);
    input
        .write_all(&encoded(COM_STMT_CLOSE, &1u32.to_le_bytes()))
        .await
        .unwrap();
    input
        .write_all(&encoded(COM_STMT_CLOSE, &9999u32.to_le_bytes()))
        .await
        .unwrap();
    // Both close variants preserve the preceding condition count.
    ok_with_warnings(&command(&mut input, COM_PING, b"").await, 2, 1);
    let second = command(&mut input, COM_STMT_PREPARE, b"SELECT 2").await;
    assert_eq!(&second[1..5], &2u32.to_le_bytes());
    packet(&mut input, 2).await;
    packet(&mut input, 3).await;
    ok(
        &command(&mut input, COM_STMT_RESET, &2u32.to_le_bytes()).await,
        2,
    );
    let result = finish(input, owner).await;
    assert_eq!(result.session.found_rows, 2);
    assert_eq!(result.session.last_insert_id, 29);
    assert_eq!(result.session.row_count, 0);
    assert_eq!(result.session.statement_condition_count_u16(), 0);
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_prepared_preparation_preserves_row_counts_without_executing() {
    let backend = connect_backend().await;
    let (mut session, globals) = fixture(true);
    session.found_rows = 2;
    session.last_insert_id = 29;
    let (mut input, owner) = connection(session, backend, globals, BytesMut::new()).await;
    let error = command(&mut input, COM_STMT_RESET, &9999u32.to_le_bytes()).await;
    assert_eq!(u16::from_le_bytes([error[1], error[2]]), 1243);
    let header = command(&mut input, COM_STMT_PREPARE, b"SELECT 1").await;
    assert_eq!(header, [0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    packet(&mut input, 2).await;
    let result = finish(input, owner).await;
    assert_eq!(result.session.row_count, -1);
    assert_eq!(result.session.found_rows, 2);
    assert_eq!(result.session.last_insert_id, 29);
    assert_eq!(result.session.statement_condition_count_u16(), 0);
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_prepared_rejects_tables_batches_controls_and_resolves_variable_variants() {
    let backend = connect_backend().await;
    let (session, globals) = fixture(true);
    let (mut input, owner) = connection(session, backend, globals, BytesMut::new()).await;
    for sql in [
        "SELECT n FROM missing",
        "SELECT ?+1",
        "SELECT ? WHERE TRUE",
        "COMMIT; SELECT 1",
        "SET autocommit=0",
        "SELECT /*! 1 */ 2",
    ] {
        let error = command(&mut input, COM_STMT_PREPARE, sql.as_bytes()).await;
        assert_eq!(error[0], 0xff);
        assert_eq!(u16::from_le_bytes([error[1], error[2]]), 1235);
    }
    let header = command(
        &mut input,
        COM_STMT_PREPARE,
        b"SELECT (+@@autocommit) AS value",
    )
    .await;
    assert_eq!(&header[1..5], &1u32.to_le_bytes());
    packet(&mut input, 2).await;
    execute(&mut input, b"").await;
    assert_eq!(packet(&mut input, 1).await, [1]);
    packet(&mut input, 2).await;
    assert_eq!(packet(&mut input, 3).await, [0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    packet(&mut input, 4).await;
    ok(
        &command(&mut input, COM_QUERY, b"SET autocommit=0").await,
        0,
    );
    execute(&mut input, b"").await;
    assert_eq!(packet(&mut input, 1).await, [1]);
    packet(&mut input, 2).await;
    assert_eq!(packet(&mut input, 3).await, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    packet(&mut input, 4).await;
    let result = finish(input, owner).await;
    assert_eq!(
        result.session.transaction_settings().unwrap().autocommit,
        AutocommitSetting::Disabled
    );
}
