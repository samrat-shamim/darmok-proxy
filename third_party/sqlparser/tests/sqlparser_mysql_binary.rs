// Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0
//! MySQL binary digit grammar, identity and foreign-dialect preservation.

use sqlparser::{
    ast::{Expr, SelectItem, Value},
    dialect::{BigQueryDialect, GenericDialect, MySqlDialect, PostgreSqlDialect},
    mysql_mode::MySqlModeFlags,
    parser::Parser,
    source::parse_mysql_source,
    tokenizer::{Token, Tokenizer},
};

#[test]
fn mysql_binary_digits_keep_distinct_ast_types_and_original_spelling() {
    for flags in [
        MySqlModeFlags::empty(),
        MySqlModeFlags::from_bits(MySqlModeFlags::ANSI_QUOTES),
        MySqlModeFlags::from_bits(MySqlModeFlags::NO_BACKSLASH_ESCAPES),
        MySqlModeFlags::from_bits(
            MySqlModeFlags::ANSI_QUOTES | MySqlModeFlags::NO_BACKSLASH_ESCAPES,
        ),
    ] {
        let parsed =
            parse_mysql_source("SELECT X'', x'00aF', 0xF, b'', B'001', 0b001", flags).unwrap();
        let source = parsed.single_select().unwrap().unwrap();
        let expected = [
            ("X''", Value::HexStringLiteral("".into())),
            ("x'00aF'", Value::HexStringLiteral("00aF".into())),
            ("0xF", Value::HexStringLiteral("F".into())),
            ("b''", Value::BitStringLiteral("".into())),
            ("B'001'", Value::BitStringLiteral("001".into())),
            ("0b001", Value::BitStringLiteral("001".into())),
        ];
        for (index, (spelling, value)) in expected.iter().enumerate() {
            let SelectItem::UnnamedExpr(Expr::Value(actual)) = &source.select().projection[index]
            else {
                panic!("expected a literal");
            };
            assert_eq!(&actual.value, value);
            assert_eq!(source.item_source(index).unwrap(), *spelling);
        }
    }
}

#[test]
fn mysql_quoted_digit_rules_do_not_use_string_escape_rules() {
    for flags in [
        MySqlModeFlags::empty(),
        MySqlModeFlags::from_bits(MySqlModeFlags::NO_BACKSLASH_ESCAPES),
    ] {
        for sql in [
            "SELECT X'F'",
            "SELECT X'0G'",
            "SELECT X'00 01'",
            r"SELECT X'4\1'",
            "SELECT b'02'",
            "SELECT b'0 1'",
            r"SELECT b'0\1'",
        ] {
            assert!(parse_mysql_source(sql, flags).is_err(), "{sql}");
        }
    }
}

#[test]
fn mysql_unquoted_digit_identifiers_are_not_split_into_a_literal_and_alias() {
    for word in [
        "0x",
        "0b",
        "0X41",
        "0B01",
        "0x41g",
        "0b012",
        "0x41_name",
        "0b01_name",
        "0x41é",
        "0b01é",
    ] {
        let tokens = Tokenizer::new(&MySqlDialect {}, word).tokenize().unwrap();
        assert!(
            matches!(tokens.as_slice(), [Token::Word(actual)] if actual.value == word && actual.quote_style.is_none()),
            "{word}"
        );
        let sql = format!("SELECT {word} AS value");
        let parsed = parse_mysql_source(&sql, MySqlModeFlags::empty());
        assert!(parsed.is_ok(), "{word}");
    }
}

#[test]
fn mysql_binary_introducers_accept_both_literal_types() {
    let parsed = parse_mysql_source(
        "SELECT _binary X'41', _BiNaRy b'01', _binary 0x041, _binary 0b001",
        MySqlModeFlags::empty(),
    )
    .unwrap();
    let source = parsed.single_select().unwrap().unwrap();
    for item in &source.select().projection {
        assert!(matches!(
            item,
            SelectItem::UnnamedExpr(Expr::Prefixed { .. })
        ));
    }
}

#[test]
fn foreign_byte_string_parsing_is_unchanged() {
    for dialect in [
        &BigQueryDialect {} as &dyn sqlparser::dialect::Dialect,
        &PostgreSqlDialect {},
        &GenericDialect {},
    ] {
        let parsed = Parser::parse_sql(dialect, "SELECT B'abc'").unwrap();
        let sqlparser::ast::Statement::Query(query) = &parsed[0] else {
            panic!("query");
        };
        let sqlparser::ast::SetExpr::Select(select) = query.body.as_ref() else {
            panic!("select");
        };
        assert!(
            matches!(&select.projection[0], SelectItem::UnnamedExpr(Expr::Value(value)) if value.value == Value::SingleQuotedByteStringLiteral("abc".into()))
        );
    }
}
