use darmok_session::{
    SessionState, SessionVariableReader, SqlMode, SqlModes, TransactionSettingsError,
};

#[test]
fn abandoned_set_retains_known_effects_and_blocks_all_setting_authorities() {
    let mut state = SessionState::new(1);
    let before = state.sql_modes().unwrap();
    let modes = SqlModes::empty().with(SqlMode::AnsiQuotes);
    {
        let mut stage = state.stage_command().unwrap();
        stage.apply_sql_modes(modes);
    }
    let pending = state.unconfirmed_command().unwrap();
    assert_eq!(pending.before.sql_modes, before);
    assert_eq!(pending.last_confirmed.sql_modes, modes);
    assert_eq!(
        state.sql_modes(),
        Err(TransactionSettingsError::UnsettledOutcome)
    );
    assert_eq!(
        state.transaction_settings(),
        Err(TransactionSettingsError::UnsettledOutcome)
    );
    assert!(state.get_system_var("sql_mode").is_err());
    assert!(state.translation_fingerprint().is_err());
    assert!(state.set_sql_modes(SqlModes::empty()).is_err());
    assert!(state.stage_command().is_err());
}

#[test]
fn a_known_sql_error_can_settle_a_partial_set_only_after_its_output_contract() {
    let mut state = SessionState::new(1);
    let modes = SqlModes::empty().with(SqlMode::AnsiQuotes);
    let mut stage = state.stage_command().unwrap();
    stage.clear_diagnostics();
    stage.apply_sql_modes(modes);
    stage.record_sql_error(1568, "transaction characteristics cannot change");
    assert_eq!(stage.finish_with_sent_output().unwrap().sql_modes, modes);
    assert_eq!(state.sql_modes().unwrap(), modes);
    assert_eq!(state.error_count(), 1);
    assert_eq!(state.row_count, -1);
    assert!(state.unconfirmed_command().is_none());
}

#[test]
fn nested_transaction_effects_cannot_settle_the_whole_command() {
    use darmok_session::{FrontendTransactionCommand, TransactionSettingsError};
    let mut state = SessionState::new(1);
    {
        let mut command = state.stage_command().unwrap();
        let mut transaction = command
            .stage_transaction(FrontendTransactionCommand::BeginExplicit { access: None })
            .unwrap();
        transaction.mark_submitted().unwrap();
        let started = transaction.next_frontend_boundary().unwrap();
        transaction
            .record_confirmed_frontend_boundary(started)
            .unwrap();
        transaction.finish_confirmed_effects().unwrap();
        assert!(command.settings().unwrap().transactions.active.is_some());
    }
    assert!(state.unconfirmed_transaction_command().is_none());
    assert!(
        state
            .unconfirmed_command()
            .unwrap()
            .last_confirmed
            .transactions
            .active
            .is_some()
    );
    assert_eq!(
        state.transaction_settings(),
        Err(TransactionSettingsError::UnsettledOutcome)
    );
    assert!(state.translation_fingerprint().is_err());
    assert!(state.stage_command().is_err());
}

#[test]
fn a_partial_replacement_retains_both_transaction_and_command_receipts() {
    use darmok_session::{
        FrontendTransactionBoundary, FrontendTransactionCommand, TransactionCompletion,
    };
    let mut state = SessionState::new(1);
    {
        let mut command = state.stage_command().unwrap();
        let mut transaction = command
            .stage_transaction(FrontendTransactionCommand::BeginExplicit { access: None })
            .unwrap();
        transaction.mark_submitted().unwrap();
        transaction
            .record_confirmed_frontend_boundary(transaction.next_frontend_boundary().unwrap())
            .unwrap();
        transaction.finish_confirmed_effects().unwrap();
        command.finish_with_sent_output().unwrap();
    }
    {
        let mut command = state.stage_command().unwrap();
        let mut replacement = command
            .stage_transaction(FrontendTransactionCommand::BeginExplicit { access: None })
            .unwrap();
        replacement.mark_submitted().unwrap();
        replacement
            .record_confirmed_frontend_boundary(FrontendTransactionBoundary::Ended(
                TransactionCompletion::Commit,
            ))
            .unwrap();
    }
    let pending = state.unconfirmed_transaction_command().unwrap();
    assert_eq!(pending.confirmed_boundaries, 1);
    assert!(pending.before.active.is_some());
    assert!(pending.last_confirmed.active.is_none());
    assert!(matches!(
        pending.next_boundary,
        Some(FrontendTransactionBoundary::Started(_))
    ));
    let command = state.unconfirmed_command().unwrap();
    assert!(command.before.transactions.active.is_some());
    assert!(command.last_confirmed.transactions.active.is_none());
    assert!(state.transaction_settings().is_err());
    assert!(state.stage_command().is_err());
}
