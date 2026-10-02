// Ordinary SET controller fixtures, required by the native-owner CI step.
// Active frontend starts are trusted fixture setup, not a start-controller or
// native isolation equivalence claim. Private DML setup is not a public API.
use super::*;
use crate::{ServerSetValues, SetOutcome, SetSqlError, execute_mysql_set};
use darmok_protocol::CapabilityFlags;
use darmok_session::{
    AutocommitSetting, FrontendIsolation, FrontendTransactionAccess, FrontendTransactionCommand,
    SessionState, SqlMode, SqlModes, TransactionCharacteristics, TransactionCompletion,
};
use sqlparser::{ast::Statement, mysql_mode::parse_mysql_with_mode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn set_fixture() -> (SessionState, ServerSetValues) {
    let mut state = SessionState::new(1);
    state.client_capabilities = CapabilityFlags::CLIENT_PROTOCOL_41.bits();
    let globals = ServerSetValues {
        sql_modes: SqlModes::MYSQL84_DEFAULT,
        transactions: TransactionCharacteristics {
            isolation: FrontendIsolation::RepeatableRead,
            access: FrontendTransactionAccess::ReadWrite,
        },
        autocommit: AutocommitSetting::Enabled,
    };
    (state, globals)
}

async fn set_request(
    state: &mut SessionState,
    backend: &mut NativeBackend,
    globals: &ServerSetValues,
    sql: &str,
) -> (SetOutcome, Vec<u8>) {
    let parsed = parse_mysql_with_mode(sql, state.sql_modes().unwrap().parser_flags()).unwrap();
    let [Statement::Set(input)] = parsed.as_slice() else {
        panic!("one SET expected")
    };
    let (mut writer, mut reader) = tokio::io::duplex(1024);
    let outcome = execute_mysql_set(state, backend, globals, input, 7, &mut writer)
        .await
        .unwrap();
    writer.shutdown().await.unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes[3], 7);
    assert_eq!(
        usize::from(bytes[0]) | (usize::from(bytes[1]) << 8) | (usize::from(bytes[2]) << 16),
        bytes.len() - 4
    );
    assert!(state.unconfirmed_set_command().is_none());
    (outcome, bytes)
}

fn set_success(outcome: SetOutcome) {
    assert!(matches!(outcome, SetOutcome::Success(_)), "{outcome:?}");
}

fn set_error(outcome: SetOutcome, bytes: &[u8], expected: SetSqlError) {
    assert!(matches!(outcome, SetOutcome::SqlError { error, .. } if error == expected));
    assert_eq!(bytes[4], 0xff);
    assert_eq!(u16::from_le_bytes([bytes[5], bytes[6]]), expected.code());
    assert_eq!(
        &bytes[7..13],
        &[
            b'#',
            expected.sql_state()[0],
            expected.sql_state()[1],
            expected.sql_state()[2],
            expected.sql_state()[3],
            expected.sql_state()[4]
        ]
    );
}

fn fixture_frontend_command(state: &mut SessionState, command: FrontendTransactionCommand) {
    let mut stage = state.stage_transaction_command(command).unwrap();
    stage.mark_submitted().unwrap();
    while let Some(boundary) = stage.next_frontend_boundary() {
        stage.record_confirmed_frontend_boundary(boundary).unwrap();
    }
    stage.finish_success_with_validated_output().unwrap();
}

async fn fixture_start(state: &mut SessionState, backend: &mut NativeBackend) {
    let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
    fixture_frontend_command(
        state,
        FrontendTransactionCommand::BeginExplicit { access: None },
    );
}

