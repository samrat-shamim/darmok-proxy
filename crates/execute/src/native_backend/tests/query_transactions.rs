// Ordinary original-source transaction controls, required on both PostgreSQL
// versions. Private fixture DML does not certify public table SQL execution.
use super::query_results::{column, packets};
use super::*;
use crate::{
    QueryOutcome, QuerySqlError, ServerSetValues, SetSqlError, TransactionSqlError,
    execute_query_command,
};
use darmok_protocol::{CapabilityFlags, Command, read_lenenc_bytes};
use darmok_session::{
    AutocommitSetting, FrontendCompletionType, FrontendIsolation, FrontendTransactionAccess,
    MysqlCompatibilityProfile, SessionState, SqlModes, WarningLevel,
};

fn fixture() -> (SessionState, ServerSetValues) {
    let mut state = SessionState::new(1);
    state.client_capabilities = CapabilityFlags::CLIENT_PROTOCOL_41.bits();
    state.last_insert_id = 73;
    state.found_rows = 18;
    let profile = MysqlCompatibilityProfile::default_mysql8();
    (
        state,
        ServerSetValues {
            sql_modes: SqlModes::MYSQL84_DEFAULT,
            transactions: profile.default_transaction_characteristics,
            autocommit: AutocommitSetting::Enabled,
            completion_type: profile.default_completion_type,
        },
    )
}

async fn request(
    state: &mut SessionState,
    backend: &mut NativeBackend,
    globals: &ServerSetValues,
    sql: &str,
) -> (QueryOutcome, Vec<u8>) {
    let mut input = vec![darmok_protocol::constants::COM_QUERY];
    input.extend_from_slice(sql.as_bytes());
    let command = Command::decode(&input, state.client_capabilities).unwrap();
    let mut output = Vec::new();
    let outcome = execute_query_command(state, backend, globals, &command, 253, &mut output)
        .await
        .unwrap();
    assert!(state.unconfirmed_command().is_none());
    assert!(state.unconfirmed_transaction_command().is_none());
    (outcome, output)
}

async fn ok(
    state: &mut SessionState,
    backend: &mut NativeBackend,
    globals: &ServerSetValues,
    sql: &str,
    flags: u16,
) {
    state.push_warning(WarningLevel::Error, 1231, "prior statement fixture");
    let (outcome, bytes) = request(state, backend, globals, sql).await;
    assert!(
        matches!(outcome, QueryOutcome::Success(_)),
        "{sql}: {outcome:?}"
    );
    let response = packets(&bytes, 253);
    let [lo, hi] = flags.to_le_bytes();
    assert_eq!(response, vec![&[0, 0, 0, lo, hi, 0, 0][..]], "{sql}");
    assert_eq!(
        (state.affected_rows, state.row_count, state.warning_count()),
        (0, 0, 0)
    );
    assert_eq!((state.last_insert_id, state.found_rows), (73, 18));
}

async fn sql_error(
    state: &mut SessionState,
    backend: &mut NativeBackend,
    globals: &ServerSetValues,
    sql: &str,
    expected: QuerySqlError,
) {
    let before = state.transaction_settings().unwrap();
    let native_before = backend.state();
    let (outcome, bytes) = request(state, backend, globals, sql).await;
    assert!(
        matches!(outcome, QueryOutcome::SqlError {error,..} if error==expected),
        "{sql}: {outcome:?}"
    );
    let response = packets(&bytes, 253);
    assert_eq!(response.len(), 1);
    let payload = response[0];
    assert_eq!(payload[0], 0xff);
    assert_eq!(
        u16::from_le_bytes([payload[1], payload[2]]),
        expected.code()
    );
    assert_eq!(payload[3], b'#');
    assert_eq!(&payload[4..9], expected.sql_state());
    assert_eq!(state.transaction_settings().unwrap(), before);
    assert_eq!(backend.state(), native_before);
    assert_eq!(
        (state.row_count, state.error_count(), state.warning_count()),
        (-1, 1, 1)
    );
}

fn reference(name: &str) -> serde_json::Value {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/mysql_query_transactions.json"
    ))
    .unwrap();
    corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap()["expected_effect"]
        .clone()
}

