//! Parse and classify the finite stock-name fixture in its recorded mode.
//! No SQL is submitted and no inter-statement mode application is implemented.

use std::ops::ControlFlow;

use darmok_session::{
    SessionInputError, SessionVariable, SqlModes, SystemVariableForm,
    classify_mysql_system_variable_read, mysql_system_variable_assignments,
};
use serde_json::Value;
use sqlparser::ast::{
    ContextModifier, Expr, Set, SetAssignmentTarget, Spanned, Statement, Visit, VisitMut, Visitor,
    VisitorMut,
};
use sqlparser::mysql_mode::{MySqlModeFlags, parse_mysql_with_mode};
use sqlparser::tokenizer::Location;

type NameFact = (SystemVariableForm, Option<char>);

#[derive(Default)]
struct Reads {
    variables: Vec<NameFact>,
    quoted_columns: usize,
}

impl Visitor for Reads {
    type Break = ();

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        match expr {
            Expr::MySqlSystemVariable(input) => {
                let fact = classify_mysql_system_variable_read(expr).unwrap();
                assert_eq!(fact.variable, SessionVariable::SqlMode);
                assert!(std::ptr::eq(fact.expression, expr));
                assert_eq!(input.name.value.to_ascii_lowercase(), "sql_mode");
                assert!(input.prefix.is_none());
                self.variables.push((fact.form, input.name.quote_style));
            }
            Expr::Identifier(name) if name.value == "@@sql_mode" => {
                assert!(name.quote_style.is_some());
                assert_eq!(
                    classify_mysql_system_variable_read(expr),
                    Err(SessionInputError::NotSystemVariableRead)
                );
                self.quoted_columns += 1;
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

fn qualified(scope: ContextModifier, quote: Option<char>) -> NameFact {
    (SystemVariableForm::Qualified(scope), quote)
}

fn expected_reads(name: &str) -> Vec<NameFact> {
    match name {
        "unqualified-backtick-read" | "backtick-name-case-insensitivity" => {
            vec![(SystemVariableForm::UnqualifiedAt, Some('`'))]
        }
        "qualified-backtick-reads" => [
            ContextModifier::Session,
            ContextModifier::Local,
            ContextModifier::Global,
        ]
        .map(|scope| qualified(scope, Some('`')))
        .into(),
        "ansi-qualified-double-quoted-identifier-reads" => [
            ContextModifier::Session,
            ContextModifier::Local,
            ContextModifier::Global,
        ]
        .map(|scope| qualified(scope, Some('"')))
        .into(),
        "space-around-qualified-read-dot" => vec![qualified(ContextModifier::Session, Some('`'))],
        "session-single-quoted-text-read" => vec![qualified(ContextModifier::Session, Some('\''))],
        "session-double-quoted-text-read" => vec![qualified(ContextModifier::Session, Some('"'))],
        "ordinary-unquoted-control"
        | "backtick-column-lookalike"
        | "ansi-double-quoted-column-lookalike" => {
            vec![(SystemVariableForm::UnqualifiedAt, None)]
        }
        "unqualified-backtick-assignment"
        | "session-backtick-assignment"
        | "local-backtick-assignment"
        | "bare-backtick-assignment"
        | "keyword-session-backtick-assignment"
        | "keyword-local-backtick-assignment"
        | "ansi-bare-double-quoted-assignment"
        | "ansi-session-double-quoted-assignment"
        | "space-around-qualified-assignment-dot" => {
            vec![qualified(ContextModifier::Session, None)]
        }
        _ => panic!("fixture name needs an explicit input contract: {name}"),
    }
}

fn expected_targets(name: &str) -> Vec<NameFact> {
    match name {
        "unqualified-backtick-assignment" | "backtick-name-case-insensitivity" => {
            vec![(SystemVariableForm::UnqualifiedAt, Some('`'))]
        }
        "session-backtick-assignment" | "space-around-qualified-assignment-dot" => {
            vec![qualified(ContextModifier::Session, Some('`'))]
        }
        "local-backtick-assignment" => vec![qualified(ContextModifier::Local, Some('`'))],
        "bare-backtick-assignment"
        | "keyword-session-backtick-assignment"
        | "keyword-local-backtick-assignment" => {
            vec![(SystemVariableForm::Bare, Some('`'))]
        }
        "ansi-bare-double-quoted-assignment" => vec![
            (SystemVariableForm::Bare, None),
            (SystemVariableForm::Bare, Some('"')),
        ],
        "ansi-session-double-quoted-assignment" => vec![
            (SystemVariableForm::Bare, None),
            qualified(ContextModifier::Session, Some('"')),
        ],
        "ordinary-unquoted-control" => vec![(SystemVariableForm::UnqualifiedAt, None)],
        "unqualified-backtick-read"
        | "qualified-backtick-reads"
        | "ansi-qualified-double-quoted-identifier-reads"
        | "backtick-column-lookalike"
        | "space-around-qualified-read-dot"
        | "ansi-double-quoted-column-lookalike"
        | "session-single-quoted-text-read"
        | "session-double-quoted-text-read" => {
            vec![(SystemVariableForm::Bare, None)]
        }
        _ => panic!("fixture name needs an explicit target contract: {name}"),
    }
}

fn round_trip(statements: &[Statement], flags: MySqlModeFlags) {
    for statement in statements {
        let formatted = statement.to_string();
        let reparsed = parse_mysql_with_mode(&formatted, flags).unwrap();
        assert_eq!(
            reparsed.as_slice(),
            std::slice::from_ref(statement),
            "{formatted}"
        );
    }
}

#[test]
fn stock_names_retain_variable_forms_quoting_columns_and_borrowed_targets() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/reference/mysql_system_variable_names.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 19);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        assert_eq!(case["expected_errors"], serde_json::json!([]));
        let effect = case["expected_effect"].as_object().unwrap();
        // The fixture explicitly records the final session value under one of
        // these two keys. This selects a grammar context, never evaluates SQL.
        let mode = match (effect.get("mode"), effect.get("session")) {
            (Some(Value::String(mode)), None) | (None, Some(Value::String(mode))) => mode,
            _ => panic!("fixture must record one session-mode value: {name}"),
        };
        let flags = SqlModes::parse_names(mode).unwrap().parser_flags();
        let settings = parse_mysql_with_mode(case["sql"].as_str().unwrap(), flags).unwrap();
        let mut targets = Vec::new();
        for statement in &settings {
            let Statement::Set(input) = statement else {
                panic!("fixture setting expected")
            };
            for fact in mysql_system_variable_assignments(input).unwrap() {
                let fact = fact.unwrap();
                assert_eq!(fact.variable, SessionVariable::SqlMode);
                let Set::SingleAssignment {
                    variable, values, ..
                } = input
                else {
                    panic!("fixture uses single targets")
                };
                assert!(std::ptr::eq(fact.name, variable));
                assert!(std::ptr::eq(fact.value, &values[0]));
                let quote = match fact.name {
                    SetAssignmentTarget::ObjectName(name) => {
                        name.0[0].as_ident().unwrap().quote_style
                    }
                    SetAssignmentTarget::MySqlSystemVariable(name) => name.name.quote_style,
                };
                targets.push((fact.form, quote));
            }
        }
        assert_eq!(targets, expected_targets(name), "{name}");
        round_trip(&settings, flags);
        let output = parse_mysql_with_mode(case["effect_sql"].as_str().unwrap(), flags).unwrap();
        let mut reads = Reads::default();
        assert_eq!(Visit::visit(&output, &mut reads), ControlFlow::Continue(()));
        assert_eq!(reads.variables, expected_reads(name), "{name}");
        let columns = usize::from(
            name == "backtick-column-lookalike" || name == "ansi-double-quoted-column-lookalike",
        );
        assert_eq!(reads.quoted_columns, columns, "{name}");
        round_trip(&output, flags);
    }
}

