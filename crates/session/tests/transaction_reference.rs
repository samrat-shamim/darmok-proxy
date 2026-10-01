//! Logical setting projections against the existing stock corpus. These traces
//! supply trusted frontend boundaries in memory; they do not parse/execute the
//! corpus SQL, validate native receipts, or test rows, locks, events or packets.

use darmok_session::*;
use darmok_types::value::Value as SessionValue;
use serde_json::Value;

use FrontendIsolation::{ReadCommitted, ReadUncommitted, RepeatableRead, Serializable};
use FrontendTransactionAccess::{ReadOnly, ReadWrite};
use TransactionCompletion::{Commit, Rollback};

fn oracle(name: &str) -> Value {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/reference/mysql_transaction_characteristics.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    cases.iter().find(|case| case["name"] == name).unwrap()["expected_effect"].clone()
}

fn choices(
    isolation: FrontendIsolation,
    access: FrontendTransactionAccess,
) -> TransactionCharacteristics {
    TransactionCharacteristics { isolation, access }
}

fn one(characteristic: NamedTransactionCharacteristic) -> TransactionCharacteristicUpdate {
    TransactionCharacteristicUpdate::One(characteristic)
}

fn apply(
    state: &mut SessionState,
    command: FrontendTransactionCommand,
) -> Vec<FrontendTransactionBoundary> {
    let mut stage = state.stage_transaction_command(command).unwrap();
    stage.mark_submitted().unwrap();
    let mut actions = Vec::new();
    while let Some(boundary) = stage.next_frontend_boundary() {
        stage.record_confirmed_frontend_boundary(boundary).unwrap();
        actions.push(boundary);
    }
    stage.finish_success_with_validated_output().unwrap();
    actions
}

fn assign(state: &mut SessionState, assignment: TransactionSettingAssignment) {
    let expected = FrontendTransactionBoundary::SettingsAssigned(assignment);
    assert_eq!(
        apply(state, FrontendTransactionCommand::Assign(assignment)),
        vec![expected]
    );
}

fn session(
    state: &mut SessionState,
    isolation: FrontendIsolation,
    access: FrontendTransactionAccess,
) {
    assign(
        state,
        TransactionSettingAssignment::SessionTransaction(TransactionCharacteristicUpdate::Both(
            choices(isolation, access),
        )),
    );
}

fn next(state: &mut SessionState, isolation: FrontendIsolation, access: FrontendTransactionAccess) {
    assign(
        state,
        TransactionSettingAssignment::NextTransaction(TransactionCharacteristicUpdate::Both(
            choices(isolation, access),
        )),
    );
}

fn variable(
    state: &mut SessionState,
    form: TransactionVariableAssignmentForm,
    characteristic: NamedTransactionCharacteristic,
) {
    assign(
        state,
        TransactionSettingAssignment::Variable {
            form,
            characteristic,
        },
    );
}

fn begin(state: &mut SessionState, access: Option<FrontendTransactionAccess>) {
    apply(state, FrontendTransactionCommand::BeginExplicit { access });
}

fn end(state: &mut SessionState, completion: TransactionCompletion) {
    apply(
        state,
        FrontendTransactionCommand::Complete {
            completion,
            chain: false,
        },
    );
}

fn assert_projection(state: &SessionState, selected: TransactionCharacteristics, expected: &Value) {
    let snapshot = state.transaction_settings().unwrap();
    assert_eq!(
        selected.isolation.variable_label().replace('-', " "),
        expected["isolation"].as_str().unwrap()
    );
    assert_eq!(
        match selected.access {
            ReadOnly => "READ ONLY",
            ReadWrite => "READ WRITE",
        },
        expected["access"].as_str().unwrap()
    );
    assert_eq!(
        snapshot.defaults.isolation.variable_label(),
        expected["default_isolation"].as_str().unwrap()
    );
    assert_eq!(
        u64::from(snapshot.defaults.access == ReadOnly),
        expected["default_read_only"].as_u64().unwrap()
    );
    assert_eq!(
        u64::from(snapshot.autocommit == AutocommitSetting::Enabled),
        expected["session_autocommit"].as_u64().unwrap()
    );
    assert_eq!(
        state.get_system_var("transaction_isolation").unwrap(),
        Some(SessionValue::String(
            snapshot.defaults.isolation.variable_label().into()
        ))
    );
    assert_eq!(
        state.get_system_var("transaction_read_only").unwrap(),
        Some(SessionValue::UInt(u64::from(
            snapshot.defaults.access == ReadOnly
        )))
    );
    assert_eq!(
        state.get_system_var("autocommit").unwrap(),
        Some(SessionValue::UInt(u64::from(
            snapshot.autocommit == AutocommitSetting::Enabled
        )))
    );
}