async fn active_pair(state: &SessionState, backend: &NativeBackend, expected: &serde_json::Value) {
    let pair = state
        .transaction_settings()
        .unwrap()
        .active
        .expect("frontend transaction active");
    let isolation = expected["isolation"].as_str().unwrap();
    let access = expected["access"].as_str().unwrap();
    assert_eq!(pair.isolation.variable_label().replace('-', " "), isolation);
    assert_eq!(
        pair.access == FrontendTransactionAccess::ReadOnly,
        access == "READ ONLY"
    );
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Transaction)
    );
    let row = client(backend).query_one("SELECT current_setting('transaction_isolation'), current_setting('transaction_read_only')", &[]).await.unwrap();
    assert_eq!(row.get::<_, String>(0), isolation.to_ascii_lowercase());
    assert_eq!(
        row.get::<_, String>(1),
        if access == "READ ONLY" { "on" } else { "off" }
    );
    let next = state.transaction_settings().unwrap().next;
    assert_eq!((next.isolation, next.access), (None, None));
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_transactions_explicit_native_characteristics_and_ok_statuses() {
    let mut backend = connect_backend().await;
    for isolation in ["READ COMMITTED", "REPEATABLE READ", "SERIALIZABLE"] {
        for access in ["READ WRITE", "READ ONLY"] {
            for start in ["BEGIN", "BEGIN WORK", "START TRANSACTION"] {
                let (mut state, globals) = fixture();
                ok(
                    &mut state,
                    &mut backend,
                    &globals,
                    &format!("SET SESSION TRANSACTION ISOLATION LEVEL {isolation}, {access}"),
                    2,
                )
                .await;
                let flags = if access == "READ ONLY" { 8195 } else { 3 };
                ok(&mut state, &mut backend, &globals, start, flags).await;
                active_pair(
                    &state,
                    &backend,
                    &serde_json::json!({"isolation":isolation,"access":access}),
                )
                .await;
                ok(
                    &mut state,
                    &mut backend,
                    &globals,
                    "COMMIT WORK AND NO CHAIN NO RELEASE",
                    2,
                )
                .await;
                assert_eq!(
                    backend.state(),
                    NativeBackendState::Ready(TransactionState::Idle)
                );
                assert!(state.transaction_settings().unwrap().active.is_none());
            }
        }
    }
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_transactions_commit_rollback_and_replacement_have_real_data_effects() {
    let mut backend = connect_backend().await;
    setup(
        &backend,
        "CREATE TEMP TABLE owned_values(n integer); INSERT INTO owned_values VALUES(0)",
    )
    .await;
    let (mut state, globals) = fixture();
    ok(&mut state, &mut backend, &globals, "BEGIN WORK", 3).await;
    setup(&backend, "INSERT INTO owned_values VALUES(1)").await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "COMMIT WORK AND NO CHAIN NO RELEASE",
        2,
    )
    .await;
    assert_eq!(values(&backend).await, [0, 1]);
    assert_eq!(reference("commit-persists-prior-work")["ids"], "0,1");
    setup(&backend, "DELETE FROM owned_values WHERE n=1").await;
    ok(&mut state, &mut backend, &globals, "START TRANSACTION", 3).await;
    setup(&backend, "INSERT INTO owned_values VALUES(2)").await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "ROLLBACK WORK AND NO CHAIN NO RELEASE",
        2,
    )
    .await;
    assert_eq!(values(&backend).await, [0]);
    assert_eq!(reference("rollback-discards-prior-work")["ids"], "0");
    ok(&mut state, &mut backend, &globals, "BEGIN", 3).await;
    setup(&backend, "INSERT INTO owned_values VALUES(3)").await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE, READ WRITE",
        3,
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "START TRANSACTION READ ONLY",
        8195,
    )
    .await;
    active_pair(
        &state,
        &backend,
        &reference("replacement-start-commits-prior-work")["replacement"],
    )
    .await;
    ok(&mut state, &mut backend, &globals, "ROLLBACK", 2).await;
    assert_eq!(values(&backend).await, [0, 3]);
    assert_eq!(
        reference("replacement-start-commits-prior-work")["ids"],
        "0,3"
    );
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_transactions_start_overrides_next_access_once_and_accepts_duplicates() {
    let mut backend = connect_backend().await;
    let (mut state, globals) = fixture();
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED, READ ONLY",
        2,
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "START TRANSACTION READ WRITE",
        3,
    )
    .await;
    active_pair(
        &state,
        &backend,
        &reference("explicit-access-overrides-next-once")["first"],
    )
    .await;
    ok(&mut state, &mut backend, &globals, "COMMIT", 2).await;
    ok(&mut state, &mut backend, &globals, "BEGIN", 3).await;
    active_pair(
        &state,
        &backend,
        &reference("explicit-access-overrides-next-once")["second"],
    )
    .await;
    ok(&mut state, &mut backend, &globals, "ROLLBACK", 2).await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "START TRANSACTION READ ONLY, READ ONLY",
        8195,
    )
    .await;
    active_pair(
        &state,
        &backend,
        &reference("repeated-identical-start-access")["active"],
    )
    .await;
    ok(&mut state, &mut backend, &globals, "ROLLBACK", 2).await;
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_transactions_active_chain_preserves_pair_after_defaults_change() {
    let mut backend = connect_backend().await;
    let (mut state, globals) = fixture();
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED, READ WRITE",
        2,
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "START TRANSACTION READ ONLY",
        8195,
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE, READ WRITE",
        8195,
    )
    .await;
    for (sql, label) in [
        ("COMMIT AND CHAIN", "commit_chain"),
        ("ROLLBACK AND CHAIN", "rollback_chain"),
    ] {
        ok(&mut state, &mut backend, &globals, sql, 8195).await;
        active_pair(
            &state,
            &backend,
            &reference("active-chain-retains-active-pair")[label],
        )
        .await;
    }
    ok(
        &mut state,
        &mut backend,
        &globals,
        "ROLLBACK AND NO CHAIN",
        2,
    )
    .await;
    ok(&mut state, &mut backend, &globals, "BEGIN", 3).await;
    active_pair(
        &state,
        &backend,
        &reference("active-chain-retains-active-pair")["next"],
    )
    .await;
    ok(&mut state, &mut backend, &globals, "COMMIT", 2).await;
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_transactions_idle_completion_resets_or_chains_pending_choices() {
    let mut backend = connect_backend().await;
    for verb in ["COMMIT", "ROLLBACK"] {
        for chain in [false, true] {
            let (mut state, globals) = fixture();
            ok(
                &mut state,
                &mut backend,
                &globals,
                "SET TRANSACTION ISOLATION LEVEL READ COMMITTED, READ ONLY",
                2,
            )
            .await;
            let sql = format!("{verb}{}", if chain { " AND CHAIN" } else { "" });
            ok(
                &mut state,
                &mut backend,
                &globals,
                &sql,
                if chain { 8195 } else { 2 },
            )
            .await;
            let after = state.transaction_settings().unwrap();
            assert_eq!((after.next.isolation, after.next.access), (None, None));
            if !chain {
                assert!(after.active.is_none());
                assert_eq!(
                    backend.state(),
                    NativeBackendState::Ready(TransactionState::Idle)
                );
                ok(&mut state, &mut backend, &globals, "START TRANSACTION", 3).await;
            }
            let name = format!(
                "idle-{}{}",
                verb.to_ascii_lowercase(),
                if chain { "-chain" } else { "-clears-next" }
            );
            active_pair(&state, &backend, &reference(&name)["active"]).await;
            ok(
                &mut state,
                &mut backend,
                &globals,
                "ROLLBACK AND NO CHAIN",
                2,
            )
            .await;
        }
    }
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_transactions_completion_defaults_and_explicit_negative_clauses() {
    let mut backend = connect_backend().await;
    let (mut state, mut globals) = fixture();
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET completion_type=CHAIN",
        2,
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "START TRANSACTION READ ONLY",
        8195,
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE",
        8195,
    )
    .await;
    ok(&mut state, &mut backend, &globals, "COMMIT", 8195).await;
    active_pair(
        &state,
        &backend,
        &reference("default-chain-retains-active-pair")["active"],
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "ROLLBACK AND NO CHAIN",
        2,
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED, READ ONLY",
        2,
    )
    .await;
    ok(&mut state, &mut backend, &globals, "ROLLBACK", 8195).await;
    active_pair(
        &state,
        &backend,
        &reference("default-chain-idle-uses-next")["active"],
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "COMMIT WORK AND NO CHAIN",
        2,
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET completion_type=RELEASE",
        2,
    )
    .await;
    ok(&mut state, &mut backend, &globals, "BEGIN", 3).await;
    ok(&mut state, &mut backend, &globals, "COMMIT NO RELEASE", 2).await;
    assert_eq!(
        state.transaction_settings().unwrap().completion_type,
        FrontendCompletionType::Release
    );
    globals.completion_type = FrontendCompletionType::Chain;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION completion_type=DEFAULT",
        2,
    )
    .await;
    assert_eq!(
        state.transaction_settings().unwrap().completion_type,
        FrontendCompletionType::Chain
    );
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_transactions_completion_type_values_reads_and_declared_columns() {
    let mut backend = connect_backend().await;
    for deprecated in [false, true] {
        let (mut state, globals) = fixture();
        if deprecated {
            state.client_capabilities |= CapabilityFlags::CLIENT_DEPRECATE_EOF.bits();
        }
        for (value, expected) in [
            ("0", FrontendCompletionType::NoChain),
            ("1", FrontendCompletionType::Chain),
            ("2", FrontendCompletionType::Release),
            ("TRUE", FrontendCompletionType::Chain),
            ("FALSE", FrontendCompletionType::NoChain),
            ("'chain'", FrontendCompletionType::Chain),
            ("'NO_CHAIN'", FrontendCompletionType::NoChain),
            ("'RELEASE'", FrontendCompletionType::Release),
            ("CHAIN", FrontendCompletionType::Chain),
            ("NO_CHAIN", FrontendCompletionType::NoChain),
            ("RELEASE", FrontendCompletionType::Release),
        ] {
            // SELECT changes FOUND_ROWS, so reset the fixture's prior result count.
            state.found_rows = 18;
            ok(
                &mut state,
                &mut backend,
                &globals,
                &format!("SET @@LOCAL.completion_type={value}"),
                2,
            )
            .await;
            assert_eq!(
                state.transaction_settings().unwrap().completion_type,
                expected
            );
            let (outcome,bytes)=request(&mut state,&mut backend,&globals,"SELECT @@completion_type AS completion, @@global.completion_type AS global_completion").await;
            assert!(matches!(outcome, QueryOutcome::Success(_)));
            let response = packets(&bytes, 253);
            assert_eq!(response.len(), if deprecated { 5 } else { 6 });
            assert_eq!(response[0], [2]);
            for (index, name) in ["completion", "global_completion"].into_iter().enumerate() {
                assert_eq!(
                    column(response[index + 1]),
                    (
                        name.as_bytes().to_vec(),
                        45,
                        87380,
                        darmok_types::mysql_const::field_type::VAR_STRING,
                        0,
                        31
                    )
                );
            }
            let mut row = response[if deprecated { 3 } else { 4 }];
            assert_eq!(
                read_lenenc_bytes(&mut row, "session completion").unwrap(),
                expected.variable_label().as_bytes()
            );
            assert_eq!(
                read_lenenc_bytes(&mut row, "global completion").unwrap(),
                b"NO_CHAIN"
            );
            assert!(row.is_empty());
            assert_eq!(
                *response.last().unwrap(),
                if deprecated {
                    &[0xfe, 0, 0, 2, 0, 0, 0][..]
                } else {
                    &[0xfe, 0, 0, 2, 0][..]
                }
            );
        }
        for value in ["'1'", "ON", "OFF", "3", "NULL", "''"] {
            sql_error(
                &mut state,
                &mut backend,
                &globals,
                &format!("SET completion_type={value}"),
                QuerySqlError::Set(SetSqlError::WrongValue),
            )
            .await;
        }
        state.found_rows = 18;
        ok(
            &mut state,
            &mut backend,
            &globals,
            "SET completion_type=DEFAULT",
            2,
        )
        .await;
        assert_eq!(
            state.transaction_settings().unwrap().completion_type,
            FrontendCompletionType::NoChain
        );
        assert_eq!(
            reference("completion-type-coercions-and-default")["default"],
            "NO_CHAIN"
        );
    }
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_transactions_admit_all_controls_before_implicit_commit() {
    let mut backend = connect_backend().await;
    setup(&backend, "CREATE TEMP TABLE owned_values(n integer)").await;
    let (mut state, globals) = fixture();
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED",
        2,
    )
    .await;
    ok(&mut state, &mut backend, &globals, "BEGIN", 3).await;
    setup(&backend, "INSERT INTO owned_values VALUES(7)").await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION TRANSACTION ISOLATION LEVEL READ UNCOMMITTED",
        3,
    )
    .await;
    // Replacement retains the supported active pair, rather than the future
    // READ UNCOMMITTED default. Its preceding commit is real and lasting.
    ok(&mut state, &mut backend, &globals, "START TRANSACTION", 3).await;
    assert_eq!(
        state
            .transaction_settings()
            .unwrap()
            .active
            .unwrap()
            .isolation,
        FrontendIsolation::ReadCommitted
    );
    assert_eq!(values(&backend).await, [7]);
    setup(&backend, "INSERT INTO owned_values VALUES(8)").await;
    for sql in [
        "START TRANSACTION WITH CONSISTENT SNAPSHOT",
        "START TRANSACTION READ ONLY, WITH CONSISTENT SNAPSHOT",
    ] {
        sql_error(
            &mut state,
            &mut backend,
            &globals,
            sql,
            QuerySqlError::Transaction(TransactionSqlError::ConsistentSnapshot),
        )
        .await;
    }
    for sql in ["COMMIT RELEASE", "ROLLBACK WORK AND NO CHAIN RELEASE"] {
        sql_error(
            &mut state,
            &mut backend,
            &globals,
            sql,
            QuerySqlError::Transaction(TransactionSqlError::Release),
        )
        .await;
    }
    // Rejected controls did not commit8. Chain rollback removes only that work,
    // retains committed7 and still starts with the supported active pair.
    assert_eq!(values(&backend).await, [7, 8]);
    ok(&mut state, &mut backend, &globals, "ROLLBACK AND CHAIN", 3).await;
    assert_eq!(values(&backend).await, [7]);
    for sql in [
        "BEGIN TRANSACTION",
        "START TRANSACTION ISOLATION LEVEL SERIALIZABLE",
        "START TRANSACTION READ ONLY, READ WRITE",
        "COMMIT TRANSACTION",
        "END",
        "COMMIT AND CHAIN RELEASE",
        "ROLLBACK AND CHAIN TO SAVEPOINT old",
    ] {
        sql_error(
            &mut state,
            &mut backend,
            &globals,
            sql,
            QuerySqlError::Parse,
        )
        .await;
    }
    for sql in [
        "ROLLBACK TO SAVEPOINT old",
        "BEGIN; COMMIT",
        "START TRANSACTION; SELECT 1",
    ] {
        let expected = if sql == "ROLLBACK TO SAVEPOINT old" {
            QuerySqlError::Transaction(TransactionSqlError::Unsupported)
        } else {
            QuerySqlError::UnsupportedStatement
        };
        sql_error(&mut state, &mut backend, &globals, sql, expected).await;
    }
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET completion_type=2",
        3,
    )
    .await;
    sql_error(
        &mut state,
        &mut backend,
        &globals,
        "COMMIT",
        QuerySqlError::Transaction(TransactionSqlError::Release),
    )
    .await;
    sql_error(
        &mut state,
        &mut backend,
        &globals,
        "ROLLBACK AND CHAIN",
        QuerySqlError::Transaction(TransactionSqlError::Release),
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "ROLLBACK AND NO CHAIN NO RELEASE",
        2,
    )
    .await;
    assert_eq!(values(&backend).await, [7]);
    sql_error(
        &mut state,
        &mut backend,
        &globals,
        "BEGIN",
        QuerySqlError::Transaction(TransactionSqlError::UnsupportedIsolation),
    )
    .await;
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED, READ ONLY",
        2,
    )
    .await;
    sql_error(
        &mut state,
        &mut backend,
        &globals,
        "COMMIT AND CHAIN NO RELEASE",
        QuerySqlError::Transaction(TransactionSqlError::UnsupportedIsolation),
    )
    .await;
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn query_transactions_replacement_retains_active_pair_then_restores_future_defaults() {
    let mut backend = connect_backend().await;
    for (access, short, future) in [
        ("READ ONLY", "ro", "READ WRITE"),
        ("READ WRITE", "rw", "READ ONLY"),
    ] {
        for (replacement, suffix) in [
            ("BEGIN", "begin"),
            ("START TRANSACTION", "start"),
            ("START TRANSACTION READ ONLY", "start-ro"),
            ("START TRANSACTION READ WRITE", "start-rw"),
        ] {
            replacement_case(
                &mut backend,
                &format!("replacement-default-{short}-{suffix}"),
                &format!("SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED, {access}"),
                "START TRANSACTION",
                future,
                replacement,
            )
            .await;
        }
    }
    for (name, next_access, first, replacement) in [
        (
            "replacement-next-explicit-access-begin",
            "READ ONLY",
            "START TRANSACTION READ WRITE",
            "BEGIN",
        ),
        (
            "replacement-next-start-explicit-access",
            "READ WRITE",
            "BEGIN",
            "START TRANSACTION READ ONLY",
        ),
    ] {
        replacement_case(
            &mut backend,
            name,
            &format!("SET TRANSACTION ISOLATION LEVEL READ COMMITTED, {next_access}"),
            first,
            "READ ONLY",
            replacement,
        )
        .await;
    }
    let _ = backend.dispose().await.unwrap();
}

