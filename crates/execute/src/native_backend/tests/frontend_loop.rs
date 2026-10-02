//! Ordinary command-phase TCP exchanges; no handshake/auth implementation or
//! transport failure injection. PostgreSQL and the native owner are required.

use super::*;
use crate::{
    FrontendConnection, FrontendEnd, FrontendReport, QueryOutcome, ServerSetValues,
    execute_query_command,
};
use bytes::{Bytes, BytesMut};
use darmok_protocol::constants::{COM_PING, COM_QUERY, COM_QUIT};
use darmok_protocol::{CapabilityFlags, Command, PacketHeader, RawPacket, StatusFlags};
use darmok_session::{AutocommitSetting, MysqlCompatibilityProfile, SessionState, SqlModes};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

fn fixture(deprecate_eof: bool) -> (SessionState, ServerSetValues) {
    let mut session = SessionState::new(1);
    session.client_capabilities = (CapabilityFlags::CLIENT_PROTOCOL_41
        | if deprecate_eof {
            CapabilityFlags::CLIENT_DEPRECATE_EOF
        } else {
            CapabilityFlags::empty()
        })
    .bits();
    let profile = MysqlCompatibilityProfile::default_mysql8();
    let globals = ServerSetValues {
        sql_modes: SqlModes::MYSQL84_DEFAULT,
        transactions: profile.default_transaction_characteristics,
        autocommit: AutocommitSetting::Enabled,
        completion_type: profile.default_completion_type,
    };
    (session, globals)
}

fn encoded(command: u8, body: &[u8]) -> BytesMut {
    let mut payload = vec![command];
    payload.extend_from_slice(body);
    let mut bytes = BytesMut::new();
    RawPacket::new(0, Bytes::from(payload))
        .unwrap()
        .encode_into(&mut bytes)
        .unwrap();
    bytes
}

async fn connection(
    session: SessionState,
    backend: NativeBackend,
    globals: ServerSetValues,
    input: BytesMut,
) -> (TcpStream, JoinHandle<FrontendReport>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (transport, _) = listener.accept().await.unwrap();
    let owner = FrontendConnection::new(transport, session, backend, globals, 0, input);
    (client, tokio::spawn(owner.run()))
}

async fn packet(input: &mut TcpStream, sequence: u8) -> Vec<u8> {
    let mut header = [0; 4];
    input.read_exact(&mut header).await.unwrap();
    let header = PacketHeader::decode(&header).unwrap();
    assert_eq!(header.sequence_id, sequence);
    let mut payload = vec![0; header.payload_len as usize];
    input.read_exact(&mut payload).await.unwrap();
    payload
}

async fn command(input: &mut TcpStream, command: u8, body: &[u8]) -> Vec<u8> {
    input.write_all(&encoded(command, body)).await.unwrap();
    packet(input, 1).await
}

fn ok(payload: &[u8], flags: u16) {
    let [lo, hi] = flags.to_le_bytes();
    assert_eq!(payload, [0, 0, 0, lo, hi, 0, 0]);
}

async fn closed(input: &mut TcpStream) {
    let mut rest = Vec::new();
    input.read_to_end(&mut rest).await.unwrap();
    assert!(rest.is_empty());
}

