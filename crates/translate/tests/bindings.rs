use darmok_translate::{InputKind, ParseLimits, ParsedStatement, parse_statement};
use darmok_types::Value;
use sqlparser::ast::{Expr, Select, SelectItem, SetExpr, Statement, Value as SqlValue};
use sqlparser::mysql_mode::MySqlModeFlags;

fn limits() -> ParseLimits {
    ParseLimits {
        max_sql_bytes: 65536,
        max_parameters: 256,
        max_recursion: 64,
        max_ast_depth: 128,
        max_ast_nodes: 65536,
    }
}

fn prepared(sql: &str) -> ParsedStatement {
    parse_statement(sql, InputKind::Prepared, MySqlModeFlags::empty(), limits()).unwrap()
}

#[test]
fn offset_comma_limit_preserves_source_parameter_values() {
    let parsed = prepared("SELECT `Id` FROM `Records` WHERE `Id` >= ? LIMIT ?, ?");
    let emitted = parsed.emit_postgres().unwrap();
    assert_eq!(parsed.client_parameter_count(), 3);
    assert_eq!(
        emitted.sql(),
        "SELECT \"Id\" FROM \"Records\" WHERE \"Id\" >= $1 LIMIT $3 OFFSET $2"
    );
    let values = [Value::Int(20), Value::Int(1), Value::Int(2)];
    assert_eq!(
        emitted.bind(&values).unwrap(),
        vec![&values[0], &values[1], &values[2]]
    );
}

#[test]
fn ctes_subqueries_and_limit_keep_source_bindings() {
    let parsed =
        prepared("WITH c AS (SELECT ? AS a) SELECT ?, (SELECT ?) FROM c WHERE a = ? LIMIT ?, ?");
    let emitted = parsed.emit_postgres().unwrap();
    assert_eq!(
        emitted.sql(),
        r#"WITH "c" AS (SELECT $1 AS "a") SELECT $2, (SELECT $3) FROM "c" WHERE "a" = $4 LIMIT $6 OFFSET $5"#
    );
    assert_eq!(emitted.parameter_order(), &[0, 1, 2, 3, 4, 5]);
}

#[test]
fn literal_and_comment_markers_are_never_parameters() {
    let parsed = prepared("SELECT '?', '$999', `?`, ? /* ? */ -- ?\n");
    assert_eq!(parsed.client_parameter_count(), 1);
    let emitted = parsed.emit_postgres().unwrap();
    assert_eq!(emitted.sql(), "SELECT '?', '$999', \"?\", $1");
    assert_eq!(emitted.parameter_count(), 1);
}

#[test]
fn eliminated_and_duplicated_parameters_have_explicit_dense_backend_layouts() {
    let mut parsed = prepared("SELECT ?, ?");
    let select = select_mut(&mut parsed);
    select.projection.remove(0);
    select.projection.push(select.projection[0].clone());
    let emitted = parsed.emit_postgres().unwrap();
    assert_eq!(emitted.sql(), "SELECT $1, $1");
    assert_eq!(emitted.parameter_order(), &[1]);
    assert_eq!(emitted.parameter_count(), 1);
    let values = [Value::Int(5), Value::Int(7)];
    assert_eq!(emitted.bind(&values).unwrap(), vec![&values[1]]);
    assert!(emitted.bind(&values[1..]).is_err());
    assert!(emitted.bind(&[]).is_err());
}

#[test]
fn rewriters_cannot_introduce_unknown_or_ambiguous_parameter_identities() {
    for placeholder in ["?", "$0", "$2", "$65536", "$01", "$+1", ":name"] {
        let mut parsed = prepared("SELECT ?");
        select_mut(&mut parsed).projection[0] = SelectItem::UnnamedExpr(Expr::Value(
            SqlValue::Placeholder(placeholder.into()).with_empty_span(),
        ));
        assert!(parsed.emit_postgres().is_err(), "accepted {placeholder}");
    }
}

#[test]
fn sql_modes_control_tokenization_before_parameters_are_numbered() {
    let sql = r#"SELECT "?", ?"#;
    let default = prepared(sql).emit_postgres().unwrap();
    assert_eq!(default.sql(), "SELECT '?', $1");
    let ansi = parse_statement(
        sql,
        InputKind::Prepared,
        MySqlModeFlags::from_bits(MySqlModeFlags::ANSI_QUOTES),
        limits(),
    )
    .unwrap()
    .emit_postgres()
    .unwrap();
    assert_eq!(ansi.sql(), "SELECT \"?\", $1");
    let sql = r"SELECT 'path\', ?";
    assert!(parse_statement(sql, InputKind::Prepared, MySqlModeFlags::empty(), limits()).is_err());
    let no_escapes = parse_statement(
        sql,
        InputKind::Prepared,
        MySqlModeFlags::from_bits(MySqlModeFlags::NO_BACKSLASH_ESCAPES),
        limits(),
    )
    .unwrap();
    assert_eq!(no_escapes.client_parameter_count(), 1);
}