fn capture(state: &SessionState, expected: &Value, key: &str) {
    let active = state
        .transaction_settings()
        .unwrap()
        .active
        .expect("this trace captures an active frontend transaction");
    assert_projection(state, active, &expected[key]);
}

fn defaults(state: &SessionState, expected: &Value) {
    let snapshot = state.transaction_settings().unwrap();
    assert_eq!(
        snapshot.defaults.isolation.variable_label(),
        expected["isolation"].as_str().unwrap()
    );
    assert_eq!(
        u64::from(snapshot.defaults.access == ReadOnly),
        expected["read_only"].as_u64().unwrap()
    );
}

#[test]
fn all_four_isolations_and_both_access_modes() {
    let expected = oracle("all-four-isolations-and-both-access-modes");
    let mut state = SessionState::new(1);
    for (isolation, prefix) in [
        (ReadCommitted, "read_committed"),
        (RepeatableRead, "repeatable_read"),
        (Serializable, "serializable"),
        (ReadUncommitted, "read_uncommitted"),
    ] {
        for (access, suffix) in [(ReadWrite, "read_write"), (ReadOnly, "read_only")] {
            session(&mut state, isolation, access);
            begin(&mut state, None);
            capture(&state, &expected, &format!("{prefix}_{suffix}"));
            end(&mut state, Rollback);
        }
    }
}

#[test]
fn next_only_controls_one_explicit_transaction() {
    let expected = oracle("next-only-controls-one-explicit-transaction");
    let mut state = SessionState::new(2);
    session(&mut state, RepeatableRead, ReadWrite);
    next(&mut state, ReadCommitted, ReadOnly);
    defaults(&state, &expected["defaults"]);
    begin(&mut state, None);
    capture(&state, &expected, "first");
    end(&mut state, Commit);
    begin(&mut state, None);
    capture(&state, &expected, "second");
    end(&mut state, Rollback);
}

#[test]
fn session_update_inside_active_keeps_active_modes() {
    let expected = oracle("session-update-inside-active-keeps-active-modes");
    let mut state = SessionState::new(3);
    session(&mut state, ReadCommitted, ReadWrite);
    begin(&mut state, Some(ReadOnly));
    capture(&state, &expected, "before");
    session(&mut state, Serializable, ReadWrite);
    capture(&state, &expected, "after");
    end(&mut state, Commit);
    begin(&mut state, None);
    capture(&state, &expected, "next");
    end(&mut state, Rollback);
}

#[test]
fn per_characteristic_next_updates_compose() {
    let expected = oracle("per-characteristic-next-updates-compose");
    let mut state = SessionState::new(4);
    assign(
        &mut state,
        TransactionSettingAssignment::NextTransaction(one(
            NamedTransactionCharacteristic::Isolation(ReadCommitted),
        )),
    );
    assign(
        &mut state,
        TransactionSettingAssignment::NextTransaction(one(NamedTransactionCharacteristic::Access(
            ReadOnly,
        ))),
    );
    begin(&mut state, None);
    capture(&state, &expected, "first");
    end(&mut state, Rollback);
    begin(&mut state, None);
    capture(&state, &expected, "second");
    end(&mut state, Rollback);
}

#[test]
fn session_update_supersedes_only_named_next() {
    let expected = oracle("session-update-between-transactions-overrides-only-named-next");
    let mut state = SessionState::new(5);
    next(&mut state, Serializable, ReadOnly);
    assign(
        &mut state,
        TransactionSettingAssignment::SessionTransaction(one(
            NamedTransactionCharacteristic::Isolation(ReadCommitted),
        )),
    );
    begin(&mut state, None);
    capture(&state, &expected, "first");
    end(&mut state, Rollback);
    begin(&mut state, None);
    capture(&state, &expected, "second");
    end(&mut state, Rollback);
}

