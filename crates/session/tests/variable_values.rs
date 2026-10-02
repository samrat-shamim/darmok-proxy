use darmok_session::*;
use darmok_types::value::Value;
use serde_json::Value as JsonValue;
use sqlparser::ast::{Expr, SelectItem, SetExpr, Statement, Value as AstValue};
use sqlparser::mysql_mode::{MySqlModeFlags, parse_mysql_with_mode};

fn oracle() -> JsonValue {
    serde_json::from_str(include_str!(
        "../../../tests/reference/mysql_sql_modes.json"
    ))
    .unwrap()
}

fn settle(state: &mut SessionState, command: FrontendTransactionCommand) {
    let mut stage = state.stage_transaction_command(command).unwrap();
    stage.mark_submitted().unwrap();
    while let Some(boundary) = stage.next_frontend_boundary() {
        stage.record_confirmed_frontend_boundary(boundary).unwrap();
    }
    stage.finish_success_with_validated_output().unwrap();
}

#[test]
fn typed_name_values_match_the_selected_stock_mode_projections() {
    // Manual model actions, not SQL interpretation or execution. Only the
    // selected mode string is compared; global scope, SQL error numbers,
    // warnings, savepoint receipts and query/data behavior are not certified.
    let traces = [
        ("default-mode-set", None, None),
        ("empty-mode-set", Some(""), None),
        (
            "case-and-duplicate-canonicalization",
            Some("no_backslash_escapes,ansi_quotes,ANSI_QUOTES"),
            None,
        ),
        ("ansi-expansion", Some("ANSI"), None),
        ("traditional-expansion", Some("TRADITIONAL"), None),
        (
            "public-mode-vocabulary",
            Some(
                "REAL_AS_FLOAT,PIPES_AS_CONCAT,ANSI_QUOTES,IGNORE_SPACE,ONLY_FULL_GROUP_BY,NO_UNSIGNED_SUBTRACTION,NO_DIR_IN_CREATE,ANSI,NO_AUTO_VALUE_ON_ZERO,NO_BACKSLASH_ESCAPES,STRICT_TRANS_TABLES,STRICT_ALL_TABLES,NO_ZERO_IN_DATE,NO_ZERO_DATE,ALLOW_INVALID_DATES,ERROR_FOR_DIVISION_BY_ZERO,TRADITIONAL,HIGH_NOT_PRECEDENCE,NO_ENGINE_SUBSTITUTION,PAD_CHAR_TO_FULL_LENGTH,TIME_TRUNCATE_FRACTIONAL",
            ),
            None,
        ),
        (
            "leading-mode-space-is-not-erased",
            None,
            Some(" ansi_quotes , no_backslash_escapes "),
        ),
        (
            "session-mode-survives-rollback-and-preserves-savepoint",
            Some("ANSI_QUOTES"),
            None,
        ),
        (
            "unknown-mode-retains-prior-value",
            Some("ANSI_QUOTES"),
            Some("DARMOK_UNDECLARED_MODE"),
        ),
        (
            "obsolete-mode-retains-prior-value",
            Some("ANSI_QUOTES"),
            Some("NO_FIELD_OPTIONS"),
        ),
        (
            "trailing-element-space-is-not-erased",
            None,
            Some("ansi_quotes   ,no_backslash_escapes  "),
        ),
        (
            "final-value-spaces-are-discarded",
            Some("ansi_quotes,no_backslash_escapes  "),
            None,
        ),
        (
            "empty-mode-elements-are-ignored",
            Some(",ansi_quotes,,no_backslash_escapes,, "),
            None,
        ),
        ("blank-whole-value-is-empty", Some("   "), None),
    ];
    let corpus = oracle();
    assert_eq!(traces.len(), corpus["cases"].as_array().unwrap().len());
    for (name, assignment, invalid) in traces {
        let mut state = SessionState::new(1);
        if let Some(names) = assignment {
            state
                .set_sql_modes(SqlModes::parse_names(names).unwrap())
                .unwrap();
        }
        if let Some(names) = invalid {
            assert_eq!(
                SqlModes::parse_names(names),
                Err(SqlModeNamesError::UnknownName),
                "{name}"
            );
        }
        let expected = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let mode = state.sql_modes().unwrap();
        assert_eq!(
            mode.canonical_names(),
            expected["expected_effect"]["mode"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            state.get_system_var("sql_mode").unwrap(),
            Value::String(mode.canonical_names().into()),
            "{name}"
        );
        assert_eq!(
            state.translation_fingerprint().unwrap().sql_modes,
            mode,
            "{name}"
        );
    }
}

#[test]
fn sql_mode_read_parser_projection_and_identity_change_together() {
    let mut state = SessionState::new(2);
    let before = state.translation_fingerprint().unwrap();
    let modes = SqlModes::parse_names("ANSI").unwrap();
    state.set_sql_modes(modes).unwrap();
    assert_eq!(
        state.get_system_var(" SQL_MODE ").unwrap(),
        Value::String(
            "REAL_AS_FLOAT,PIPES_AS_CONCAT,ANSI_QUOTES,IGNORE_SPACE,ONLY_FULL_GROUP_BY,ANSI".into()
        )
    );
    assert_eq!(state.translation_fingerprint().unwrap().sql_modes, modes);
    assert_ne!(state.translation_fingerprint().unwrap(), before);
    let flags = state.sql_modes().unwrap().parser_flags();
    assert!(flags.contains(MySqlModeFlags::IGNORE_SPACE));
    assert!(modes.contains(SqlMode::OnlyFullGroupBy));
    assert!(!flags.contains(MySqlModeFlags::NO_BACKSLASH_ESCAPES));
}

#[test]
fn all_six_parser_flags_are_projected_without_discarding_other_modes() {
    let modes = SqlModes::parse_names("REAL_AS_FLOAT,PIPES_AS_CONCAT,ANSI_QUOTES,IGNORE_SPACE,NO_BACKSLASH_ESCAPES,HIGH_NOT_PRECEDENCE,STRICT_ALL_TABLES,TIME_TRUNCATE_FRACTIONAL").unwrap();
    let flags = modes.parser_flags();
    assert_eq!(
        flags.bits(),
        MySqlModeFlags::REAL_AS_FLOAT
            | MySqlModeFlags::PIPES_AS_CONCAT
            | MySqlModeFlags::ANSI_QUOTES
            | MySqlModeFlags::IGNORE_SPACE
            | MySqlModeFlags::NO_BACKSLASH_ESCAPES
            | MySqlModeFlags::HIGH_NOT_PRECEDENCE
    );
    assert!(modes.contains(SqlMode::StrictAllTables));
    assert!(modes.contains(SqlMode::TimeTruncateFractional));
    assert_eq!(std::mem::size_of::<SqlModes>(), 4);
}

#[test]
fn equivalent_named_lists_have_one_identity_but_composite_names_are_retained() {
    let a = SqlModes::parse_names("ANSI_QUOTES,STRICT_ALL_TABLES").unwrap();
    let b = SqlModes::parse_names("strict_all_tables,ansi_quotes,ANSI_QUOTES").unwrap();
    assert_eq!(a, b);
    let ansi = SqlModes::parse_names("ANSI").unwrap();
    let expanded = SqlModes::parse_names(
        "REAL_AS_FLOAT,PIPES_AS_CONCAT,ANSI_QUOTES,IGNORE_SPACE,ONLY_FULL_GROUP_BY",
    )
    .unwrap();
    assert_eq!(ansi.parser_flags(), expanded.parser_flags());
    assert_ne!(ansi, expanded);
    assert!(ansi.canonical_names().ends_with(",ANSI"));
}

#[test]
fn unknown_names_and_non_name_formats_cannot_construct_a_partial_value() {
    for names in [
        "DARMOK_UNDECLARED_MODE",
        "ANSI_QUOTES,DARMOK_UNDECLARED_MODE",
        "NO_FIELD_OPTIONS",
        "DEFAULT",
        "0",
        " ansi_quotes",
        "ANSI_QUOTES, no_backslash_escapes",
    ] {
        assert_eq!(
            SqlModes::parse_names(names),
            Err(SqlModeNamesError::UnknownName)
        );
    }
    assert_eq!(SqlModes::parse_names(","), Ok(SqlModes::empty()));
    assert_eq!(
        SqlModes::parse_names("ANSI_QUOTES,"),
        Ok(SqlModes::empty().with(SqlMode::AnsiQuotes))
    );
    assert_eq!(
        SqlModes::parse_names("ANSI_QUOTES,,NO_BACKSLASH_ESCAPES"),
        SqlModes::parse_names("ANSI_QUOTES,NO_BACKSLASH_ESCAPES")
    );
}

#[test]
fn parser_consumes_the_typed_mode_projection_for_ordinary_quotes_and_literals() {
    let mut state = SessionState::new(3);
    let extract = |statements: Vec<Statement>| {
        let Statement::Query(query) = statements.into_iter().next().unwrap() else {
            panic!("query required")
        };
        let SetExpr::Select(select) = *query.body else {
            panic!("select required")
        };
        let SelectItem::UnnamedExpr(expr) = select.projection.into_iter().next().unwrap() else {
            panic!("expression required")
        };
        expr
    };
    let default = extract(
        parse_mysql_with_mode(
            "SELECT \"column\"",
            state.sql_modes().unwrap().parser_flags(),
        )
        .unwrap(),
    );
    assert!(
        matches!(default, Expr::Value(value) if value.value == AstValue::DoubleQuotedString("column".into()))
    );
    state
        .set_sql_modes(SqlModes::parse_names("ANSI_QUOTES,NO_BACKSLASH_ESCAPES").unwrap())
        .unwrap();
    let quoted = extract(
        parse_mysql_with_mode(
            "SELECT \"column\"",
            state.sql_modes().unwrap().parser_flags(),
        )
        .unwrap(),
    );
    assert!(matches!(quoted, Expr::Identifier(ident) if ident.value == "column"));
    let literal = extract(
        parse_mysql_with_mode("SELECT 'a\\nb'", state.sql_modes().unwrap().parser_flags()).unwrap(),
    );
    assert!(
        matches!(literal, Expr::Value(value) if value.value == AstValue::SingleQuotedString("a\\nb".into()))
    );
}

#[test]
fn canonical_reads_agree_with_immutable_text_identity() {
    let state = SessionState::new(4);
    let identity = state.translation_fingerprint().unwrap();
    for (name, expected) in [
        ("time_zone", identity.timezone),
        ("collation_connection", identity.collation),
        ("character_set_client", identity.character_set_client),
        (
            "character_set_connection",
            identity.character_set_connection,
        ),
        ("character_set_results", identity.character_set_results),
    ] {
        assert_eq!(
            state.get_system_var(name).unwrap(),
            Value::String(expected.into())
        );
        assert_eq!(
            SessionVariable::from_canonical_name(name)
                .unwrap()
                .write_path(),
            Err(SessionVariableError::SettingNotImplemented(
                SessionVariable::from_canonical_name(name).unwrap()
            ))
        );
    }
}

#[test]
fn server_identity_and_statement_counts_have_no_value_overwrite_path() {
    let mut state = SessionState::new(5);
    state.push_warning(WarningLevel::Note, 1000, "note");
    state.push_warning(WarningLevel::Warning, 1001, "warning");
    state.push_warning(WarningLevel::Error, 1002, "error");
    assert_eq!(
        state.read_variable(SessionVariable::WarningCount).unwrap(),
        Value::UInt(3)
    );
    assert_eq!(
        state.read_variable(SessionVariable::ErrorCount).unwrap(),
        Value::UInt(1)
    );
    assert_eq!(state.warning_stack().len(), 3);
    assert_eq!(state.warning_count_u16(), 3);
    assert_eq!(state.statement_condition_count_u16(), 3);
    assert_eq!(
        state
            .read_variable(SessionVariable::VersionCompileOs)
            .unwrap(),
        Value::String(std::env::consts::OS.into())
    );
    for variable in [
        SessionVariable::Version,
        SessionVariable::VersionComment,
        SessionVariable::VersionCompileOs,
        SessionVariable::WarningCount,
        SessionVariable::ErrorCount,
    ] {
        assert_eq!(
            variable.write_path(),
            Err(SessionVariableError::ReadOnly(variable))
        );
    }
    state.clear_diagnostics();
    assert_eq!(
        state.read_variable(SessionVariable::WarningCount).unwrap(),
        Value::UInt(0)
    );
    assert_eq!(
        state.read_variable(SessionVariable::ErrorCount).unwrap(),
        Value::UInt(0)
    );
    assert_eq!(state.warning_count_u16(), 0);
    assert_eq!(state.statement_condition_count_u16(), 0);
}

#[test]
fn unavailable_values_are_errors_instead_of_absence_or_invented_defaults() {
    let state = SessionState::new(6);
    for variable in [
        SessionVariable::ForeignKeyChecks,
        SessionVariable::DefaultStorageEngine,
        SessionVariable::CharacterSetDatabase,
        SessionVariable::CharacterSetSystem,
        SessionVariable::CollationDatabase,
        SessionVariable::SessionTrackSystemVariables,
        SessionVariable::MaxAllowedPacket,
        SessionVariable::WaitTimeout,
    ] {
        assert_eq!(
            state.read_variable(variable),
            Err(SessionVariableError::ValueNotImplemented(variable))
        );
    }
}

#[test]
fn canonical_lookup_retains_sql_scope_and_does_not_invent_legacy_aliases() {
    for name in ["@@SESSION.sql_mode", "@@sql_mode", "global.sql_mode"] {
        assert_eq!(
            SessionVariable::from_canonical_name(name),
            Err(SessionVariableError::ScopedReference)
        );
    }
    for name in [
        "storage_engine",
        "interactive_wait_timeout",
        "tx_isolation",
        "unknown_system_var",
    ] {
        assert!(matches!(
            SessionVariable::from_canonical_name(name),
            Err(SessionVariableError::UnknownName(_))
        ));
    }
    assert_eq!(
        SessionVariable::from_canonical_name("SQL_MODE")
            .unwrap()
            .write_path()
            .unwrap(),
        VariableWritePath::SqlModes
    );
    assert_eq!(
        SessionVariable::Autocommit.write_path().unwrap(),
        VariableWritePath::TransactionCommand
    );
}

#[test]
fn mode_updates_preserve_next_and_active_transaction_choices() {
    let mut state = SessionState::new(7);
    settle(
        &mut state,
        FrontendTransactionCommand::Assign(TransactionSettingAssignment::NextTransaction(
            TransactionCharacteristicUpdate::Both(TransactionCharacteristics {
                isolation: FrontendIsolation::Serializable,
                access: FrontendTransactionAccess::ReadOnly,
            }),
        )),
    );
    let pending = state.transaction_settings().unwrap();
    state
        .set_sql_modes(SqlModes::parse_names("TRADITIONAL").unwrap())
        .unwrap();
    assert_eq!(state.transaction_settings().unwrap(), pending);
    settle(
        &mut state,
        FrontendTransactionCommand::BeginExplicit { access: None },
    );
    let active = state.transaction_settings().unwrap();
    state
        .set_sql_modes(SqlModes::parse_names("ANSI").unwrap())
        .unwrap();
    assert_eq!(state.transaction_settings().unwrap(), active);
    settle(
        &mut state,
        FrontendTransactionCommand::Complete {
            completion: TransactionCompletion::Rollback,
            chain: false,
        },
    );
    assert_eq!(
        state.sql_modes().unwrap(),
        SqlModes::parse_names("ANSI").unwrap()
    );
}

#[test]
fn unknown_transaction_outcome_blocks_mode_reads_and_local_updates() {
    let mut state = SessionState::new(8);
    {
        let mut stage = state
            .stage_transaction_command(FrontendTransactionCommand::BeginExplicit { access: None })
            .unwrap();
        stage.mark_submitted().unwrap();
    }
    assert_eq!(
        state.sql_modes(),
        Err(TransactionSettingsError::UnsettledOutcome)
    );
    assert_eq!(
        state.set_sql_modes(SqlModes::empty()),
        Err(TransactionSettingsError::UnsettledOutcome)
    );
    assert_eq!(
        state.get_system_var("sql_mode"),
        Err(SessionVariableError::TransactionOutcome(
            TransactionSettingsError::UnsettledOutcome
        ))
    );
}