fn report(report: &FrontendReport, end: FrontendEnd, rolled_back: bool) {
    assert_eq!(report.end.as_ref().unwrap(), &end);
    assert!(report.shutdown.is_ok(), "{report:?}");
    assert_eq!(
        report.disposal.as_ref().unwrap().previous_state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
    assert_eq!(report.rollback.is_some(), rolled_back);
    if let Some(receipt) = &report.rollback {
        assert_eq!(receipt.as_ref().unwrap().control(), NativeControl::Rollback);
    }
    assert!(report.session.unconfirmed_command().is_none());
    assert!(report.session.unconfirmed_transaction_command().is_none());
}

async fn prepare(
    session: &mut SessionState,
    backend: &mut NativeBackend,
    globals: &ServerSetValues,
    sql: &str,
) {
    let mut writer = Vec::new();
    let command = Command::decode(
        &[&[COM_QUERY][..], sql.as_bytes()].concat(),
        session.client_capabilities,
    )
    .unwrap();
    assert!(matches!(
        execute_query_command(session, backend, globals, &command, 1, &mut writer)
            .await
            .unwrap(),
        QueryOutcome::Success(_)
    ));
}

async fn table(backend: &NativeBackend) -> String {
    let pid: i32 = client(backend)
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let name = format!("public.darmok_frontend_fixture_{pid}");
    setup(
        backend,
        &format!("CREATE TABLE {name}(n integer PRIMARY KEY); INSERT INTO {name} VALUES(0)"),
    )
    .await;
    name
}

async fn effects_and_cleanup(name: &str) -> Vec<i32> {
    let observer = connect_backend().await;
    let values = client(&observer)
        .query(&format!("SELECT n FROM {name} ORDER BY n"), &[])
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    setup(&observer, &format!("DROP TABLE {name}")).await;
    let absent: bool = client(&observer)
        .query_one("SELECT to_regclass($1::text) IS NULL", &[&name])
        .await
        .unwrap()
        .get(0);
    let _ = observer.dispose().await.unwrap();
    assert!(absent, "fixture cleanup was not confirmed");
    values
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_serial_tcp_commands_preserve_modes_results_and_status() {
    for deprecate_eof in [false, true] {
        let backend = connect_backend().await;
        let (session, globals) = fixture(deprecate_eof);
        let (mut input, owner) = connection(session, backend, globals, BytesMut::new()).await;
        ok(&command(&mut input, COM_PING, b"").await, 2);
        ok(
            &command(
                &mut input,
                COM_QUERY,
                b"SET sql_mode='ANSI_QUOTES,NO_BACKSLASH_ESCAPES'",
            )
            .await,
            514,
        );
        input
            .write_all(&encoded(COM_QUERY, br#"SELECT 'a\b' AS "label""#))
            .await
            .unwrap();
        assert_eq!(packet(&mut input, 1).await, [1]);
        let column = packet(&mut input, 2).await;
        assert_eq!(super::query_results::column(&column).0, b"label");
        let mut sequence = 3;
        if !deprecate_eof {
            assert_eq!(packet(&mut input, sequence).await, [0xfe, 0, 0, 2, 2]);
            sequence += 1;
        }
        assert_eq!(packet(&mut input, sequence).await, [3, b'a', b'\\', b'b']);
        sequence += 1;
        let end = packet(&mut input, sequence).await;
        if deprecate_eof {
            assert_eq!(end, [0xfe, 0, 0, 2, 2, 0, 0]);
        } else {
            assert_eq!(end, [0xfe, 0, 0, 2, 2]);
        }
        ok(
            &command(&mut input, COM_QUERY, b"START TRANSACTION READ ONLY").await,
            8707,
        );
        ok(&command(&mut input, COM_PING, b"").await, 8707);
        let error = command(&mut input, COM_QUERY, b"SELECT 1 FROM unsupported_table").await;
        assert_eq!(
            &error[..9],
            [0xff, 0xd3, 4, b'#', b'4', b'2', b'0', b'0', b'0']
        );
        ok(&command(&mut input, COM_QUERY, b"COMMIT").await, 514);
        input.write_all(&encoded(COM_QUIT, b"")).await.unwrap();
        closed(&mut input).await;
        let result = owner.await.unwrap();
        report(&result, FrontendEnd::Quit, false);
        assert_eq!(result.session.row_count, 0);
    }
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_release_policy_completes_before_close_and_discards_buffered_input() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/mysql_frontend_release.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let mut backend = connect_backend().await;
        let name = table(&backend).await;
        let (mut session, globals) = fixture(false);
        for sql in case["setup"].as_array().unwrap() {
            prepare(&mut session, &mut backend, &globals, sql.as_str().unwrap()).await;
        }
        if case["active"].as_bool().unwrap() {
            prepare(&mut session, &mut backend, &globals, "BEGIN").await;
            setup(&backend, &format!("INSERT INTO {name} VALUES(1)")).await;
        }
        let closes = case["closes"].as_bool().unwrap();
        let mut buffered = encoded(COM_QUERY, case["completion"].as_str().unwrap().as_bytes());
        buffered.extend_from_slice(&encoded(COM_QUERY, b"SET sql_mode='ANSI_QUOTES'"));
        buffered.extend_from_slice(&encoded(COM_QUIT, b""));
        let (mut input, owner) = connection(session, backend, globals, buffered).await;
        let chain = case["chain"].as_bool().unwrap();
        ok(&packet(&mut input, 1).await, if chain { 3 } else { 2 });
        if !closes {
            ok(&packet(&mut input, 1).await, if chain { 3 } else { 2 });
        }
        closed(&mut input).await;
        let result = owner.await.unwrap();
        let values = effects_and_cleanup(&name).await;
        report(
            &result,
            if closes {
                FrontendEnd::Release
            } else {
                FrontendEnd::Quit
            },
            chain,
        );
        assert_eq!(
            result
                .session
                .transaction_settings()
                .unwrap()
                .active
                .is_some(),
            chain,
            "{case}"
        );
        assert_eq!(
            result
                .session
                .sql_modes()
                .unwrap()
                .contains(darmok_session::SqlMode::AnsiQuotes),
            !closes,
            "{case}"
        );
        assert_eq!(
            values,
            case["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as i32)
                .collect::<Vec<_>>(),
            "{case}"
        );
    }
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_normal_quit_and_eof_confirm_rollback_of_prior_work() {
    for quit in [true, false] {
        let mut backend = connect_backend().await;
        let name = table(&backend).await;
        let (mut session, globals) = fixture(false);
        prepare(&mut session, &mut backend, &globals, "BEGIN").await;
        setup(&backend, &format!("INSERT INTO {name} VALUES(1)")).await;
        let (mut input, owner) = connection(session, backend, globals, BytesMut::new()).await;
        if quit {
            input.write_all(&encoded(COM_QUIT, b"")).await.unwrap();
        } else {
            input.shutdown().await.unwrap();
        }
        closed(&mut input).await;
        let result = owner.await.unwrap();
        let values = effects_and_cleanup(&name).await;
        report(
            &result,
            if quit {
                FrontendEnd::Quit
            } else {
                FrontendEnd::Eof
            },
            true,
        );
        assert_eq!(values, [0]);
        assert!(
            result
                .session
                .transaction_settings()
                .unwrap()
                .active
                .is_some()
        );
    }
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_unimplemented_responding_command_errors_without_closing_or_effects() {
    let backend = connect_backend().await;
    let (session, globals) = fixture(false);
    let (mut input, owner) = connection(session, backend, globals, BytesMut::new()).await;
    let error = command(&mut input, darmok_protocol::constants::COM_STATISTICS, b"").await;
    assert_eq!(
        &error[..9],
        [0xff, 0xd3, 4, b'#', b'4', b'2', b'0', b'0', b'0']
    );
    ok(
        &command(&mut input, COM_PING, b"").await,
        StatusFlags::SERVER_STATUS_AUTOCOMMIT.bits(),
    );
    input.write_all(&encoded(COM_QUIT, b"")).await.unwrap();
    closed(&mut input).await;
    let result = owner.await.unwrap();
    report(&result, FrontendEnd::Quit, false);
    assert_eq!(result.session.warning_count(), 1);
    assert_eq!(result.session.row_count, 0);
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_unimplemented_no_response_commands_terminate_without_an_invented_packet() {
    for (command, body, name) in [
        (
            darmok_protocol::constants::COM_STMT_CLOSE,
            vec![1, 0, 0, 0],
            "COM_STMT_CLOSE",
        ),
        (
            darmok_protocol::constants::COM_STMT_SEND_LONG_DATA,
            vec![1, 0, 0, 0, 0, 0],
            "COM_STMT_SEND_LONG_DATA",
        ),
    ] {
        let backend = connect_backend().await;
        let (session, globals) = fixture(false);
        let mut buffered = encoded(command, &body);
        buffered.extend_from_slice(&encoded(COM_PING, b""));
        let (mut input, owner) = connection(session, backend, globals, buffered).await;
        closed(&mut input).await;
        let result = owner.await.unwrap();
        assert!(
            matches!(result.end, Err(crate::FrontendError::NoResponseCommand(actual)) if actual == name)
        );
        assert!(result.shutdown.is_ok());
        assert!(result.rollback.is_none());
        assert_eq!(
            result.disposal.unwrap().previous_state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
        assert!(result.session.unconfirmed_command().is_none());
    }
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_normal_coalesced_and_split_frames_reset_each_command_sequence() {
    let backend = connect_backend().await;
    let (session, globals) = fixture(false);
    let (mut input, owner) = connection(session, backend, globals, BytesMut::new()).await;
    let mut coalesced = encoded(COM_PING, b"");
    coalesced.extend_from_slice(&encoded(COM_PING, b""));
    input.write_all(&coalesced).await.unwrap();
    ok(&packet(&mut input, 1).await, 2);
    ok(&packet(&mut input, 1).await, 2);
    let split = encoded(COM_QUERY, b"SET autocommit=0");
    input.write_all(&split[..2]).await.unwrap();
    input.write_all(&split[2..]).await.unwrap();
    ok(&packet(&mut input, 1).await, 0);
    input.shutdown().await.unwrap();
    closed(&mut input).await;
    let result = owner.await.unwrap();
    report(&result, FrontendEnd::Eof, false);
    assert_eq!(
        result.session.transaction_settings().unwrap().autocommit,
        AutocommitSetting::Disabled
    );
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn frontend_loop_release_received_over_tcp_sends_ok_then_eof() {
    for verb in ["COMMIT", "ROLLBACK"] {
        let mut backend = connect_backend().await;
        let name = table(&backend).await;
        let (mut session, globals) = fixture(false);
        prepare(&mut session, &mut backend, &globals, "BEGIN").await;
        setup(&backend, &format!("INSERT INTO {name} VALUES(1)")).await;
        let (mut input, owner) = connection(session, backend, globals, BytesMut::new()).await;
        ok(
            &command(&mut input, COM_QUERY, format!("{verb} RELEASE").as_bytes()).await,
            2,
        );
        closed(&mut input).await;
        let result = owner.await.unwrap();
        let values = effects_and_cleanup(&name).await;
        report(&result, FrontendEnd::Release, false);
        assert_eq!(
            values,
            if verb == "COMMIT" {
                vec![0, 1]
            } else {
                vec![0]
            }
        );
    }
}