#[test]
fn explicit_access_override_is_one_transaction() {
    let expected = oracle("explicit-start-access-overrides-next-and-is-one-only");
    let mut state = SessionState::new(6);
    session(&mut state, RepeatableRead, ReadOnly);
    next(&mut state, ReadCommitted, ReadOnly);
    begin(&mut state, Some(ReadWrite));
    capture(&state, &expected, "first");
    end(&mut state, Rollback);
    begin(&mut state, None);
    capture(&state, &expected, "second");
    end(&mut state, Commit);
}

#[test]
fn rejected_next_changes_preserve_active_choices_and_future_defaults() {
    let expected = oracle("next-only-set-inside-active-rejected-and-prior-work-retained");
    let mut state = SessionState::new(7);
    session(&mut state, ReadCommitted, ReadWrite);
    begin(&mut state, None);
    let before = state.transaction_settings().unwrap();
    for change in [
        NamedTransactionCharacteristic::Isolation(Serializable),
        NamedTransactionCharacteristic::Access(ReadOnly),
    ] {
        assert_eq!(
            state
                .stage_transaction_command(FrontendTransactionCommand::Assign(
                    TransactionSettingAssignment::NextTransaction(one(change))
                ))
                .unwrap_err(),
            TransactionSettingsError::NextChoicesDuringActive
        );
        assert_eq!(state.transaction_settings().unwrap(), before);
    }
    capture(&state, &expected, "active");
    end(&mut state, Commit);
    begin(&mut state, None);
    capture(&state, &expected, "next");
    end(&mut state, Rollback);
}

#[test]
fn implicit_transactions_consume_next_after_autocommit_off() {
    let expected = oracle("implicit-transaction-consumes-next-after-autocommit-off");
    let mut state = SessionState::new(8);
    apply(
        &mut state,
        FrontendTransactionCommand::SetAutocommit(AutocommitSetting::Disabled),
    );
    next(&mut state, ReadCommitted, ReadOnly);
    apply(&mut state, FrontendTransactionCommand::BeginImplicit);
    capture(&state, &expected, "first");
    end(&mut state, Commit);
    apply(&mut state, FrontendTransactionCommand::BeginImplicit);
    capture(&state, &expected, "second");
    end(&mut state, Rollback);
    apply(
        &mut state,
        FrontendTransactionCommand::SetAutocommit(AutocommitSetting::Enabled),
    );
}

#[test]
fn autocommit_statement_consumes_next_without_changing_defaults() {
    let expected = oracle("autocommit-next-consumed-after-transactional-statement");
    let mut state = SessionState::new(9);
    next(&mut state, ReadCommitted, ReadOnly);
    let pending = state.transaction_settings().unwrap().next;
    // The recorded DO, literal SELECT and default-variable read are not
    // transaction commands in this trace. No statement classifier is tested.
    assert_eq!(
        state.get_system_var("transaction_isolation").unwrap(),
        Some(SessionValue::String(
            expected["visible"].as_str().unwrap().into()
        ))
    );
    assert_eq!(state.transaction_settings().unwrap().next, pending);
    for key in ["first", "second"] {
        let actions = apply(&mut state, FrontendTransactionCommand::AutocommitStatement);
        let [
            FrontendTransactionBoundary::Started(selected),
            FrontendTransactionBoundary::Ended(Commit),
        ] = actions.as_slice()
        else {
            panic!("unexpected successful autocommit actions")
        };
        assert_eq!(state.transaction_settings().unwrap().active, None);
        // MySQL retains the completed event; it is not current frontend active
        // state. Compare its selected pair, without inventing event identity.
        assert_projection(&state, *selected, &expected[key]);
    }
}

#[test]
fn chains_retain_active_characteristics_over_changed_defaults() {
    let expected = oracle("completion-chain-retains-active-characteristics");
    let mut state = SessionState::new(10);
    session(&mut state, ReadCommitted, ReadWrite);
    begin(&mut state, Some(ReadOnly));
    capture(&state, &expected, "before");
    session(&mut state, Serializable, ReadWrite);
    for (completion, key) in [(Commit, "commit_chain"), (Rollback, "rollback_chain")] {
        assert_eq!(
            apply(
                &mut state,
                FrontendTransactionCommand::Complete {
                    completion,
                    chain: true
                }
            ),
            vec![
                FrontendTransactionBoundary::Ended(completion),
                FrontendTransactionBoundary::Started(choices(ReadCommitted, ReadOnly))
            ]
        );
        capture(&state, &expected, key);
    }
    end(&mut state, Rollback);
    begin(&mut state, None);
    capture(&state, &expected, "next");
    end(&mut state, Commit);
}