#[test]
fn text_parameters_batches_and_executable_comments_fail_explicitly() {
    assert!(
        parse_statement(
            "SELECT ?",
            InputKind::Text,
            MySqlModeFlags::empty(),
            limits()
        )
        .is_err()
    );
    for sql in [
        "",
        "-- comment",
        "SELECT 1; SELECT 2",
        "/*!80000 SELECT 1 */",
        "SELECT 1 /*! + 1 */",
    ] {
        assert!(
            parse_statement(sql, InputKind::Prepared, MySqlModeFlags::empty(), limits()).is_err(),
            "accepted {sql}"
        );
    }
    assert!(
        parse_statement(
            "SELECT 1; -- ordinary comment",
            InputKind::Text,
            MySqlModeFlags::empty(),
            limits()
        )
        .is_ok()
    );
}

#[test]
fn limits_fail_before_silent_truncation_or_parameter_overflow() {
    assert!(
        parse_statement(
            "SELECT 1",
            InputKind::Text,
            MySqlModeFlags::empty(),
            ParseLimits {
                max_sql_bytes: 3,
                ..limits()
            }
        )
        .is_err()
    );
    assert!(
        parse_statement(
            "SELECT ?, ?",
            InputKind::Prepared,
            MySqlModeFlags::empty(),
            ParseLimits {
                max_parameters: 1,
                ..limits()
            }
        )
        .is_err()
    );
    assert!(
        parse_statement(
            "SELECT (((((1)))))",
            InputKind::Text,
            MySqlModeFlags::empty(),
            ParseLimits {
                max_recursion: 1,
                ..limits()
            }
        )
        .is_err()
    );
    assert!(
        parse_statement(
            "SELECT 1",
            InputKind::Text,
            MySqlModeFlags::from_bits(1 << 31),
            limits()
        )
        .is_err()
    );
}

#[test]
fn default_diagnostics_never_include_sql_literals() {
    let parsed = prepared("SELECT 'private-literal'");
    assert!(!format!("{parsed:?}").contains("private-literal"));
    let emitted = parsed.emit_postgres().unwrap();
    assert!(!format!("{emitted:?}").contains("private-literal"));
    let error = parse_statement(
        "SELECT 'private-literal' INVALID SQL",
        InputKind::Text,
        MySqlModeFlags::empty(),
        limits(),
    )
    .unwrap_err();
    assert!(!format!("{error:?}").contains("private-literal"));
}

#[test]
fn mysql_dollar_identifiers_cannot_turn_into_postgres_parameter_references() {
    let parsed = prepared("SELECT $1, $foo, ? FROM t");
    let emitted = parsed.emit_postgres().unwrap();
    assert_eq!(emitted.sql(), r#"SELECT "$1", "$foo", $1 FROM "t""#);
    assert_eq!(emitted.parameter_order(), &[0]);
    let mut parsed = prepared("SELECT ?");
    select_mut(&mut parsed)
        .projection
        .push(SelectItem::UnnamedExpr(Expr::Identifier(
            sqlparser::ast::Ident::new("$2"),
        )));
    assert_eq!(parsed.emit_postgres().unwrap().sql(), "SELECT $1, \"$2\"");
}

#[test]
fn residual_validation_visits_nested_statements() {
    for sql in [
        "EXPLAIN INSERT IGNORE INTO t VALUES (?)",
        "EXPLAIN CREATE TABLE t (x INT) ENGINE=InnoDB",
        "EXPLAIN REPLACE INTO t VALUES (?)",
    ] {
        assert!(prepared(sql).emit_postgres().is_err(), "accepted {sql}");
    }
}

fn select_mut(parsed: &mut ParsedStatement) -> &mut Select {
    let Statement::Query(query) = parsed.ast_mut() else {
        panic!("query expected");
    };
    let SetExpr::Select(select) = query.body.as_mut() else {
        panic!("select expected");
    };
    select
}

#[test]
fn raw_ast_values_cannot_bypass_parameter_identity_validation() {
    let mut parsed = prepared("CREATE TABLE t (x INT)");
    let Statement::CreateTable(create) = parsed.ast_mut() else {
        panic!("create expected");
    };
    create.clustered_by = Some(sqlparser::ast::ClusteredBy {
        columns: vec![sqlparser::ast::Ident::new("x")],
        sorted_by: None,
        num_buckets: SqlValue::Placeholder("$999".into()),
    });
    let error = parsed.emit_postgres().unwrap_err();
    assert!(error.to_string().contains("binding_identity"));
}

#[test]
fn reserved_function_syntax_and_keyword_column_identifiers_are_distinct() {
    let emitted = prepared("SELECT CURRENT_DATE, CURRENT_TIMESTAMP, user, COUNT(user) FROM t")
        .emit_postgres()
        .unwrap();
    assert_eq!(
        emitted.sql(),
        r#"SELECT CURRENT_DATE, CURRENT_TIMESTAMP, "user", COUNT("user") FROM "t""#
    );
}
