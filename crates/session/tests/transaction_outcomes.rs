use darmok_session::*;
use darmok_types::value::Value;

use FrontendIsolation::{ReadCommitted, RepeatableRead, Serializable};
use FrontendTransactionAccess::{ReadOnly, ReadWrite};
use TransactionCompletion::Commit;

fn choices(
    isolation: FrontendIsolation,
    access: FrontendTransactionAccess,
) -> TransactionCharacteristics {
    TransactionCharacteristics { isolation, access }
}

fn settle(state: &mut SessionState, command: FrontendTransactionCommand) {
    let mut stage = state.stage_transaction_command(command).unwrap();
    stage.mark_submitted().unwrap();
    while let Some(boundary) = stage.next_frontend_boundary() {
        stage.record_confirmed_frontend_boundary(boundary).unwrap();
    }
    stage.finish_success_with_validated_output().unwrap();
}

fn pending(state: &mut SessionState) {
    settle(
        state,
        FrontendTransactionCommand::Assign(TransactionSettingAssignment::NextTransaction(
            TransactionCharacteristicUpdate::Both(choices(Serializable, ReadOnly)),
        )),
    );
}

fn begin(state: &mut SessionState) {
    settle(
        state,
        FrontendTransactionCommand::BeginExplicit { access: None },
    );
}

fn assert_unsettled(state: &mut SessionState) {
    assert_eq!(
        state.transaction_settings(),
        Err(TransactionSettingsError::UnsettledOutcome)
    );
    assert!(matches!(
        state.translation_fingerprint(),
        Err(SessionVariableError::TransactionOutcome(
            TransactionSettingsError::UnsettledOutcome
        ))
    ));
    for name in [
        "autocommit",
        "transaction_isolation",
        "transaction_read_only",
        "sql_mode",
    ] {
        assert_eq!(
            state.get_system_var(name),
            Err(SessionVariableError::TransactionOutcome(
                TransactionSettingsError::UnsettledOutcome
            ))
        );
    }
    assert_eq!(
        state.set_system_var("sql_mode", Value::String("ANSI_QUOTES".into())),
        Err(SessionVariableError::TransactionOutcome(
            TransactionSettingsError::UnsettledOutcome
        ))
    );
    assert_eq!(
        state
            .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
            .unwrap_err(),
        TransactionSettingsError::UnsettledOutcome
    );
}

#[test]
fn abandoning_prepared_start_preserves_pending_choices_and_fingerprint() {
    let mut state = SessionState::new(1);
    pending(&mut state);
    let before = state.transaction_settings().unwrap();
    let fingerprint = state.translation_fingerprint().unwrap();
    {
        let stage = state
            .stage_transaction_command(FrontendTransactionCommand::BeginExplicit {
                access: Some(ReadWrite),
            })
            .unwrap();
        assert_eq!(
            stage.next_frontend_boundary(),
            Some(FrontendTransactionBoundary::Started(choices(
                Serializable,
                ReadWrite
            )))
        );
    }
    assert_eq!(state.transaction_settings().unwrap(), before);
    assert_eq!(state.translation_fingerprint().unwrap(), fingerprint);
    assert_eq!(state.unconfirmed_transaction_command(), None);
}

#[test]
fn submitted_start_without_confirmation_is_not_reusable() {
    let mut state = SessionState::new(2);
    pending(&mut state);
    let before = state.transaction_settings().unwrap();
    {
        let mut stage = state
            .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
            .unwrap();
        stage.mark_submitted().unwrap();
    }
    let unknown = state.unconfirmed_transaction_command().unwrap();
    assert_eq!(unknown.before, before);
    assert_eq!(unknown.last_confirmed, before);
    assert_eq!(unknown.confirmed_boundaries, 0);
    assert_unsettled(&mut state);
}

