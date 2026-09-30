// Modified for Darmok: neutral description and explicit alloc imports for no_std builds.
// Original extension attribution is retained in NOTICE.
// Licensed under Apache-2.0 (extension to sqlparser-rs)

//! Session-aware MySQL parsing mode flags.
//!
//! These flags affect how the MySQL dialect parses SQL. They must be set
//! before parsing based on the connection's sql_mode.

use core::any::TypeId;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::ast::Statement;
use crate::dialect::{Dialect, ModeAwareMySqlDialect, MySqlDialect};
use crate::parser::{Parser, ParserError};

/// MySQL sql_mode flags that affect parsing behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MySqlModeFlags(u32);

impl MySqlModeFlags {
    /// Double-quote is an identifier quote, not a string literal quote.
    pub const ANSI_QUOTES: u32 = 1 << 0;
    /// Backslash has no special meaning in string literals.
    pub const NO_BACKSLASH_ESCAPES: u32 = 1 << 1;
    /// `||` is string concatenation (like ANSI SQL), not logical OR.
    pub const PIPES_AS_CONCAT: u32 = 1 << 2;
    /// Permit spaces between function name and `(` for built-in functions.
    pub const IGNORE_SPACE: u32 = 1 << 3;
    /// `NOT` has higher precedence (MySQL pre-5.7 behavior).
    pub const HIGH_NOT_PRECEDENCE: u32 = 1 << 4;
    /// `REAL` is `FLOAT` instead of `DOUBLE`.
    pub const REAL_AS_FLOAT: u32 = 1 << 5;

    /// Create empty flags (MySQL 8.0 default).
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Create from raw bits.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Get raw bits.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Check if a flag is set.
    pub const fn contains(self, flag: u32) -> bool {
        self.0 & flag != 0
    }

    /// Set a flag.
    pub fn insert(&mut self, flag: u32) {
        self.0 |= flag;
    }
}

/// Subset of mode flags that affect the lexer/tokenizer.
/// Used by the parameterizer for charset-safe literal extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MySqlLexerMode {
    /// Double-quote is an identifier quote.
    pub ansi_quotes: bool,
    /// Backslash has no special meaning in string literals.
    pub no_backslash_escapes: bool,
}

impl From<MySqlModeFlags> for MySqlLexerMode {
    fn from(flags: MySqlModeFlags) -> Self {
        Self {
            ansi_quotes: flags.contains(MySqlModeFlags::ANSI_QUOTES),
            no_backslash_escapes: flags.contains(MySqlModeFlags::NO_BACKSLASH_ESCAPES),
        }
    }
}

/// A session-aware MySQL dialect that preserves upstream MySQL parsing behavior
/// while letting a session's `sql_mode` adjust a small set of lexer/parser rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SessionMySqlDialect {
    base: MySqlDialect,
    flags: MySqlModeFlags,
}

impl SessionMySqlDialect {
    /// Create a new session-aware MySQL dialect for the provided mode flags.
    pub const fn new(flags: MySqlModeFlags) -> Self {
        Self {
            base: MySqlDialect {},
            flags,
        }
    }

    /// Return the configured mode flags.
    pub const fn flags(self) -> MySqlModeFlags {
        self.flags
    }

    fn lexer_mode(self) -> MySqlLexerMode {
        self.flags.into()
    }
}

impl Dialect for SessionMySqlDialect {
    fn dialect(&self) -> TypeId {
        TypeId::of::<MySqlDialect>()
    }

