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