#[test]
fn confirmed_start_is_retained_when_output_outcome_is_unconfirmed() {
    let mut state = SessionState::new(3);
    pending(&mut state);
    {
        let mut stage = state
            .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
            .unwrap();
        stage.mark_submitted().unwrap();
        stage
            .record_confirmed_frontend_boundary(FrontendTransactionBoundary::Started(choices(
                Serializable,
                ReadOnly,
            )))
            .unwrap();
    }
    let unknown = state.unconfirmed_transaction_command().unwrap();
    assert_eq!(unknown.confirmed_boundaries, 1);
    assert_eq!(
        unknown.last_confirmed.active,
        Some(choices(Serializable, ReadOnly))
    );
    assert_eq!(
        unknown.last_confirmed.next,
        NextTransactionCharacteristics::default()
    );
    assert_eq!(unknown.before.active, None);
    assert_eq!(unknown.before.next.isolation, Some(Serializable));
    assert_unsettled(&mut state);
}

#[test]
fn active_start_commits_before_starting_with_changed_defaults() {
    let mut state = SessionState::new(4);
    begin(&mut state);
    settle(
        &mut state,
        FrontendTransactionCommand::Assign(TransactionSettingAssignment::SessionTransaction(
            TransactionCharacteristicUpdate::Both(choices(Serializable, ReadOnly)),
        )),
    );
    let mut stage = state
        .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
        .unwrap();
    stage.mark_submitted().unwrap();
    assert_eq!(
        stage.next_frontend_boundary(),
        Some(FrontendTransactionBoundary::Ended(Commit))
    );
    stage
        .record_confirmed_frontend_boundary(FrontendTransactionBoundary::Ended(Commit))
        .unwrap();
    assert_eq!(
        stage.next_frontend_boundary(),
        Some(FrontendTransactionBoundary::Started(choices(
            Serializable,
            ReadOnly
        )))
    );
    stage
        .record_confirmed_frontend_boundary(FrontendTransactionBoundary::Started(choices(
            Serializable,
            ReadOnly,
        )))
        .unwrap();
    stage.finish_success_with_validated_output().unwrap();
    assert_eq!(
        state.transaction_settings().unwrap().active,
        Some(choices(Serializable, ReadOnly))
    );
}

#[test]
fn two_confirmed_boundaries_still_require_validated_output() {
    let mut state = SessionState::new(5);
    begin(&mut state);
    {
        let mut stage = state
            .stage_transaction_command(FrontendTransactionCommand::Complete {
                completion: Commit,
                chain: true,
            })
            .unwrap();
        stage.mark_submitted().unwrap();
        stage
            .record_confirmed_frontend_boundary(FrontendTransactionBoundary::Ended(Commit))
            .unwrap();
        stage
            .record_confirmed_frontend_boundary(FrontendTransactionBoundary::Started(choices(
                RepeatableRead,
                ReadWrite,
            )))
            .unwrap();
        assert_eq!(stage.next_frontend_boundary(), None);
    }
    let unknown = state.unconfirmed_transaction_command().unwrap();
    assert_eq!(unknown.confirmed_boundaries, 2);
    assert_eq!(unknown.next_boundary, None);
    assert_eq!(
        unknown.last_confirmed.active,
        Some(choices(RepeatableRead, ReadWrite))
    );
    assert_unsettled(&mut state);
}

#[test]
fn completed_prior_commit_does_not_invent_a_following_start() {
    let mut state = SessionState::new(6);
    begin(&mut state);
    {
        let mut stage = state
            .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
            .unwrap();
        stage.mark_submitted().unwrap();
        stage
            .record_confirmed_frontend_boundary(FrontendTransactionBoundary::Ended(Commit))
            .unwrap();
    }
    let unknown = state.unconfirmed_transaction_command().unwrap();
    assert_eq!(
        unknown.before.active,
        Some(choices(RepeatableRead, ReadWrite))
    );
    assert_eq!(unknown.last_confirmed.active, None);
    assert_eq!(
        unknown.next_boundary,
        Some(FrontendTransactionBoundary::Started(choices(
            RepeatableRead,
            ReadWrite
        )))
    );
    assert_unsettled(&mut state);
}