async fn replacement_case(
    backend: &mut NativeBackend,
    name: &str,
    initial_setting: &str,
    first_start: &str,
    future_access: &str,
    replacement: &str,
) {
    let (mut state, globals) = fixture();
    let expected = reference(name);
    let flags = |pair: &serde_json::Value| {
        if pair["access"] == "READ ONLY" {
            8195
        } else {
            3
        }
    };
    ok(&mut state, backend, &globals, initial_setting, 2).await;
    ok(
        &mut state,
        backend,
        &globals,
        first_start,
        flags(&expected["first"]),
    )
    .await;
    active_pair(&state, backend, &expected["first"]).await;
    ok(
        &mut state,
        backend,
        &globals,
        &format!("SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE, {future_access}"),
        flags(&expected["first"]),
    )
    .await;
    let future = state.transaction_settings().unwrap().defaults;
    assert_eq!(future.isolation, FrontendIsolation::Serializable);
    assert_eq!(
        future.access == FrontendTransactionAccess::ReadOnly,
        expected["future"]["access"] == "READ ONLY"
    );
    ok(
        &mut state,
        backend,
        &globals,
        replacement,
        flags(&expected["replacement"]),
    )
    .await;
    active_pair(&state, backend, &expected["replacement"]).await;
    assert_eq!(state.transaction_settings().unwrap().defaults, future);
    ok(&mut state, backend, &globals, "ROLLBACK AND NO CHAIN", 2).await;
    ok(
        &mut state,
        backend,
        &globals,
        "BEGIN",
        flags(&expected["after_completion"]),
    )
    .await;
    active_pair(&state, backend, &expected["after_completion"]).await;
    ok(&mut state, backend, &globals, "ROLLBACK AND NO CHAIN", 2).await;
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 default and BigDecimal CI steps"]
async fn query_transactions_set_numeric_admission_uses_original_tokens_before_commit() {
    let mut backend = connect_backend().await;
    setup(&backend, "CREATE TEMP TABLE owned_values(n integer)").await;
    let (mut state, globals) = fixture();
    for variable in [
        "sql_mode",
        "autocommit",
        "transaction_isolation",
        "transaction_read_only",
        "completion_type",
    ] {
        for token in ["0.0", "0e0", ".0", "0.", "1e0", "1.0"] {
            sql_error(
                &mut state,
                &mut backend,
                &globals,
                &format!("SET /* hé */ {variable}={token}"),
                QuerySqlError::Set(SetSqlError::Unsupported),
            )
            .await;
        }
    }
    ok(
        &mut state,
        &mut backend,
        &globals,
        "SET /* hé */ sql_mode=000, autocommit=001, completion_type=000,\n transaction_isolation=001, transaction_read_only=000",
        2,
    )
    .await;
    assert_eq!(state.sql_modes().unwrap(), SqlModes::empty());
    assert_eq!(
        state.transaction_settings().unwrap().defaults.isolation,
        FrontendIsolation::ReadCommitted
    );
    ok(&mut state, &mut backend, &globals, "SET autocommit=0", 0).await;
    ok(&mut state, &mut backend, &globals, "BEGIN", 1).await;
    setup(&backend, "INSERT INTO owned_values VALUES(9)").await;
    sql_error(
        &mut state,
        &mut backend,
        &globals,
        "SET autocommit=1, completion_type=1e0",
        QuerySqlError::Set(SetSqlError::Unsupported),
    )
    .await;
    assert_eq!(values(&backend).await, [9]);
    assert_eq!(
        state.transaction_settings().unwrap().autocommit,
        AutocommitSetting::Disabled
    );
    ok(
        &mut state,
        &mut backend,
        &globals,
        "ROLLBACK AND NO CHAIN",
        0,
    )
    .await;
    assert!(values(&backend).await.is_empty());
    ok(&mut state, &mut backend, &globals, "SET autocommit=1", 2).await;
    let _ = backend.dispose().await.unwrap();
}
