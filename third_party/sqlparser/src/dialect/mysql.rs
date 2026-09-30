// Modified for Darmok: place production helpers before the test module.
// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
use core::any::TypeId;

use crate::{
    ast::{BinaryOperator, CastKind, DataType, Expr, LockTable, LockTableType, Statement},
    dialect::Dialect,
    keywords::Keyword,
    mysql_mode::{parse_sql_mode, MySqlLexerMode, MySqlModeFlags},
    parser::{Parser, ParserError},
    tokenizer::Token,
};

use super::keywords;

const RESERVED_FOR_TABLE_ALIAS_MYSQL: &[Keyword] = &[
    Keyword::USE,
    Keyword::IGNORE,
    Keyword::FORCE,
    Keyword::STRAIGHT_JOIN,
];

/// A [`Dialect`] for [MySQL](https://www.mysql.com/)
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MySqlDialect {}

/// A MySQL dialect wrapper that applies session `sql_mode` flags.
///
/// This keeps the existing zero-sized [`MySqlDialect`] API stable while
/// allowing callers to opt into session-aware parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ModeAwareMySqlDialect {
    mode_flags: MySqlModeFlags,
}

impl Default for ModeAwareMySqlDialect {
    fn default() -> Self {
        Self::new(MySqlModeFlags::empty())
    }
}

impl ModeAwareMySqlDialect {
    /// Create a mode-aware MySQL dialect from parsed mode flags.
    pub const fn new(mode_flags: MySqlModeFlags) -> Self {
        Self { mode_flags }
    }

    /// Create a mode-aware MySQL dialect from a raw `sql_mode` string.
    pub fn from_sql_mode(sql_mode: &str) -> Self {
        Self::new(parse_sql_mode(sql_mode))
    }

    /// Return the configured session mode flags.
    pub const fn mode_flags(self) -> MySqlModeFlags {
        self.mode_flags
    }

    /// Return the lexer-relevant subset of mode flags.
    pub const fn lexer_mode(self) -> MySqlLexerMode {
        MySqlLexerMode {
            ansi_quotes: self.mode_flags.contains(MySqlModeFlags::ANSI_QUOTES),
            no_backslash_escapes: self
                .mode_flags
                .contains(MySqlModeFlags::NO_BACKSLASH_ESCAPES),
        }
    }
}