    fn is_identifier_start(&self, ch: char) -> bool {
        <MySqlDialect as Dialect>::is_identifier_start(&self.base, ch)
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        <MySqlDialect as Dialect>::is_identifier_part(&self.base, ch)
    }

    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        ch == '`' || (self.lexer_mode().ansi_quotes && ch == '"')
    }

    fn identifier_quote_style(&self, _identifier: &str) -> Option<char> {
        if self.lexer_mode().ansi_quotes {
            Some('"')
        } else {
            Some('`')
        }
    }

    fn supports_string_literal_backslash_escape(&self) -> bool {
        !self.lexer_mode().no_backslash_escapes
    }

    fn supports_string_literal_concatenation(&self) -> bool {
        <MySqlDialect as Dialect>::supports_string_literal_concatenation(&self.base)
    }

    fn ignores_wildcard_escapes(&self) -> bool {
        <MySqlDialect as Dialect>::ignores_wildcard_escapes(&self.base)
    }

    fn supports_numeric_prefix(&self) -> bool {
        <MySqlDialect as Dialect>::supports_numeric_prefix(&self.base)
    }

    fn supports_bitwise_shift_operators(&self) -> bool {
        <MySqlDialect as Dialect>::supports_bitwise_shift_operators(&self.base)
    }

    fn supports_multiline_comment_hints(&self) -> bool {
        <MySqlDialect as Dialect>::supports_multiline_comment_hints(&self.base)
    }

    fn parse_prefix(&self, parser: &mut Parser) -> Option<Result<crate::ast::Expr, ParserError>> {
        <MySqlDialect as Dialect>::parse_prefix(&self.base, parser)
    }

    fn parse_infix(
        &self,
        parser: &mut Parser,
        expr: &crate::ast::Expr,
        precedence: u8,
    ) -> Option<Result<crate::ast::Expr, ParserError>> {
        <MySqlDialect as Dialect>::parse_infix(&self.base, parser, expr, precedence)
    }

    fn parse_statement(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        <MySqlDialect as Dialect>::parse_statement(&self.base, parser)
    }

    fn require_interval_qualifier(&self) -> bool {
        <MySqlDialect as Dialect>::require_interval_qualifier(&self.base)
    }

    fn supports_limit_comma(&self) -> bool {
        <MySqlDialect as Dialect>::supports_limit_comma(&self.base)
    }

    fn supports_create_table_select(&self) -> bool {
        <MySqlDialect as Dialect>::supports_create_table_select(&self.base)
    }

    fn supports_insert_set(&self) -> bool {
        <MySqlDialect as Dialect>::supports_insert_set(&self.base)
    }

    fn supports_user_host_grantee(&self) -> bool {
        <MySqlDialect as Dialect>::supports_user_host_grantee(&self.base)
    }

    fn is_table_factor_alias(
        &self,
        explicit: bool,
        kw: &crate::keywords::Keyword,
        parser: &mut Parser,
    ) -> bool {
        <MySqlDialect as Dialect>::is_table_factor_alias(&self.base, explicit, kw, parser)
    }

    fn supports_table_hints(&self) -> bool {
        <MySqlDialect as Dialect>::supports_table_hints(&self.base)
    }

    fn requires_single_line_comment_whitespace(&self) -> bool {
        <MySqlDialect as Dialect>::requires_single_line_comment_whitespace(&self.base)
    }

    fn supports_match_against(&self) -> bool {
        <MySqlDialect as Dialect>::supports_match_against(&self.base)
    }

    fn supports_select_modifiers(&self) -> bool {
        <MySqlDialect as Dialect>::supports_select_modifiers(&self.base)
    }

    fn supports_set_names(&self) -> bool {
        <MySqlDialect as Dialect>::supports_set_names(&self.base)
    }

    fn supports_comma_separated_set_assignments(&self) -> bool {
        <MySqlDialect as Dialect>::supports_comma_separated_set_assignments(&self.base)
    }

    fn supports_update_order_by(&self) -> bool {
        <MySqlDialect as Dialect>::supports_update_order_by(&self.base)
    }

    fn supports_data_type_signed_suffix(&self) -> bool {
        <MySqlDialect as Dialect>::supports_data_type_signed_suffix(&self.base)
    }

    fn supports_cross_join_constraint(&self) -> bool {
        <MySqlDialect as Dialect>::supports_cross_join_constraint(&self.base)
    }

    fn supports_double_ampersand_operator(&self) -> bool {
        <MySqlDialect as Dialect>::supports_double_ampersand_operator(&self.base)
    }

    fn supports_binary_kw_as_cast(&self) -> bool {
        <MySqlDialect as Dialect>::supports_binary_kw_as_cast(&self.base)
    }

    fn supports_comment_optimizer_hint(&self) -> bool {
        <MySqlDialect as Dialect>::supports_comment_optimizer_hint(&self.base)
    }

    fn supports_constraint_keyword_without_name(&self) -> bool {
        <MySqlDialect as Dialect>::supports_constraint_keyword_without_name(&self.base)
    }

    fn supports_key_column_option(&self) -> bool {
        <MySqlDialect as Dialect>::supports_key_column_option(&self.base)
    }
}

