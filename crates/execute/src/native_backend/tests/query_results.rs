// Ordinary COM_QUERY SELECT fixtures. PostgreSQL is required to prove the
// public path preserves its exclusive native owner's settled transaction.
use super::*;
use crate::{QueryOutcome, QuerySqlError, SelectSqlError, ServerSetValues, execute_query_command};
use bytes::Buf;
use darmok_protocol::{CapabilityFlags, Command, PacketHeader, read_lenenc_bytes};
use darmok_session::{
    AutocommitSetting, MysqlCompatibilityProfile, SessionState, SqlModes, WarningLevel,
};

fn fixture() -> (SessionState, ServerSetValues) {
    let mut state = SessionState::new(1);
    state.client_capabilities = CapabilityFlags::CLIENT_PROTOCOL_41.bits();
    let globals = ServerSetValues {
        sql_modes: SqlModes::MYSQL84_DEFAULT,
        transactions: MysqlCompatibilityProfile::default_mysql8()
            .default_transaction_characteristics,
        autocommit: AutocommitSetting::Enabled,
        completion_type: darmok_session::FrontendCompletionType::NoChain,
    };
    (state, globals)
}

async fn request(
    state: &mut SessionState,
    backend: &mut NativeBackend,
    globals: &ServerSetValues,
    sql: &str,
    sequence: u8,
) -> (QueryOutcome, Vec<u8>) {
    let mut input = vec![darmok_protocol::constants::COM_QUERY];
    input.extend_from_slice(sql.as_bytes());
    let command = Command::decode(&input, state.client_capabilities).unwrap();
    let mut output = Vec::new();
    let outcome = execute_query_command(state, backend, globals, &command, sequence, &mut output)
        .await
        .unwrap();
    assert!(state.unconfirmed_command().is_none());
    (outcome, output)
}

pub(super) fn packets(mut response: &[u8], mut sequence: u8) -> Vec<&[u8]> {
    let mut result = Vec::new();
    while !response.is_empty() {
        let header = PacketHeader::decode(response).unwrap();
        assert_eq!(header.sequence_id, sequence);
        let length = header.payload_len as usize;
        result.push(&response[4..4 + length]);
        response = &response[4 + length..];
        sequence = sequence.wrapping_add(1);
    }
    result
}

pub(super) fn column(mut payload: &[u8]) -> (Vec<u8>, u16, u32, u8, u16, u8) {
    assert_eq!(read_lenenc_bytes(&mut payload, "catalog").unwrap(), b"def");
    for name in ["schema", "table", "original table"] {
        assert!(read_lenenc_bytes(&mut payload, name).unwrap().is_empty());
    }
    let name = read_lenenc_bytes(&mut payload, "name").unwrap();
    assert!(
        read_lenenc_bytes(&mut payload, "original name")
            .unwrap()
            .is_empty()
    );
    assert_eq!(payload.get_u8(), 12);
    let result = (
        name,
        payload.get_u16_le(),
        payload.get_u32_le(),
        payload.get_u8(),
        payload.get_u16_le(),
        payload.get_u8(),
    );
    assert_eq!(payload, [0, 0]);
    result
}