#[test]
fn contradictory_boundary_is_retained_and_cannot_be_overridden() {
    let mut state = SessionState::new(7);
    let wrong = FrontendTransactionBoundary::Started(choices(ReadCommitted, ReadOnly));
    let mut stage = state
        .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
        .unwrap();
    stage.mark_submitted().unwrap();
    assert_eq!(
        stage.record_confirmed_frontend_boundary(wrong),
        Err(TransactionSettingsError::UnexpectedBoundary {
            expected: Some(FrontendTransactionBoundary::Started(choices(
                RepeatableRead,
                ReadWrite
            ))),
            received: wrong
        })
    );
    assert_eq!(
        stage.record_confirmed_frontend_boundary(FrontendTransactionBoundary::Started(choices(
            RepeatableRead,
            ReadWrite
        ))),
        Err(TransactionSettingsError::UnsettledOutcome)
    );
    assert_eq!(
        stage.finish_success_with_validated_output(),
        Err(TransactionSettingsError::UnsettledOutcome)
    );
    assert_eq!(
        state.unconfirmed_transaction_command().unwrap().phase_error,
        Some(TransactionCommandPhaseError::UnexpectedBoundary {
            expected: Some(FrontendTransactionBoundary::Started(choices(
                RepeatableRead,
                ReadWrite
            ))),
            received: wrong
        })
    );
    assert_unsettled(&mut state);
}

#[test]
fn boundary_before_submission_is_not_abandoned_as_effect_free() {
    let mut state = SessionState::new(8);
    let reported = FrontendTransactionBoundary::Started(choices(RepeatableRead, ReadWrite));
    let mut stage = state
        .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
        .unwrap();
    assert_eq!(
        stage.record_confirmed_frontend_boundary(reported),
        Err(TransactionSettingsError::NotSubmitted)
    );
    drop(stage);
    assert_eq!(
        state.unconfirmed_transaction_command().unwrap().phase_error,
        Some(TransactionCommandPhaseError::BoundaryBeforeSubmission(
            reported
        ))
    );
    assert_unsettled(&mut state);
}

#[test]
fn incomplete_success_cannot_settle_submission() {
    let mut state = SessionState::new(9);
    let mut stage = state
        .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
        .unwrap();
    stage.mark_submitted().unwrap();
    assert_eq!(
        stage.finish_success_with_validated_output(),
        Err(TransactionSettingsError::IncompleteBoundaries)
    );
    assert_unsettled(&mut state);
}

#[test]
fn success_before_submission_is_not_effect_free_abandonment() {
    let mut state = SessionState::new(15);
    pending(&mut state);
    let before = state.transaction_settings().unwrap();
    let stage = state
        .stage_transaction_command(FrontendTransactionCommand::BeginExplicit {
            access: Some(ReadWrite),
        })
        .unwrap();
    assert_eq!(
        stage.finish_success_with_validated_output(),
        Err(TransactionSettingsError::NotSubmitted)
    );
    let unknown = state.unconfirmed_transaction_command().unwrap();
    assert_eq!(
        unknown.phase_error,
        Some(TransactionCommandPhaseError::SuccessBeforeSubmission)
    );
    assert_eq!(unknown.before, before);
    assert_eq!(unknown.last_confirmed, before);
    assert_eq!(unknown.confirmed_boundaries, 0);
    assert_eq!(
        unknown.next_boundary,
        Some(FrontendTransactionBoundary::Started(choices(
            Serializable,
            ReadWrite
        )))
    );
    assert_unsettled(&mut state);
}

#[test]
fn repeated_submission_cannot_settle_or_erase_a_confirmed_prior_commit() {
    for confirm_commit in [false, true] {
        let mut state = SessionState::new(13);
        begin(&mut state);
        let before = state.transaction_settings().unwrap();
        let mut stage = state
            .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
            .unwrap();
        stage.mark_submitted().unwrap();
        if confirm_commit {
            stage
                .record_confirmed_frontend_boundary(FrontendTransactionBoundary::Ended(Commit))
                .unwrap();
        }
        assert_eq!(
            stage.mark_submitted(),
            Err(TransactionSettingsError::AlreadySubmitted)
        );
        let expected = stage.next_frontend_boundary().unwrap();
        assert_eq!(
            stage.record_confirmed_frontend_boundary(expected),
            Err(TransactionSettingsError::UnsettledOutcome)
        );
        assert_eq!(
            stage.finish_success_with_validated_output(),
            Err(TransactionSettingsError::UnsettledOutcome)
        );
        let unknown = state.unconfirmed_transaction_command().unwrap();
        assert_eq!(
            unknown.phase_error,
            Some(TransactionCommandPhaseError::RepeatedSubmission)
        );
        assert_eq!(unknown.before, before);
        assert_eq!(unknown.confirmed_boundaries, u8::from(confirm_commit));
        assert_eq!(
            unknown.last_confirmed.active,
            if confirm_commit { None } else { before.active }
        );
        assert_unsettled(&mut state);
    }
}

