//! Ordinary parsed input and trusted in-memory setting boundaries. No native
//! execution, expression coercion, output metadata or SQL admission is tested.

use darmok_session::*;
use sqlparser::ast::{ContextModifier, Expr, SelectItem, Set, SetExpr, SetTransaction, Statement};
use sqlparser::mysql_mode::{MySqlModeFlags, parse_mysql_with_mode};

fn set(sql: &str) -> Set {
    let mut statements = parse_mysql_with_mode(sql, MySqlModeFlags::empty()).unwrap();
    assert_eq!(statements.len(), 1);
    let Statement::Set(input) = statements.remove(0) else {
        panic!("expected SET");
    };
    input
}

fn transaction(sql: &str) -> TransactionSettingAssignment {
    let Set::SetTransaction(input) = set(sql) else {
        panic!("expected transaction setting");
    };
    classify_mysql_transaction_setting(&input).unwrap()
}

fn read(sql: &str) -> Expr {
    let mut statements = parse_mysql_with_mode(sql, MySqlModeFlags::empty()).unwrap();
    let Statement::Query(query) = statements.remove(0) else {
        panic!("expected query");
    };
    let SetExpr::Select(select) = *query.body else {
        panic!("expected SELECT");
    };
    let SelectItem::UnnamedExpr(expression) = select.projection.into_iter().next().unwrap() else {
        panic!("expected expression");
    };
    expression
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
fn parsed_transaction_updates_preserve_the_selected_characteristics_and_scope() {
    for (label, isolation) in [
        ("READ UNCOMMITTED", FrontendIsolation::ReadUncommitted),
        ("READ COMMITTED", FrontendIsolation::ReadCommitted),
        ("REPEATABLE READ", FrontendIsolation::RepeatableRead),
        ("SERIALIZABLE", FrontendIsolation::Serializable),
    ] {
        for (label_access, access) in [
            ("READ WRITE", FrontendTransactionAccess::ReadWrite),
            ("READ ONLY", FrontendTransactionAccess::ReadOnly),
        ] {
            let choices = TransactionCharacteristics { isolation, access };
            let expected = TransactionCharacteristicUpdate::Both(choices);
            assert_eq!(
                transaction(&format!(
                    "SET SESSION TRANSACTION ISOLATION LEVEL {label}, {label_access}"
                )),
                TransactionSettingAssignment::SessionTransaction(expected)
            );
            assert_eq!(
                transaction(&format!(
                    "SET TRANSACTION {label_access}, ISOLATION LEVEL {label}"
                )),
                TransactionSettingAssignment::NextTransaction(expected)
            );
        }
    }
}

#[test]
fn parsed_named_updates_keep_pending_and_active_choices_separate() {
    let mut state = SessionState::new(1);
    settle(
        &mut state,
        FrontendTransactionCommand::Assign(transaction(
            "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE, READ ONLY",
        )),
    );
    settle(
        &mut state,
        FrontendTransactionCommand::Assign(transaction(
            "SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED",
        )),
    );
    let snapshot = state.transaction_settings().unwrap();
    assert_eq!(
        snapshot.defaults.isolation,
        FrontendIsolation::ReadCommitted
    );
    assert_eq!(snapshot.next.isolation, None);
    assert_eq!(
        snapshot.next.access,
        Some(FrontendTransactionAccess::ReadOnly)
    );
    settle(
        &mut state,
        FrontendTransactionCommand::BeginExplicit { access: None },
    );
    settle(
        &mut state,
        FrontendTransactionCommand::Assign(transaction(
            "SET SESSION TRANSACTION READ WRITE, ISOLATION LEVEL REPEATABLE READ",
        )),
    );
    let snapshot = state.transaction_settings().unwrap();
    assert_eq!(
        snapshot.active,
        Some(TransactionCharacteristics {
            isolation: FrontendIsolation::ReadCommitted,
            access: FrontendTransactionAccess::ReadOnly,
        })
    );
    assert_eq!(
        snapshot.defaults,
        TransactionCharacteristics {
            isolation: FrontendIsolation::RepeatableRead,
            access: FrontendTransactionAccess::ReadWrite,
        }
    );
    assert_eq!(snapshot.next, NextTransactionCharacteristics::default());
}

#[test]
fn input_classification_does_not_publish_a_setting_boundary() {
    let mut state = SessionState::new(1);
    let before = state.transaction_settings().unwrap();
    let assignment = transaction("SET SESSION TRANSACTION READ ONLY");
    assert_eq!(state.transaction_settings().unwrap(), before);
    let stage = state
        .stage_transaction_command(FrontendTransactionCommand::Assign(assignment))
        .unwrap();
    assert_eq!(
        stage.next_frontend_boundary(),
        Some(FrontendTransactionBoundary::SettingsAssigned(assignment))
    );
    drop(stage); // Unsubmitted classification does not change confirmed choices.
    assert_eq!(state.transaction_settings().unwrap(), before);
}

#[test]
fn a_parsed_next_setting_inside_active_preserves_confirmed_choices() {
    let mut state = SessionState::new(1);
    settle(
        &mut state,
        FrontendTransactionCommand::BeginExplicit { access: None },
    );
    let before = state.transaction_settings().unwrap();
    let assignment = transaction("SET TRANSACTION READ ONLY");
    assert!(matches!(
        state.stage_transaction_command(FrontendTransactionCommand::Assign(assignment)),
        Err(TransactionSettingsError::NextChoicesDuringActive)
    ));
    assert_eq!(state.transaction_settings().unwrap(), before);
}

#[test]
fn additional_transaction_scopes_and_source_forms_are_explicit_inputs() {
    for scope in [ContextModifier::Global] {
        let sql = format!("SET {scope}TRANSACTION READ ONLY");
        let Set::SetTransaction(input) = set(&sql) else {
            panic!("expected transaction setting")
        };
        assert_eq!(
            classify_mysql_transaction_setting(&input),
            Err(SessionInputError::UnsupportedTransactionScope(scope))
        );
        assert_eq!(input.to_string(), sql);
    }
    assert_eq!(
        transaction("SET LOCAL TRANSACTION READ ONLY"),
        transaction("SET SESSION TRANSACTION READ ONLY")
    );
    let Set::SetTransaction(input) = set("SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY")
    else {
        panic!("expected characteristics form")
    };
    assert!(matches!(
        input,
        SetTransaction::Characteristics {
            scope: Some(ContextModifier::Session),
            ..
        }
    ));
    assert_eq!(
        classify_mysql_transaction_setting(&input),
        Err(SessionInputError::UnsupportedTransactionSyntax)
    );
    let Set::SetTransaction(input) = set("SET TRANSACTION SNAPSHOT '000003A1-1'") else {
        panic!("expected snapshot form")
    };
    assert!(matches!(
        input,
        SetTransaction::Snapshot { scope: None, .. }
    ));
    assert_eq!(
        classify_mysql_transaction_setting(&input),
        Err(SessionInputError::UnsupportedTransactionSyntax)
    );
}

#[test]
fn keyword_context_persists_while_at_qualifiers_remain_local_to_the_name() {
    let input = set(
        "SET SESSION sql_mode='ANSI_QUOTES', @@LOCAL.transaction_isolation='READ-COMMITTED', autocommit=1, LOCAL time_zone='+00:00', transaction_read_only=0",
    );
    let facts = mysql_system_variable_assignments(&input)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(facts.len(), 5);
    assert_eq!(
        facts.iter().map(|fact| fact.variable).collect::<Vec<_>>(),
        vec![
            SessionVariable::SqlMode,
            SessionVariable::TransactionIsolation,
            SessionVariable::Autocommit,
            SessionVariable::TimeZone,
            SessionVariable::TransactionReadOnly,
        ]
    );
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.explicit_keyword_scope)
            .collect::<Vec<_>>(),
        vec![
            Some(ContextModifier::Session),
            None,
            None,
            Some(ContextModifier::Local),
            None,
        ]
    );
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.preceding_keyword_scope)
            .collect::<Vec<_>>(),
        vec![
            None,
            Some(ContextModifier::Session),
            Some(ContextModifier::Session),
            Some(ContextModifier::Session),
            Some(ContextModifier::Local),
        ]
    );
    assert_eq!(
        facts[1].form,
        SystemVariableForm::Qualified(ContextModifier::Local)
    );
    assert_eq!(facts[2].keyword_context(), Some(ContextModifier::Session));
    assert_eq!(facts[4].keyword_context(), Some(ContextModifier::Local));
}