async fn fixture_rollback(state: &mut SessionState, backend: &mut NativeBackend) {
    let _ = backend.rollback().await.unwrap();
    fixture_frontend_command(
        state,
        FrontendTransactionCommand::Complete {
            completion: TransactionCompletion::Rollback,
            chain: false,
        },
    );
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_scope_context_and_next_choices_are_distinct() {
    let mut backend = connect_backend().await;
    for (sql, default_isolation, default_access, next_isolation, next_access) in [
        (
            "SET SESSION sql_mode='', @@transaction_isolation='SERIALIZABLE', transaction_read_only=1",
            FrontendIsolation::RepeatableRead,
            FrontendTransactionAccess::ReadOnly,
            Some(FrontendIsolation::Serializable),
            None,
        ),
        (
            "SET LOCAL transaction_isolation='READ-COMMITTED', @@transaction_isolation='SERIALIZABLE', transaction_read_only=1",
            FrontendIsolation::ReadCommitted,
            FrontendTransactionAccess::ReadOnly,
            Some(FrontendIsolation::Serializable),
            None,
        ),
        (
            "SET @@SESSION.transaction_isolation='READ-COMMITTED', @@transaction_read_only=1, transaction_isolation='REPEATABLE-READ'",
            FrontendIsolation::RepeatableRead,
            FrontendTransactionAccess::ReadWrite,
            None,
            Some(FrontendTransactionAccess::ReadOnly),
        ),
    ] {
        let (mut state, globals) = set_fixture();
        let (outcome, _) = set_request(&mut state, &mut backend, &globals, sql).await;
        set_success(outcome);
        let settings = state.transaction_settings().unwrap();
        assert_eq!(
            settings.defaults,
            TransactionCharacteristics {
                isolation: default_isolation,
                access: default_access
            }
        );
        assert_eq!(settings.next.isolation, next_isolation);
        assert_eq!(settings.next.access, next_access);
        assert!(settings.active.is_none());
    }
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_expressions_read_pre_update_values() {
    let mut backend = connect_backend().await;
    let (mut state, globals) = set_fixture();
    set_success(
        set_request(
            &mut state,
            &mut backend,
            &globals,
            "SET SESSION sql_mode='PIPES_AS_CONCAT', transaction_isolation='READ-COMMITTED'",
        )
        .await
        .0,
    );
    set_success(set_request(&mut state, &mut backend, &globals,
        "SET SESSION sql_mode='ANSI_QUOTES', sql_mode=@@session.sql_mode, transaction_isolation='SERIALIZABLE', transaction_isolation=@@session.transaction_isolation, autocommit=0, transaction_read_only=@@session.autocommit").await.0);
    assert_eq!(
        state.sql_modes().unwrap().canonical_names(),
        "PIPES_AS_CONCAT"
    );
    let settings = state.transaction_settings().unwrap();
    assert_eq!(
        settings.defaults.isolation,
        FrontendIsolation::ReadCommitted
    );
    assert_eq!(
        settings.defaults.access,
        FrontendTransactionAccess::ReadOnly
    );
    assert_eq!(settings.autocommit, AutocommitSetting::Disabled);
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_all_ordinary_checks_precede_every_update() {
    let mut backend = connect_backend().await;
    let (mut state, globals) = set_fixture();
    set_success(
        set_request(
            &mut state,
            &mut backend,
            &globals,
            "SET SESSION sql_mode='PIPES_AS_CONCAT'",
        )
        .await
        .0,
    );
    for sql in [
        "SET SESSION sql_mode='ANSI_QUOTES', transaction_isolation='NOT-AN-ISOLATION'",
        "SET SESSION transaction_read_only=NULL, sql_mode='ANSI_QUOTES'",
        "SET SESSION sql_mode=DEFAULT, transaction_read_only=NULL",
    ] {
        let before = state.translation_fingerprint().unwrap();
        let (outcome, bytes) = set_request(&mut state, &mut backend, &globals, sql).await;
        set_error(outcome, &bytes, SetSqlError::WrongValue);
        assert_eq!(state.translation_fingerprint().unwrap(), before);
        assert_eq!(state.error_count(), 1);
    }
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_default_error_preserves_prior_mode_and_native_work() {
    let mut backend = connect_backend().await;
    setup(&backend, "CREATE TEMP TABLE set_values (id integer PRIMARY KEY, n integer NOT NULL); INSERT INTO set_values VALUES (0,0)").await;
    for variable in ["transaction_isolation", "transaction_read_only"] {
        let (mut state, globals) = set_fixture();
        set_success(
            set_request(
                &mut state,
                &mut backend,
                &globals,
                "SET SESSION sql_mode=''",
            )
            .await
            .0,
        );
        fixture_start(&mut state, &mut backend).await;
        setup(&backend, "UPDATE set_values SET n=1 WHERE id=0").await;
        let sql = format!("SET SESSION sql_mode='ANSI_QUOTES', @@{variable}=DEFAULT");
        let (outcome, bytes) = set_request(&mut state, &mut backend, &globals, &sql).await;
        set_error(outcome, &bytes, SetSqlError::NextChoicesDuringActive);
        assert_eq!(state.sql_modes().unwrap().canonical_names(), "ANSI_QUOTES");
        assert_eq!(
            client(&backend)
                .query_one("SELECT n FROM set_values WHERE id=0", &[])
                .await
                .unwrap()
                .get::<_, i32>(0),
            1
        );
        assert_eq!(
            backend.state(),
            NativeBackendState::Ready(TransactionState::Transaction)
        );
        fixture_rollback(&mut state, &mut backend).await;
        assert_eq!(state.sql_modes().unwrap().canonical_names(), "ANSI_QUOTES");
        assert_eq!(
            client(&backend)
                .query_one("SELECT n FROM set_values WHERE id=0", &[])
                .await
                .unwrap()
                .get::<_, i32>(0),
            0
        );
    }
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_early_default_and_check_errors_prevent_later_updates() {
    let mut backend = connect_backend().await;
    let (mut state, globals) = set_fixture();
    set_success(
        set_request(
            &mut state,
            &mut backend,
            &globals,
            "SET SESSION sql_mode='PIPES_AS_CONCAT'",
        )
        .await
        .0,
    );
    fixture_start(&mut state, &mut backend).await;
    for sql in [
        "SET @@transaction_isolation=DEFAULT, SESSION sql_mode='ANSI_QUOTES'",
        "SET SESSION sql_mode='ANSI_QUOTES', @@transaction_isolation='SERIALIZABLE'",
    ] {
        let before = state.translation_fingerprint().unwrap();
        let (outcome, bytes) = set_request(&mut state, &mut backend, &globals, sql).await;
        set_error(outcome, &bytes, SetSqlError::NextChoicesDuringActive);
        assert_eq!(state.translation_fingerprint().unwrap(), before);
    }
    fixture_rollback(&mut state, &mut backend).await;
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_session_changes_preserve_active_choices_and_local_transaction_scope() {
    let mut backend = connect_backend().await;
    let (mut state, globals) = set_fixture();
    fixture_start(&mut state, &mut backend).await;
    let active = state.transaction_settings().unwrap().active;
    set_success(
        set_request(
            &mut state,
            &mut backend,
            &globals,
            "SET SESSION sql_mode='ANSI_QUOTES', transaction_isolation='READ-COMMITTED'",
        )
        .await
        .0,
    );
    set_success(
        set_request(
            &mut state,
            &mut backend,
            &globals,
            "SET LOCAL TRANSACTION ISOLATION LEVEL READ COMMITTED, READ ONLY",
        )
        .await
        .0,
    );
    let settings = state.transaction_settings().unwrap();
    assert_eq!(settings.active, active);
    assert_eq!(
        settings.defaults.isolation,
        FrontendIsolation::ReadCommitted
    );
    assert_eq!(
        settings.defaults.access,
        FrontendTransactionAccess::ReadOnly
    );
    fixture_rollback(&mut state, &mut backend).await;
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_defaults_use_the_supplied_current_global_authority() {
    let mut backend = connect_backend().await;
    let (mut state, mut globals) = set_fixture();
    globals.sql_modes = SqlModes::empty().with(SqlMode::PipesAsConcat);
    globals.transactions = TransactionCharacteristics {
        isolation: FrontendIsolation::ReadCommitted,
        access: FrontendTransactionAccess::ReadOnly,
    };
    globals.autocommit = AutocommitSetting::Disabled;
    set_success(set_request(&mut state, &mut backend, &globals, "SET SESSION sql_mode='ANSI_QUOTES', transaction_isolation='SERIALIZABLE', transaction_read_only=0").await.0);
    set_success(set_request(&mut state, &mut backend, &globals, "SET SESSION sql_mode=DEFAULT, transaction_isolation=DEFAULT, transaction_read_only=DEFAULT, autocommit=DEFAULT").await.0);
    assert_eq!(state.sql_modes().unwrap(), globals.sql_modes);
    let settings = state.transaction_settings().unwrap();
    assert_eq!(settings.defaults, globals.transactions);
    assert_eq!(settings.autocommit, globals.autocommit);
    globals.transactions = TransactionCharacteristics {
        isolation: FrontendIsolation::Serializable,
        access: FrontendTransactionAccess::ReadWrite,
    };
    set_success(
        set_request(
            &mut state,
            &mut backend,
            &globals,
            "SET @@transaction_isolation=DEFAULT, @@transaction_read_only=DEFAULT",
        )
        .await
        .0,
    );
    let next = state.transaction_settings().unwrap();
    assert_eq!(next.defaults, settings.defaults);
    assert_eq!(next.next.isolation, Some(globals.transactions.isolation));
    assert_eq!(next.next.access, Some(globals.transactions.access));
    set_success(
        set_request(
            &mut state,
            &mut backend,
            &globals,
            "SET SESSION transaction_isolation=@@global.transaction_isolation",
        )
        .await
        .0,
    );
    assert_eq!(
        state.transaction_settings().unwrap().defaults.isolation,
        FrontendIsolation::Serializable
    );
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_selected_coercions_and_success_packet_status() {
    let mut backend = connect_backend().await;
    let (mut state, globals) = set_fixture();
    let (outcome, bytes) = set_request(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION autocommit='OFF', transaction_read_only='ON', sql_mode='NO_BACKSLASH_ESCAPES'",
    )
    .await;
    set_success(outcome);
    assert_eq!(bytes, [7, 0, 0, 7, 0, 0, 0, 0, 2, 0, 0]);
    set_success(
        set_request(
            &mut state,
            &mut backend,
            &globals,
            "SET SESSION autocommit=TRUE, transaction_read_only=FALSE",
        )
        .await
        .0,
    );
    let (outcome, bytes) = set_request(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION sql_mode=4, transaction_isolation=0",
    )
    .await;
    set_success(outcome);
    assert_eq!(bytes, [7, 0, 0, 7, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(state.sql_modes().unwrap().canonical_names(), "ANSI_QUOTES");
    assert_eq!(
        state.transaction_settings().unwrap().defaults.isolation,
        FrontendIsolation::ReadUncommitted
    );
    assert_eq!(state.error_count(), 0);
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_autocommit_commit_has_native_table_effect_and_then_settings_output() {
    let mut backend = connect_backend().await;
    setup(&backend, "CREATE TEMP TABLE set_commit (id integer PRIMARY KEY, n integer NOT NULL); INSERT INTO set_commit VALUES (0,0)").await;
    let (mut state, globals) = set_fixture();
    set_success(
        set_request(&mut state, &mut backend, &globals, "SET autocommit=0")
            .await
            .0,
    );
    fixture_start(&mut state, &mut backend).await;
    setup(&backend, "UPDATE set_commit SET n=9 WHERE id=0").await;
    let (outcome, bytes) = set_request(
        &mut state,
        &mut backend,
        &globals,
        "SET SESSION sql_mode='ANSI_QUOTES', autocommit=1",
    )
    .await;
    set_success(outcome);
    assert_eq!(bytes, [7, 0, 0, 7, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
    let settings = state.transaction_settings().unwrap();
    assert!(settings.active.is_none());
    assert_eq!(settings.autocommit, AutocommitSetting::Enabled);
    let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
    let _ = backend.rollback().await.unwrap();
    assert_eq!(
        client(&backend)
            .query_one("SELECT n FROM set_commit WHERE id=0", &[])
            .await
            .unwrap()
            .get::<_, i32>(0),
        9
    );
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn set_controller_unsupported_default_fails_admission_before_settings_or_commit() {
    let mut backend = connect_backend().await;
    setup(&backend, "CREATE TEMP TABLE set_support (id integer PRIMARY KEY, n integer NOT NULL); INSERT INTO set_support VALUES (0,0)").await;
    let (mut state, mut globals) = set_fixture();
    globals.sql_modes = SqlModes::empty().with(SqlMode::PadCharToFullLength);
    let before = state.translation_fingerprint().unwrap();
    let (outcome, bytes) = set_request(
        &mut state,
        &mut backend,
        &globals,
        "SET autocommit=0, sql_mode=DEFAULT",
    )
    .await;
    set_error(outcome, &bytes, SetSqlError::Unsupported);
    assert_eq!(state.translation_fingerprint().unwrap(), before);
    set_success(
        set_request(&mut state, &mut backend, &globals, "SET autocommit=0")
            .await
            .0,
    );
    fixture_start(&mut state, &mut backend).await;
    setup(&backend, "UPDATE set_support SET n=7 WHERE id=0").await;
    let before = state.translation_fingerprint().unwrap();
    let (outcome, bytes) = set_request(
        &mut state,
        &mut backend,
        &globals,
        "SET autocommit=1, sql_mode=DEFAULT",
    )
    .await;
    set_error(outcome, &bytes, SetSqlError::Unsupported);
    assert_eq!(state.translation_fingerprint().unwrap(), before);
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Transaction)
    );
    fixture_rollback(&mut state, &mut backend).await;
    assert_eq!(
        client(&backend)
            .query_one("SELECT n FROM set_support WHERE id=0", &[])
            .await
            .unwrap()
            .get::<_, i32>(0),
        0
    );
    let _ = backend.dispose().await.unwrap();
}