#[test]
fn sigil_scope_and_name_share_a_full_expression_span() {
    let statements =
        parse_mysql_with_mode("SELECT @@SESSION . `sql_mode`", MySqlModeFlags::empty()).unwrap();
    struct SpanCheck(bool);
    impl Visitor for SpanCheck {
        type Break = ();
        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
            if let Expr::MySqlSystemVariable(input) = expr {
                self.0 = true;
                assert_eq!(input.sigil_span.start, Location::new(1, 8));
                assert_eq!(input.name.span.start, Location::new(1, 20));
                assert_eq!(expr.span().start, input.sigil_span.start);
                assert_eq!(expr.span().end, input.name.span.end);
            }
            ControlFlow::Continue(())
        }
    }
    let mut check = SpanCheck(false);
    assert_eq!(
        Visit::visit(&statements, &mut check),
        ControlFlow::Continue(())
    );
    assert!(check.0);
    round_trip(&statements, MySqlModeFlags::empty());
}

#[test]
fn mutable_visitors_can_change_the_variable_without_changing_a_quoted_column() {
    struct Rename;
    impl VisitorMut for Rename {
        type Break = ();
        fn pre_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<()> {
            if let Expr::MySqlSystemVariable(input) = expr {
                input.name.value = "autocommit".into();
            }
            ControlFlow::Continue(())
        }
    }
    let mut statements =
        parse_mysql_with_mode("SELECT @@`sql_mode`, `@@sql_mode`", MySqlModeFlags::empty())
            .unwrap();
    assert_eq!(
        VisitMut::visit(&mut statements, &mut Rename),
        ControlFlow::Continue(())
    );
    assert_eq!(
        statements[0].to_string(),
        "SELECT @@`autocommit`, `@@sql_mode`"
    );
    round_trip(&statements, MySqlModeFlags::empty());
}