#[test]
fn ambiguous_compound_transaction_input_retains_both_syntax_facts() {
    let input = set(
        "SET SESSION sql_mode='', @@transaction_isolation='READ-COMMITTED', transaction_read_only=1",
    );
    let facts = mysql_system_variable_assignments(&input)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(facts[1].form, SystemVariableForm::UnqualifiedAt);
    assert_eq!(facts[1].keyword_context(), Some(ContextModifier::Session));
    assert_eq!(facts[2].form, SystemVariableForm::Bare);
    assert_eq!(facts[2].keyword_context(), Some(ContextModifier::Session));
    // The input view deliberately supplies no effective transaction target.
}

#[test]
fn assignment_expressions_and_names_are_borrowed_without_coercion() {
    for sql in [
        "SET sql_mode=CONCAT(@@sql_mode, ',ANSI_QUOTES')",
        "SET @@SESSION.sql_mode=DEFAULT",
        "SET LOCAL transaction_read_only=OFF",
    ] {
        let input = set(sql);
        let Set::SingleAssignment {
            variable, values, ..
        } = &input
        else {
            panic!("expected single assignment")
        };
        let fact = mysql_system_variable_assignments(&input)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert!(std::ptr::eq(fact.name, variable));
        assert!(std::ptr::eq(fact.value, &values[0]));
    }
}