fn numeric_reference_column(expected: &serde_json::Value) -> (Vec<u8>, u16, u32, u8, u16, u8) {
    use darmok_types::mysql_const::{column_flag as flag, field_type as ty};
    let kind = match expected["type"].as_str().unwrap() {
        "LONGLONG" => ty::LONGLONG,
        "NEWDECIMAL" => ty::NEWDECIMAL,
        other => panic!("unexpected numeric reference type: {other}"),
    };
    let flags = expected["cli_flags"]
        .as_array()
        .unwrap()
        .iter()
        .fold(0, |flags, name| {
            flags
                | match name.as_str().unwrap() {
                    "NOT_NULL" => flag::NOT_NULL,
                    "UNSIGNED" => flag::UNSIGNED,
                    "BINARY" => flag::BINARY,
                    // NUM is computed by the reference client. MySQL's server
                    // field serialization does not transmit that CLI flag.
                    "NUM" => 0,
                    other => panic!("unexpected numeric reference flag: {other}"),
                }
        });
    (
        expected["name"].as_str().unwrap().as_bytes().to_vec(),
        expected["charset"].as_u64().unwrap().try_into().unwrap(),
        expected["width"].as_u64().unwrap().try_into().unwrap(),
        kind,
        flags,
        expected["decimals"].as_u64().unwrap().try_into().unwrap(),
    )
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_results_exact_numbers_match_reference_rows_and_both_eof_forms() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/mysql_exact_numbers.json"
    ))
    .unwrap();
    let mut backend = connect_backend().await;
    for deprecated in [false, true] {
        let (mut state, globals) = fixture();
        if deprecated {
            state.client_capabilities |= CapabilityFlags::CLIENT_DEPRECATE_EOF.bits();
        }
        state.last_insert_id = 29;
        let before = state.transaction_settings().unwrap();
        for case in reference["cases"].as_array().unwrap() {
            state.push_warning(WarningLevel::Error, 1231, "prior statement fixture");
            let (outcome, output) = request(
                &mut state,
                &mut backend,
                &globals,
                case["sql"].as_str().unwrap(),
                253,
            )
            .await;
            assert!(matches!(outcome, QueryOutcome::Success(_)), "{case}");
            let response = packets(&output, 253);
            let expected = case["columns"].as_array().unwrap();
            let count = expected.len();
            assert_eq!(response.len(), count + if deprecated { 3 } else { 4 });
            assert_eq!(response[0], [u8::try_from(count).unwrap()]);
            for (index, expected) in expected.iter().enumerate() {
                assert_eq!(
                    column(response[index + 1]),
                    numeric_reference_column(expected)
                );
            }
            let row_index = if deprecated {
                count + 1
            } else {
                assert_eq!(response[count + 1], [0xfe, 0, 0, 2, 0]);
                count + 2
            };
            let mut row = response[row_index];
            let expected_row = case["row"].as_array().unwrap();
            assert_eq!(expected_row.len(), count);
            for expected in expected_row {
                assert_eq!(
                    read_lenenc_bytes(&mut row, "exact numeric value").unwrap(),
                    expected.as_str().unwrap().as_bytes()
                );
            }
            assert!(row.is_empty());
            assert_eq!(
                *response.last().unwrap(),
                if deprecated {
                    &[0xfe, 0, 0, 2, 0, 0, 0][..]
                } else {
                    &[0xfe, 0, 0, 2, 0][..]
                }
            );
            assert_eq!(state.row_count, -1);
            assert_eq!(state.found_rows, 1);
            assert_eq!(state.affected_rows, 0);
            assert_eq!(state.last_insert_id, 29);
            assert_eq!(
                u64::try_from(state.warning_count()).unwrap(),
                case["warning_count"].as_u64().unwrap()
            );
            assert_eq!(state.transaction_settings().unwrap(), before);
            assert_eq!(
                backend.state(),
                NativeBackendState::Ready(TransactionState::Idle)
            );
        }
    }
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_results_exact_numbers_preserve_next_and_active_settings_and_reject_approximation() {
    use darmok_session::{FrontendTransactionAccess, FrontendTransactionCommand};
    let mut backend = connect_backend().await;
    let (mut state, globals) = fixture();
    assert!(matches!(
        request(
            &mut state,
            &mut backend,
            &globals,
            "SET TRANSACTION READ ONLY",
            1
        )
        .await
        .0,
        QueryOutcome::Success(_)
    ));
    assert!(matches!(
        request(
            &mut state,
            &mut backend,
            &globals,
            "SET autocommit=0, sql_mode='NO_BACKSLASH_ESCAPES'",
            1
        )
        .await
        .0,
        QueryOutcome::Success(_)
    ));
    let before_next = state.transaction_settings().unwrap();
    let (outcome, output) = request(
        &mut state,
        &mut backend,
        &globals,
        "SELECT -0.00 AS zero, 18446744073709551616 AS value",
        1,
    )
    .await;
    assert!(matches!(outcome, QueryOutcome::Success(_)));
    assert_eq!(
        column(packets(&output, 1)[1]),
        (b"zero".to_vec(), 63, 5, 246, 0x81, 2)
    );
    assert_eq!(*packets(&output, 1).last().unwrap(), [0xfe, 0, 0, 0, 2]);
    assert_eq!(state.transaction_settings().unwrap(), before_next);
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
    let _ = backend
        .begin(NativeTransactionSpec {
            isolation: NativeIsolation::RepeatableRead,
            access: NativeTransactionAccess::ReadOnly,
        })
        .await
        .unwrap();
    // Trusted fixture start. This does not implement public START SQL or prove
    // table-read isolation equivalence between PostgreSQL and MySQL.
    let mut stage = state
        .stage_transaction_command(FrontendTransactionCommand::BeginExplicit {
            access: Some(FrontendTransactionAccess::ReadOnly),
        })
        .unwrap();
    stage.mark_submitted().unwrap();
    while let Some(boundary) = stage.next_frontend_boundary() {
        stage.record_confirmed_frontend_boundary(boundary).unwrap();
    }
    stage.finish_success_with_validated_output().unwrap();
    let before_active = state.transaction_settings().unwrap();
    state.last_insert_id = 29;
    let (outcome, output) = request(
        &mut state,
        &mut backend,
        &globals,
        "SELECT +(-1.00) AS n",
        1,
    )
    .await;
    assert!(matches!(outcome, QueryOutcome::Success(_)));
    assert_eq!(packets(&output, 1)[3], b"\x05-1.00");
    assert_eq!(*packets(&output, 1).last().unwrap(), [0xfe, 0, 0, 1, 34]);
    assert_eq!(state.transaction_settings().unwrap(), before_active);
    let (outcome, output) = request(
        &mut state,
        &mut backend,
        &globals,
        "SELECT 1.00, 1e2, 2.00",
        1,
    )
    .await;
    assert!(matches!(
        outcome,
        QueryOutcome::SqlError {
            error: QuerySqlError::Select(SelectSqlError::Unsupported),
            ..
        }
    ));
    let response = packets(&output, 1);
    assert_eq!(response.len(), 1);
    assert_eq!(
        &response[0][..9],
        &[0xff, 0xd3, 4, b'#', b'4', b'2', b'0', b'0', b'0']
    );
    assert_eq!(state.transaction_settings().unwrap(), before_active);
    assert_eq!(state.last_insert_id, 29);
    assert_eq!(state.error_count(), 1);
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Transaction)
    );
    assert_eq!(
        client(&backend)
            .query_one("SHOW transaction_read_only", &[])
            .await
            .unwrap()
            .get::<_, String>(0),
        "on"
    );
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_results_emit_declared_columns_text_nulls_and_both_eof_forms() {
    let mut backend = connect_backend().await;
    for deprecated in [false, true] {
        let (mut state, globals) = fixture();
        if deprecated {
            state.client_capabilities |= CapabilityFlags::CLIENT_DEPRECATE_EOF.bits();
        }
        state.last_insert_id = 29;
        state.push_warning(WarningLevel::Error, 1231, "prior statement fixture");
        let (outcome, output) = request(
            &mut state,
            &mut backend,
            &globals,
            "SELECT 001 AS n, 'hé' AS text, NULL AS absent, @@SESSION.autocommit AS auto",
            253,
        )
        .await;
        assert!(matches!(outcome, QueryOutcome::Success(_)));
        let response = packets(&output, 253);
        assert_eq!(response.len(), if deprecated { 7 } else { 8 });
        assert_eq!(response[0], [4]);
        assert_eq!(column(response[1]), (b"n".to_vec(), 63, 4, 8, 0x0081, 0));
        assert_eq!(column(response[2]), (b"text".to_vec(), 45, 8, 253, 1, 31));
        assert_eq!(
            column(response[3]),
            (b"absent".to_vec(), 63, 0, 6, 0x0080, 0)
        );
        assert_eq!(column(response[4]), (b"auto".to_vec(), 63, 1, 8, 0x0080, 0));
        let row = if deprecated {
            response[5]
        } else {
            assert_eq!(response[5], [0xfe, 0, 0, 2, 0]);
            response[6]
        };
        assert_eq!(row, [1, b'1', 3, b'h', 0xc3, 0xa9, 0xfb, 1, b'1']);
        assert_eq!(
            *response.last().unwrap(),
            if deprecated {
                &[0xfe, 0, 0, 2, 0, 0, 0][..]
            } else {
                &[0xfe, 0, 0, 2, 0][..]
            }
        );
        assert_eq!(state.row_count, -1);
        assert_eq!(state.found_rows, 1);
        assert_eq!(state.affected_rows, 0);
        assert_eq!(state.last_insert_id, 29);
        assert_eq!(state.warning_count(), 0);
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Idle)
        );
    }
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_results_read_current_mode_and_defaults_without_starting_a_transaction() {
    let mut backend = connect_backend().await;
    let (mut state, globals) = fixture();
    assert!(matches!(
        request(
            &mut state,
            &mut backend,
            &globals,
            "SET TRANSACTION READ ONLY",
            1
        )
        .await
        .0,
        QueryOutcome::Success(_)
    ));
    let original_next = state.transaction_settings().unwrap().next;
    assert!(matches!(
        request(
            &mut state,
            &mut backend,
            &globals,
            "SET autocommit=0, sql_mode='NO_BACKSLASH_ESCAPES'",
            1
        )
        .await
        .0,
        QueryOutcome::Success(_)
    ));
    let (outcome, output) = request(
        &mut state,
        &mut backend,
        &globals,
        "SELECT 'a\\nb' AS text, @@session.autocommit AS auto, @@global.autocommit AS global_auto",
        1,
    )
    .await;
    assert!(matches!(outcome, QueryOutcome::Success(_)));
    let response = packets(&output, 1);
    assert_eq!(response[5], [4, b'a', b'\\', b'n', b'b', 1, b'0', 1, b'1']);
    assert_eq!(*response.last().unwrap(), [0xfe, 0, 0, 0, 2]);
    assert_eq!(state.transaction_settings().unwrap().next, original_next);
    assert!(state.transaction_settings().unwrap().active.is_none());
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
    let (outcome, output) = request(
        &mut state,
        &mut backend,
        &globals,
        "SELECT @@session.version",
        1,
    )
    .await;
    assert!(matches!(
        outcome,
        QueryOutcome::SqlError {
            error: QuerySqlError::Select(SelectSqlError::GlobalVariable),
            ..
        }
    ));
    assert_eq!(
        &packets(&output, 1)[0][..9],
        &[0xff, 0xd6, 4, b'#', b'H', b'Y', b'0', b'0', b'0']
    );
    assert_eq!(state.error_count(), 1);
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_results_preserve_an_owned_active_read_only_transaction() {
    use darmok_session::{FrontendTransactionAccess, FrontendTransactionCommand};
    let mut backend = connect_backend().await;
    let (mut state, globals) = fixture();
    let _ = backend
        .begin(NativeTransactionSpec {
            isolation: NativeIsolation::RepeatableRead,
            access: NativeTransactionAccess::ReadOnly,
        })
        .await
        .unwrap();
    // Trusted fixture start. This is not public START SQL execution or proof of
    // native/frontend isolation equivalence for table reads.
    let mut stage = state
        .stage_transaction_command(FrontendTransactionCommand::BeginExplicit {
            access: Some(FrontendTransactionAccess::ReadOnly),
        })
        .unwrap();
    stage.mark_submitted().unwrap();
    while let Some(boundary) = stage.next_frontend_boundary() {
        stage.record_confirmed_frontend_boundary(boundary).unwrap();
    }
    stage.finish_success_with_validated_output().unwrap();
    let before = state.transaction_settings().unwrap();
    let (outcome, output) = request(
        &mut state,
        &mut backend,
        &globals,
        "SELECT @@transaction_read_only AS default_access, 1 AS n",
        1,
    )
    .await;
    assert!(matches!(outcome, QueryOutcome::Success(_)));
    let response = packets(&output, 1);
    assert_eq!(response[4], [1, b'0', 1, b'1']);
    assert_eq!(*response.last().unwrap(), [0xfe, 0, 0, 3, 32]);
    assert_eq!(state.transaction_settings().unwrap(), before);
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Transaction)
    );
    assert_eq!(
        client(&backend)
            .query_one("SHOW transaction_read_only", &[])
            .await
            .unwrap()
            .get::<_, String>(0),
        "on"
    );
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_results_reject_executable_comments_before_settings_or_transaction_effects() {
    use darmok_session::{FrontendTransactionAccess, FrontendTransactionCommand};
    let mut backend = connect_backend().await;
    let (mut state, globals) = fixture();
    assert!(matches!(
        request(&mut state, &mut backend, &globals, "SET autocommit=0", 1)
            .await
            .0,
        QueryOutcome::Success(_)
    ));
    let _ = backend
        .begin(NativeTransactionSpec {
            isolation: NativeIsolation::RepeatableRead,
            access: NativeTransactionAccess::ReadOnly,
        })
        .await
        .unwrap();
    // Trusted fixture start; rejection must not publish an autocommit setting
    // or finish this owned native transaction.
    let mut stage = state
        .stage_transaction_command(FrontendTransactionCommand::BeginExplicit {
            access: Some(FrontendTransactionAccess::ReadOnly),
        })
        .unwrap();
    stage.mark_submitted().unwrap();
    while let Some(boundary) = stage.next_frontend_boundary() {
        stage.record_confirmed_frontend_boundary(boundary).unwrap();
    }
    stage.finish_success_with_validated_output().unwrap();
    state.last_insert_id = 29;
    let before = state.transaction_settings().unwrap();
    for sql in [
        "SELECT /*! TRUE */",
        "SELECT /*!80411 001 */",
        "SET /*! autocommit=1 */",
    ] {
        let (outcome, output) = request(&mut state, &mut backend, &globals, sql, 1).await;
        assert!(matches!(
            outcome,
            QueryOutcome::SqlError {
                error: QuerySqlError::UnsupportedExecutableComment,
                ..
            }
        ));
        let response = packets(&output, 1);
        assert_eq!(response.len(), 1);
        assert_eq!(
            &response[0][..9],
            &[0xff, 0xd3, 4, b'#', b'4', b'2', b'0', b'0', b'0']
        );
        assert_eq!(state.transaction_settings().unwrap(), before);
        assert_eq!(state.last_insert_id, 29);
        assert_eq!(state.error_count(), 1);
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Transaction)
        );
    }
    // A quoted value containing the same characters is an ordinary string.
    let (outcome, output) = request(
        &mut state,
        &mut backend,
        &globals,
        "SELECT '/*! TRUE */' AS normal",
        1,
    )
    .await;
    assert!(matches!(outcome, QueryOutcome::Success(_)));
    assert_eq!(packets(&output, 1)[3], b"\x0b/*! TRUE */");
    assert_eq!(state.transaction_settings().unwrap(), before);
    assert_eq!(state.error_count(), 0);
    assert_eq!(
        client(&backend)
            .query_one("SHOW transaction_read_only", &[])
            .await
            .unwrap()
            .get::<_, String>(0),
        "on"
    );
    let _ = backend.dispose().await.unwrap();
}