impl Dialect for MySqlDialect {
    fn is_identifier_start(&self, ch: char) -> bool {
        // See https://dev.mysql.com/doc/refman/8.0/en/identifiers.html.
        // Identifiers which begin with a digit are recognized while tokenizing numbers,
        // so they can be distinguished from exponent numeric literals.
        // MySQL also implements non ascii utf-8 charecters
        ch.is_alphabetic()
            || ch == '_'
            || ch == '$'
            || ch == '@'
            || ('\u{0080}'..='\u{ffff}').contains(&ch)
            || !ch.is_ascii()
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        self.is_identifier_start(ch) || ch.is_ascii_digit() ||
        // MySQL implements Unicode characters in identifiers.
        !ch.is_ascii()
    }

    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        ch == '`'
    }

    fn identifier_quote_style(&self, _identifier: &str) -> Option<char> {
        Some('`')
    }

    // See https://dev.mysql.com/doc/refman/8.0/en/string-literals.html#character-escape-sequences
    fn supports_string_literal_backslash_escape(&self) -> bool {
        true
    }

    /// see <https://dev.mysql.com/doc/refman/8.4/en/string-functions.html#function_concat>
    fn supports_string_literal_concatenation(&self) -> bool {
        true
    }

    fn ignores_wildcard_escapes(&self) -> bool {
        true
    }

    fn supports_numeric_prefix(&self) -> bool {
        true
    }

    fn supports_bitwise_shift_operators(&self) -> bool {
        true
    }

    /// see <https://dev.mysql.com/doc/refman/8.4/en/comments.html>
    fn supports_multiline_comment_hints(&self) -> bool {
        true
    }

    fn parse_prefix(&self, parser: &mut Parser) -> Option<Result<Expr, ParserError>> {
        parse_binary_prefix_cast(parser, self.prec_value(super::Precedence::Eq))
    }

    fn parse_infix(
        &self,
        parser: &mut crate::parser::Parser,
        expr: &crate::ast::Expr,
        _precedence: u8,
    ) -> Option<Result<crate::ast::Expr, ParserError>> {
        // Parse DIV as an operator
        if parser.parse_keyword(Keyword::DIV) {
            let left = Box::new(expr.clone());
            let right = Box::new(match parser.parse_expr() {
                Ok(expr) => expr,
                Err(e) => return Some(Err(e)),
            });
            Some(Ok(Expr::BinaryOp {
                left,
                op: BinaryOperator::MyIntegerDivide,
                right,
            }))
        } else {
            None
        }
    }

    fn parse_statement(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        if parser.parse_keywords(&[Keyword::LOCK, Keyword::TABLES]) {
            Some(parse_lock_tables(parser))
        } else if parser.parse_keywords(&[Keyword::UNLOCK, Keyword::TABLES]) {
            Some(parse_unlock_tables(parser))
        } else {
            None
        }
    }

    fn require_interval_qualifier(&self) -> bool {
        true
    }

    fn supports_limit_comma(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/create-table-select.html>
    fn supports_create_table_select(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/insert.html>
    fn supports_insert_set(&self) -> bool {
        true
    }

    fn supports_user_host_grantee(&self) -> bool {
        true
    }

    fn is_table_factor_alias(&self, explicit: bool, kw: &Keyword, _parser: &mut Parser) -> bool {
        explicit
            || (!keywords::RESERVED_FOR_TABLE_ALIAS.contains(kw)
                && !RESERVED_FOR_TABLE_ALIAS_MYSQL.contains(kw))
    }

    fn supports_table_hints(&self) -> bool {
        true
    }

    fn requires_single_line_comment_whitespace(&self) -> bool {
        true
    }

    fn supports_match_against(&self) -> bool {
        true
    }

    fn supports_select_modifiers(&self) -> bool {
        true
    }

    fn supports_set_names(&self) -> bool {
        true
    }

    fn supports_comma_separated_set_assignments(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/update.html>
    fn supports_update_order_by(&self) -> bool {
        true
    }

    fn supports_data_type_signed_suffix(&self) -> bool {
        true
    }

    fn supports_cross_join_constraint(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/expressions.html>
    fn supports_double_ampersand_operator(&self) -> bool {
        true
    }

    /// Deprecated functionality by MySQL but still supported
    /// See: <https://dev.mysql.com/doc/refman/8.4/en/cast-functions.html#operator_binary>
    fn supports_binary_kw_as_cast(&self) -> bool {
        true
    }

    fn supports_comment_optimizer_hint(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/create-table.html>
    fn supports_constraint_keyword_without_name(&self) -> bool {
        true
    }

    /// See: <https://dev.mysql.com/doc/refman/8.4/en/create-table.html>
    fn supports_key_column_option(&self) -> bool {
        true
    }
}

impl Dialect for ModeAwareMySqlDialect {
    fn dialect(&self) -> TypeId {
        TypeId::of::<MySqlDialect>()
    }

    fn is_identifier_start(&self, ch: char) -> bool {
        MySqlDialect {}.is_identifier_start(ch)
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        MySqlDialect {}.is_identifier_part(ch)
    }

    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        ch == '`' || (ch == '"' && self.mode_flags.contains(MySqlModeFlags::ANSI_QUOTES))
    }

    fn identifier_quote_style(&self, _identifier: &str) -> Option<char> {
        if self.mode_flags.contains(MySqlModeFlags::ANSI_QUOTES) {
            Some('"')
        } else {
            Some('`')
        }
    }

    fn supports_string_literal_backslash_escape(&self) -> bool {
        !self
            .mode_flags
            .contains(MySqlModeFlags::NO_BACKSLASH_ESCAPES)
    }

    fn supports_string_literal_concatenation(&self) -> bool {
        MySqlDialect {}.supports_string_literal_concatenation()
    }

    fn ignores_wildcard_escapes(&self) -> bool {
        MySqlDialect {}.ignores_wildcard_escapes()
    }

    fn supports_numeric_prefix(&self) -> bool {
        MySqlDialect {}.supports_numeric_prefix()
    }

    fn supports_bitwise_shift_operators(&self) -> bool {
        MySqlDialect {}.supports_bitwise_shift_operators()
    }

    fn supports_multiline_comment_hints(&self) -> bool {
        MySqlDialect {}.supports_multiline_comment_hints()
    }

    fn parse_prefix(&self, parser: &mut Parser) -> Option<Result<Expr, ParserError>> {
        parse_binary_prefix_cast(parser, self.prec_value(super::Precedence::Eq))
    }

    fn get_next_precedence(&self, parser: &Parser) -> Option<Result<u8, ParserError>> {
        if matches!(parser.peek_token_ref().token, Token::StringConcat)
            && !self.mode_flags.contains(MySqlModeFlags::PIPES_AS_CONCAT)
        {
            return Some(Ok(self.prec_value(super::Precedence::Or)));
        }
        None
    }

    fn parse_infix(
        &self,
        parser: &mut crate::parser::Parser,
        expr: &crate::ast::Expr,
        precedence: u8,
    ) -> Option<Result<crate::ast::Expr, ParserError>> {
        if matches!(parser.peek_token_ref().token, Token::StringConcat)
            && !self.mode_flags.contains(MySqlModeFlags::PIPES_AS_CONCAT)
        {
            parser.advance_token();
            return Some(Ok(Expr::BinaryOp {
                left: Box::new(expr.clone()),
                op: BinaryOperator::Or,
                right: Box::new(match parser.parse_subexpr(precedence) {
                    Ok(expr) => expr,
                    Err(err) => return Some(Err(err)),
                }),
            }));
        }

        MySqlDialect {}.parse_infix(parser, expr, precedence)
    }

    fn parse_statement(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        MySqlDialect {}.parse_statement(parser)
    }

    fn require_interval_qualifier(&self) -> bool {
        MySqlDialect {}.require_interval_qualifier()
    }

    fn supports_limit_comma(&self) -> bool {
        MySqlDialect {}.supports_limit_comma()
    }

    fn supports_create_table_select(&self) -> bool {
        MySqlDialect {}.supports_create_table_select()
    }

    fn supports_insert_set(&self) -> bool {
        MySqlDialect {}.supports_insert_set()
    }

    fn supports_user_host_grantee(&self) -> bool {
        MySqlDialect {}.supports_user_host_grantee()
    }

    fn is_table_factor_alias(&self, explicit: bool, kw: &Keyword, parser: &mut Parser) -> bool {
        MySqlDialect {}.is_table_factor_alias(explicit, kw, parser)
    }

    fn supports_table_hints(&self) -> bool {
        MySqlDialect {}.supports_table_hints()
    }

    fn requires_single_line_comment_whitespace(&self) -> bool {
        MySqlDialect {}.requires_single_line_comment_whitespace()
    }

    fn supports_match_against(&self) -> bool {
        MySqlDialect {}.supports_match_against()
    }

    fn supports_select_modifiers(&self) -> bool {
        MySqlDialect {}.supports_select_modifiers()
    }

    fn supports_set_names(&self) -> bool {
        MySqlDialect {}.supports_set_names()
    }

    fn supports_comma_separated_set_assignments(&self) -> bool {
        MySqlDialect {}.supports_comma_separated_set_assignments()
    }

    fn supports_update_order_by(&self) -> bool {
        MySqlDialect {}.supports_update_order_by()
    }

    fn supports_data_type_signed_suffix(&self) -> bool {
        MySqlDialect {}.supports_data_type_signed_suffix()
    }

    fn supports_cross_join_constraint(&self) -> bool {
        MySqlDialect {}.supports_cross_join_constraint()
    }

    fn supports_double_ampersand_operator(&self) -> bool {
        MySqlDialect {}.supports_double_ampersand_operator()
    }

    fn supports_binary_kw_as_cast(&self) -> bool {
        MySqlDialect {}.supports_binary_kw_as_cast()
    }

    fn supports_comment_optimizer_hint(&self) -> bool {
        MySqlDialect {}.supports_comment_optimizer_hint()
    }

    fn supports_constraint_keyword_without_name(&self) -> bool {
        MySqlDialect {}.supports_constraint_keyword_without_name()
    }

    fn supports_key_column_option(&self) -> bool {
        MySqlDialect {}.supports_key_column_option()
    }
}

fn parse_binary_prefix_cast(
    parser: &mut Parser,
    operand_precedence: u8,
) -> Option<Result<Expr, ParserError>> {
    if !parser.parse_keyword(Keyword::BINARY) {
        return None;
    }

    Some(
        parser
            .parse_subexpr(operand_precedence)
            .map(|expr| Expr::Cast {
                kind: CastKind::Cast,
                expr: Box::new(expr),
                data_type: DataType::Binary(None),
                array: false,
                format: None,
            }),
    )
}

/// `LOCK TABLES`
/// <https://dev.mysql.com/doc/refman/8.0/en/lock-tables.html>
fn parse_lock_tables(parser: &mut Parser) -> Result<Statement, ParserError> {
    let tables = parser.parse_comma_separated(parse_lock_table)?;
    Ok(Statement::LockTables { tables })
}

// tbl_name [[AS] alias] lock_type
fn parse_lock_table(parser: &mut Parser) -> Result<LockTable, ParserError> {
    let table = parser.parse_identifier()?;
    let alias =
        parser.parse_optional_alias(&[Keyword::READ, Keyword::WRITE, Keyword::LOW_PRIORITY])?;
    let lock_type = parse_lock_tables_type(parser)?;

    Ok(LockTable {
        table,
        alias,
        lock_type,
    })
}

// READ [LOCAL] | [LOW_PRIORITY] WRITE
fn parse_lock_tables_type(parser: &mut Parser) -> Result<LockTableType, ParserError> {
    if parser.parse_keyword(Keyword::READ) {
        if parser.parse_keyword(Keyword::LOCAL) {
            Ok(LockTableType::Read { local: true })
        } else {
            Ok(LockTableType::Read { local: false })
        }
    } else if parser.parse_keyword(Keyword::WRITE) {
        Ok(LockTableType::Write {
            low_priority: false,
        })
    } else if parser.parse_keywords(&[Keyword::LOW_PRIORITY, Keyword::WRITE]) {
        Ok(LockTableType::Write { low_priority: true })
    } else {
        parser.expected_ref("an lock type in LOCK TABLES", parser.peek_token_ref())
    }
}

/// UNLOCK TABLES
/// <https://dev.mysql.com/doc/refman/8.0/en/lock-tables.html>
fn parse_unlock_tables(_parser: &mut Parser) -> Result<Statement, ParserError> {
    Ok(Statement::UnlockTables)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ast::{BinaryOperator, Expr, SelectItem, Statement, TableFactor},
        parser::Parser,
    };

    fn only_select_expr(stmt: &Statement) -> &Expr {
        let Statement::Query(query) = stmt else {
            panic!("expected query statement");
        };
        let Some(select) = query.body.as_select() else {
            panic!("expected select body");
        };
        let SelectItem::UnnamedExpr(expr) = &select.projection[0] else {
            panic!("expected unnamed expression");
        };
        expr
    }

    #[test]
    fn ansi_quotes_mode_parses_identifiers() {
        let stmt =
            Parser::parse_mysql_sql_with_mode_string(r#"SELECT "col" FROM "tbl""#, "ANSI_QUOTES")
                .unwrap()
                .remove(0);

        let Statement::Query(query) = stmt else {
            panic!("expected query statement");
        };
        let Some(select) = query.body.as_select() else {
            panic!("expected select body");
        };
        let SelectItem::UnnamedExpr(Expr::Identifier(ident)) = &select.projection[0] else {
            panic!("expected identifier projection");
        };
        assert_eq!(ident.value, "col");
        assert_eq!(ident.quote_style, Some('"'));

        let TableFactor::Table { name, .. } = &select.from[0].relation else {
            panic!("expected table factor");
        };
        assert_eq!(name.to_string(), r#""tbl""#);
    }

    #[test]
    fn no_backslash_escapes_mode_rejects_mysql_escape_sequences() {
        let err = Parser::parse_mysql_sql_with_flags(
            r"SELECT 'I\'m'",
            MySqlModeFlags::from_bits(MySqlModeFlags::NO_BACKSLASH_ESCAPES),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            ParserError::TokenizerError(_) | ParserError::ParserError(_)
        ));
    }

    #[test]
    fn pipes_as_concat_mode_controls_operator_semantics() {
        let default_stmt = Parser::parse_mysql_sql("SELECT 'a' || 'b'")
            .unwrap()
            .remove(0);
        let concat_stmt =
            Parser::parse_mysql_sql_with_mode_string("SELECT 'a' || 'b'", "PIPES_AS_CONCAT")
                .unwrap()
                .remove(0);

        let Expr::BinaryOp { op, .. } = only_select_expr(&default_stmt) else {
            panic!("expected binary operator");
        };
        assert_eq!(*op, BinaryOperator::Or);

        let Expr::BinaryOp { op, .. } = only_select_expr(&concat_stmt) else {
            panic!("expected binary operator");
        };
        assert_eq!(*op, BinaryOperator::StringConcat);
    }

    #[test]
    fn binary_prefix_parenthesized_operand_stays_cast_operand() {
        let stmt = Parser::parse_mysql_sql("SELECT BINARY (a || b) = 'x' FROM t")
            .unwrap()
            .remove(0);

        let Expr::BinaryOp { left, .. } = only_select_expr(&stmt) else {
            panic!("expected comparison expression");
        };
        let Expr::Cast {
            expr, data_type, ..
        } = left.as_ref()
        else {
            panic!("expected BINARY prefix cast, got {left:?}");
        };

        assert!(matches!(data_type, DataType::Binary(None)));
        assert!(matches!(expr.as_ref(), Expr::Nested(_)));
    }
}