#[test]
fn unverified_active_autocommit_changes_preserve_confirmed_state() {
    let mut state = SessionState::new(14);
    begin(&mut state);
    let before = state.transaction_settings().unwrap();
    for setting in [AutocommitSetting::Disabled, AutocommitSetting::Enabled] {
        assert_eq!(
            state
                .stage_transaction_command(FrontendTransactionCommand::SetAutocommit(setting))
                .unwrap_err(),
            TransactionSettingsError::UnverifiedAutocommitBoundary
        );
        assert_eq!(state.transaction_settings().unwrap(), before);
        assert_eq!(state.unconfirmed_transaction_command(), None);
    }
}

#[test]
fn autocommit_enable_orders_commit_and_repeated_zero_retains_active() {
    let mut state = SessionState::new(10);
    settle(
        &mut state,
        FrontendTransactionCommand::SetAutocommit(AutocommitSetting::Disabled),
    );
    settle(&mut state, FrontendTransactionCommand::BeginImplicit);
    let before = state.transaction_settings().unwrap();
    settle(
        &mut state,
        FrontendTransactionCommand::SetAutocommit(AutocommitSetting::Disabled),
    );
    assert_eq!(state.transaction_settings().unwrap(), before);
    let mut stage = state
        .stage_transaction_command(FrontendTransactionCommand::SetAutocommit(
            AutocommitSetting::Enabled,
        ))
        .unwrap();
    stage.mark_submitted().unwrap();
    assert_eq!(
        stage.next_frontend_boundary(),
        Some(FrontendTransactionBoundary::Ended(Commit))
    );
    stage
        .record_confirmed_frontend_boundary(FrontendTransactionBoundary::Ended(Commit))
        .unwrap();
    assert_eq!(
        stage.next_frontend_boundary(),
        Some(FrontendTransactionBoundary::AutocommitAssigned(
            AutocommitSetting::Enabled
        ))
    );
    stage
        .record_confirmed_frontend_boundary(FrontendTransactionBoundary::AutocommitAssigned(
            AutocommitSetting::Enabled,
        ))
        .unwrap();
    stage.finish_success_with_validated_output().unwrap();
    assert_eq!(state.transaction_settings().unwrap().active, None);
    assert_eq!(
        state.transaction_settings().unwrap().autocommit,
        AutocommitSetting::Enabled
    );
}

#[test]
fn fingerprints_include_pending_access_active_choices_and_autocommit() {
    let mut state = SessionState::new(11);
    let initial = state.translation_fingerprint().unwrap();
    pending(&mut state);
    let with_pending = state.translation_fingerprint().unwrap();
    assert_ne!(initial, with_pending);
    begin(&mut state);
    let with_active = state.translation_fingerprint().unwrap();
    assert_ne!(with_pending, with_active);
    settle(
        &mut state,
        FrontendTransactionCommand::Assign(TransactionSettingAssignment::SessionTransaction(
            TransactionCharacteristicUpdate::Both(choices(ReadCommitted, ReadWrite)),
        )),
    );
    let different_defaults = state.translation_fingerprint().unwrap();
    assert_eq!(
        different_defaults.transactions.active,
        Some(choices(Serializable, ReadOnly))
    );
    assert_ne!(with_active, different_defaults);
    settle(
        &mut state,
        FrontendTransactionCommand::Complete {
            completion: Commit,
            chain: false,
        },
    );
    let enabled = state.translation_fingerprint().unwrap();
    settle(
        &mut state,
        FrontendTransactionCommand::SetAutocommit(AutocommitSetting::Disabled),
    );
    assert_ne!(enabled, state.translation_fingerprint().unwrap());
}

#[test]
fn charset_null_or_nonstring_does_not_silently_use_profile_identity() {
    let mut state = SessionState::new(12);
    for value in [Value::Null, Value::Bool(true)] {
        state
            .set_system_var("character_set_results", value)
            .unwrap();
        assert_eq!(
            state.translation_fingerprint(),
            Err(SessionVariableError::NonStringTranslationValue(
                "character_set_results".into()
            ))
        );
    }
}