#[test]
fn variable_assignment_forms_keep_their_distinct_targets() {
    let expected = oracle("variable-assignment-distinguishes-bare-and-unqualified-at-scope");
    let mut state = SessionState::new(11);
    variable(
        &mut state,
        TransactionVariableAssignmentForm::SessionKeyword,
        NamedTransactionCharacteristic::Isolation(ReadCommitted),
    );
    variable(
        &mut state,
        TransactionVariableAssignmentForm::SessionKeyword,
        NamedTransactionCharacteristic::Access(ReadWrite),
    );
    variable(
        &mut state,
        TransactionVariableAssignmentForm::UnqualifiedAt,
        NamedTransactionCharacteristic::Isolation(Serializable),
    );
    variable(
        &mut state,
        TransactionVariableAssignmentForm::UnqualifiedAt,
        NamedTransactionCharacteristic::Access(ReadOnly),
    );
    begin(&mut state, None);
    capture(&state, &expected, "first");
    end(&mut state, Rollback);
    variable(
        &mut state,
        TransactionVariableAssignmentForm::Bare,
        NamedTransactionCharacteristic::Isolation(RepeatableRead),
    );
    variable(
        &mut state,
        TransactionVariableAssignmentForm::Bare,
        NamedTransactionCharacteristic::Access(ReadWrite),
    );
    begin(&mut state, None);
    capture(&state, &expected, "second");
    end(&mut state, Commit);
}

#[test]
fn reverse_next_order_preserves_pending_access() {
    let expected = oracle("reverse-next-updates-preserve-pending-access");
    let mut state = SessionState::new(12);
    assign(
        &mut state,
        TransactionSettingAssignment::NextTransaction(one(NamedTransactionCharacteristic::Access(
            ReadOnly,
        ))),
    );
    assign(
        &mut state,
        TransactionSettingAssignment::NextTransaction(one(
            NamedTransactionCharacteristic::Isolation(ReadCommitted),
        )),
    );
    begin(&mut state, None);
    capture(&state, &expected, "first");
    end(&mut state, Rollback);
    begin(&mut state, None);
    capture(&state, &expected, "second");
    end(&mut state, Rollback);
}

#[test]
fn distinct_defaults_detect_retention_of_overridden_next_access() {
    let expected = oracle("explicit-access-discards-overridden-next-with-distinct-defaults");
    let mut state = SessionState::new(13);
    for (default_access, pending_isolation, pending_access, override_key, after_key) in [
        (
            ReadWrite,
            ReadCommitted,
            ReadOnly,
            "override_to_write",
            "after_write_override",
        ),
        (
            ReadOnly,
            Serializable,
            ReadWrite,
            "override_to_read",
            "after_read_override",
        ),
    ] {
        session(&mut state, RepeatableRead, default_access);
        next(&mut state, pending_isolation, pending_access);
        begin(&mut state, Some(default_access));
        capture(&state, &expected, override_key);
        end(&mut state, Rollback);
        begin(&mut state, None);
        capture(&state, &expected, after_key);
        end(&mut state, Rollback);
    }
}

#[test]
fn qualified_session_assignments_change_defaults_not_active_choices() {
    let expected = oracle("qualified-session-assignments-update-named-defaults-not-active");
    let mut state = SessionState::new(14);
    next(&mut state, Serializable, ReadOnly);
    variable(
        &mut state,
        TransactionVariableAssignmentForm::QualifiedSession,
        NamedTransactionCharacteristic::Access(ReadWrite),
    );
    defaults(&state, &expected["defaults"]);
    begin(&mut state, None);
    capture(&state, &expected, "first");
    variable(
        &mut state,
        TransactionVariableAssignmentForm::QualifiedSession,
        NamedTransactionCharacteristic::Isolation(ReadCommitted),
    );
    variable(
        &mut state,
        TransactionVariableAssignmentForm::QualifiedSession,
        NamedTransactionCharacteristic::Access(ReadOnly),
    );
    capture(&state, &expected, "after_session_update");
    end(&mut state, Rollback);
    begin(&mut state, None);
    capture(&state, &expected, "second");
    end(&mut state, Commit);
    begin(&mut state, None);
    capture(&state, &expected, "third");
    end(&mut state, Rollback);
}