/// Create a session-aware MySQL dialect for the provided `sql_mode` flags.
pub fn mysql_dialect(flags: MySqlModeFlags) -> SessionMySqlDialect {
    SessionMySqlDialect::new(flags)
}

/// Parse MySQL SQL using a mode-aware MySQL dialect configured from
/// session `sql_mode` flags.
pub fn parse_mysql_with_mode(
    sql: &str,
    flags: MySqlModeFlags,
) -> Result<Vec<Statement>, ParserError> {
    let dialect = ModeAwareMySqlDialect::new(flags);
    Parser::parse_sql(&dialect, sql)
}

/// Parse MySQL SQL into raw AST statements.
///
/// With the `visitor` feature, metadata-aware parsing is also available through
/// `metadata::parse_mysql_statement` and `metadata::parse_mysql_sql`.
pub fn parse_mysql_sql(sql: &str, flags: MySqlModeFlags) -> Result<Vec<Statement>, ParserError> {
    parse_mysql_with_mode(sql, flags)
}

/// Parse a sql_mode string (e.g. "ANSI_QUOTES,NO_BACKSLASH_ESCAPES") into flags.
pub fn parse_sql_mode(sql_mode: &str) -> MySqlModeFlags {
    let mut flags = MySqlModeFlags::empty();
    for part in sql_mode.split(',') {
        match part.trim().to_uppercase().as_str() {
            "ANSI_QUOTES" => flags.insert(MySqlModeFlags::ANSI_QUOTES),
            "NO_BACKSLASH_ESCAPES" => flags.insert(MySqlModeFlags::NO_BACKSLASH_ESCAPES),
            "PIPES_AS_CONCAT" => flags.insert(MySqlModeFlags::PIPES_AS_CONCAT),
            "IGNORE_SPACE" => flags.insert(MySqlModeFlags::IGNORE_SPACE),
            "HIGH_NOT_PRECEDENCE" | "NO_FIELD_OPTIONS" => {
                flags.insert(MySqlModeFlags::HIGH_NOT_PRECEDENCE);
            }
            "REAL_AS_FLOAT" => flags.insert(MySqlModeFlags::REAL_AS_FLOAT),
            "ANSI" => {
                flags.insert(MySqlModeFlags::ANSI_QUOTES);
                flags.insert(MySqlModeFlags::PIPES_AS_CONCAT);
                flags.insert(MySqlModeFlags::REAL_AS_FLOAT);
            }
            _ => {} // Ignore unknown modes (STRICT_TRANS_TABLES, etc.)
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, SelectItem, SetExpr, Statement, Value};
    use crate::parser::Parser;

    #[test]
    fn parse_empty_mode() {
        assert_eq!(parse_sql_mode(""), MySqlModeFlags::empty());
    }

    #[test]
    fn parse_ansi_quotes() {
        let flags = parse_sql_mode("ANSI_QUOTES");
        assert!(flags.contains(MySqlModeFlags::ANSI_QUOTES));
        assert!(!flags.contains(MySqlModeFlags::NO_BACKSLASH_ESCAPES));
    }

    #[test]
    fn parse_multiple_modes() {
        let flags = parse_sql_mode("ANSI_QUOTES,NO_BACKSLASH_ESCAPES,STRICT_TRANS_TABLES");
        assert!(flags.contains(MySqlModeFlags::ANSI_QUOTES));
        assert!(flags.contains(MySqlModeFlags::NO_BACKSLASH_ESCAPES));
    }

    #[test]
    fn parse_ansi_composite() {
        let flags = parse_sql_mode("ANSI");
        assert!(flags.contains(MySqlModeFlags::ANSI_QUOTES));
        assert!(flags.contains(MySqlModeFlags::PIPES_AS_CONCAT));
        assert!(flags.contains(MySqlModeFlags::REAL_AS_FLOAT));
    }

    #[test]
    fn lexer_mode_from_flags() {
        let mut flags = MySqlModeFlags::empty();
        flags.insert(MySqlModeFlags::ANSI_QUOTES);
        flags.insert(MySqlModeFlags::PIPES_AS_CONCAT);
        let lexer: MySqlLexerMode = flags.into();
        assert!(lexer.ansi_quotes);
        assert!(!lexer.no_backslash_escapes);
    }

    #[test]
    fn ansi_quotes_treats_double_quotes_as_identifiers() {
        let sql = r#"SELECT "col" FROM "tbl""name""#;
        let dialect = mysql_dialect(MySqlModeFlags::from_bits(MySqlModeFlags::ANSI_QUOTES));
        let ast = Parser::parse_sql(&dialect, sql).unwrap();

        let Statement::Query(query) = &ast[0] else {
            panic!("expected query");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("expected select");
        };
        let SelectItem::UnnamedExpr(Expr::Identifier(ident)) = &select.projection[0] else {
            panic!("expected identifier projection");
        };
        assert_eq!(ident.value, "col");
        assert_eq!(ident.quote_style, Some('"'));
    }

    #[test]
    fn parse_mysql_with_mode_uses_mode_aware_dialect() {
        let ast = parse_mysql_with_mode(
            r#"SELECT "col""#,
            MySqlModeFlags::from_bits(MySqlModeFlags::ANSI_QUOTES),
        )
        .unwrap();

        let Statement::Query(query) = &ast[0] else {
            panic!("expected query");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("expected select");
        };
        let SelectItem::UnnamedExpr(Expr::Identifier(ident)) = &select.projection[0] else {
            panic!("expected identifier projection");
        };
        assert_eq!(ident.value, "col");
        assert_eq!(ident.quote_style, Some('"'));
    }

    #[test]
    fn default_mysql_mode_treats_double_quotes_as_strings() {
        let ast = parse_mysql_sql(r#"SELECT "col""#, MySqlModeFlags::empty()).unwrap();
        let Statement::Query(query) = &ast[0] else {
            panic!("expected query");
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            panic!("expected select");
        };
        let SelectItem::UnnamedExpr(Expr::Value(value)) = &select.projection[0] else {
            panic!("expected string literal");
        };
        assert_eq!(value.value, Value::DoubleQuotedString("col".into()));
    }

    #[test]
    fn no_backslash_escapes_preserves_literal_backslashes() {
        let escaped = parse_mysql_sql("SELECT 'a\\nb'", MySqlModeFlags::empty()).unwrap();
        let raw = parse_mysql_sql(
            "SELECT 'a\\nb'",
            MySqlModeFlags::from_bits(MySqlModeFlags::NO_BACKSLASH_ESCAPES),
        )
        .unwrap();

        let extract = |stmt: &Statement| -> Value {
            let Statement::Query(query) = stmt else {
                panic!("expected query");
            };
            let SetExpr::Select(select) = query.body.as_ref() else {
                panic!("expected select");
            };
            let SelectItem::UnnamedExpr(Expr::Value(value)) = &select.projection[0] else {
                panic!("expected value");
            };
            value.value.clone()
        };

        assert_eq!(
            extract(&escaped[0]),
            Value::SingleQuotedString("a\nb".into())
        );
        assert_eq!(extract(&raw[0]), Value::SingleQuotedString("a\\nb".into()));
    }
}