#[test]
fn scoped_reads_keep_registry_identity_without_selecting_model_values() {
    for variable in SessionVariable::ALL {
        for (prefix, form) in [
            ("@@", SystemVariableForm::UnqualifiedAt),
            (
                "@@SESSION.",
                SystemVariableForm::Qualified(ContextModifier::Session),
            ),
            (
                "@@LOCAL.",
                SystemVariableForm::Qualified(ContextModifier::Local),
            ),
            (
                "@@GLOBAL.",
                SystemVariableForm::Qualified(ContextModifier::Global),
            ),
        ] {
            let expression = read(&format!(
                "SELECT {prefix}{}",
                variable.name().to_ascii_uppercase()
            ));
            let fact = classify_mysql_system_variable_read(&expression).unwrap();
            assert_eq!((fact.variable, fact.form), (variable, form));
            assert!(std::ptr::eq(fact.expression, &expression));
        }
    }
    for sql in [
        "SELECT sql_mode",
        "SELECT `sql_mode`",
        "SELECT `@@sql_mode`",
        "SELECT amount",
        "SELECT items.amount",
    ] {
        assert_eq!(
            classify_mysql_system_variable_read(&read(sql)),
            Err(SessionInputError::NotSystemVariableRead)
        );
    }
}

#[test]
fn an_ordinary_unimplemented_variable_stops_compound_classification() {
    let state = SessionState::new(1);
    let before = state.transaction_settings().unwrap();
    let input = set("SET sql_mode='', optimizer_switch='index_merge=on', autocommit=0");
    let mut facts = mysql_system_variable_assignments(&input).unwrap();
    assert_eq!(
        facts.next().unwrap().unwrap().variable,
        SessionVariable::SqlMode
    );
    assert_eq!(facts.next(), Some(Err(SessionInputError::UnknownVariable)));
    assert_eq!(facts.next(), None);
    assert_eq!(facts.next(), None);
    assert_eq!(state.transaction_settings().unwrap(), before);
    assert!(matches!(
        mysql_system_variable_assignments(&set("SET NAMES utf8mb4")),
        Err(SessionInputError::UnsupportedVariableSet)
    ));
}
