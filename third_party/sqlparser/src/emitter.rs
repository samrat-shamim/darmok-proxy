// Modified for Darmok: neutral description and alloc/core imports for no_std builds.
// Original extension attribution is retained in NOTICE.
// Licensed under Apache-2.0 (extension to sqlparser-rs)

//! PG-targeted SQL emitter.
//!
//! The upstream sqlparser `Display` impl emits MySQL-flavored SQL.
//! This module provides a `PgEmitter` that writes PostgreSQL-executable SQL:
//! - Backtick identifiers → double-quoted
//! - `?` placeholders → `$N`
//! - `LIMIT offset, count` → `LIMIT count OFFSET offset`
//! - MySQL double-quoted strings → single-quoted strings
//! - Strips MySQL-only table options (ENGINE=, CHARSET=, COLLATE=)
//! - Rejects unsupported MySQL-only constructs (INSERT IGNORE, REPLACE INTO,
//!   ON DUPLICATE KEY UPDATE)
//!
//! The emitter uses the AST-visitor approach (`VisitorMut`) for transformations
//! and the AST `Display` impl for final output, avoiding re-tokenization which
//! would corrupt escaped literals.

use crate::ast::{
    CreateTableOptions, Ident, LimitClause, Offset, OffsetRows, OnInsert, Query, SqlOption,
    Statement, Value, ValueWithSpan, VisitMut, VisitorMut,
};
use core::fmt;
use core::ops::ControlFlow;

#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

/// Options controlling PG SQL emission.
#[derive(Debug, Clone)]
pub struct EmitOptions {
    /// Starting parameter number (default 1).
    pub first_param_index: u32,
    /// Whether to quote all identifiers.
    pub always_quote_identifiers: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            first_param_index: 1,
            always_quote_identifiers: false,
        }
    }
}

impl EmitOptions {
    /// Default options for PostgreSQL emission.
    pub fn postgres() -> Self {
        Self::default()
    }
}

/// Trait for SQL emission. Implemented by PgEmitter for PostgreSQL output.
pub trait SqlEmitter {
    /// Error type for emission failures.
    type Error;
    /// Emit a complete SQL statement.
    fn emit_statement<W: fmt::Write>(
        &mut self,
        stmt: &Statement,
        out: &mut W,
    ) -> Result<(), Self::Error>;
}

/// PostgreSQL SQL emitter.
pub struct PgEmitter {
    /// Emission options.
    pub opts: EmitOptions,
    /// Next $N parameter index.
    pub next_param: u32,
}

impl PgEmitter {
    /// Create a new PG emitter with the given options.
    pub fn new(opts: EmitOptions) -> Self {
        let next_param = opts.first_param_index;
        Self { opts, next_param }
    }

    /// Allocate the next $N parameter placeholder.
    pub fn next_placeholder(&mut self) -> String {
        let n = self.next_param;
        self.next_param += 1;
        format!("${n}")
    }

    /// Reset parameter counter (e.g., between statements).
    pub fn reset_params(&mut self) {
        self.next_param = self.opts.first_param_index;
    }
}

/// Error type for PG emission failures.
#[derive(Debug)]
pub enum EmitError {
    /// AST node that cannot be lowered to PG SQL without a prior rewrite pass.
    UnsupportedNode(String),
    /// fmt::Write error.
    Fmt(fmt::Error),
}

impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmitError::UnsupportedNode(node) => {
                write!(f, "unsupported MySQL node requires lowering: {node}")
            }
            EmitError::Fmt(e) => write!(f, "format error: {e}"),
        }
    }
}

#[cfg(feature = "std")]
impl core::error::Error for EmitError {}

impl From<fmt::Error> for EmitError {
    fn from(e: fmt::Error) -> Self {
        EmitError::Fmt(e)
    }
}

impl SqlEmitter for PgEmitter {
    type Error = EmitError;

    fn emit_statement<W: fmt::Write>(
        &mut self,
        stmt: &Statement,
        out: &mut W,
    ) -> Result<(), EmitError> {
        // 1. Validate: reject MySQL-only constructs that can't be translated.
        validate_statement(stmt)?;

        // 2. Clone the AST and apply transformations via VisitorMut.
        let mut stmt = stmt.clone();

        // Strip MySQL-only CREATE TABLE options.
        strip_mysql_create_table_options(&mut stmt);

        // Apply AST-level rewrites: backtick→double-quote identifiers,
        // ?→$N placeholders, LIMIT offset,count→LIMIT count OFFSET offset,
        // double-quoted strings→single-quoted strings.
        let mut rewriter = PgRewriter { emitter: self };
        let _ = stmt.visit(&mut rewriter);

        // 3. Use the AST's Display impl for output — then fix up any
        //    remaining backtick-quoted identifiers that the visitor couldn't
        //    reach (column defs, non-expression aliases, etc.).
        let sql = stmt.to_string();
        let fixed = rewrite_backtick_to_double_quote(&sql);
        out.write_str(&fixed)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Validation: reject MySQL-only constructs
// ---------------------------------------------------------------------------

fn validate_statement(stmt: &Statement) -> Result<(), EmitError> {
    match stmt {
        Statement::Insert(insert) => {
            if insert.replace_into {
                return Err(EmitError::UnsupportedNode(
                    "REPLACE INTO is MySQL-only; use INSERT ... ON CONFLICT for PG".into(),
                ));
            }
            if insert.ignore {
                return Err(EmitError::UnsupportedNode(
                    "INSERT IGNORE is MySQL-only; use INSERT ... ON CONFLICT DO NOTHING for PG"
                        .into(),
                ));
            }
            if let Some(OnInsert::DuplicateKeyUpdate(_)) = &insert.on {
                return Err(EmitError::UnsupportedNode(
                    "ON DUPLICATE KEY UPDATE is MySQL-only; use ON CONFLICT ... DO UPDATE for PG"
                        .into(),
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// AST visitor: PgRewriter
// ---------------------------------------------------------------------------

struct PgRewriter<'a> {
    emitter: &'a mut PgEmitter,
}

impl VisitorMut for PgRewriter<'_> {
    type Break = ();

    /// Rewrite identifiers: backtick → double-quote.
    fn post_visit_relation(
        &mut self,
        relation: &mut crate::ast::ObjectName,
    ) -> ControlFlow<Self::Break> {
        for part in relation.0.iter_mut() {
            if let crate::ast::ObjectNamePart::Identifier(ident) = part {
                rewrite_ident(ident);
            }
        }
        ControlFlow::Continue(())
    }

    /// Rewrite expressions: backtick idents, ?→$N placeholders.
    fn post_visit_expr(&mut self, expr: &mut crate::ast::Expr) -> ControlFlow<Self::Break> {
        match expr {
            crate::ast::Expr::Identifier(ident) => {
                rewrite_ident(ident);
            }
            crate::ast::Expr::CompoundIdentifier(idents) => {
                for ident in idents.iter_mut() {
                    rewrite_ident(ident);
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    /// Rewrite values: ?→$N placeholders, double-quoted strings→single-quoted.
    fn post_visit_value(&mut self, value: &mut ValueWithSpan) -> ControlFlow<Self::Break> {
        match &value.value {
            Value::Placeholder(p) if p == "?" => {
                value.value = Value::Placeholder(self.emitter.next_placeholder());
            }
            Value::DoubleQuotedString(s) => {
                // In MySQL without ANSI_QUOTES, double-quoted strings are string
                // literals. In PG, double-quotes denote identifiers. Convert to
                // single-quoted string.
                value.value = Value::SingleQuotedString(s.clone());
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    /// Rewrite LIMIT offset, count → LIMIT count OFFSET offset.
    fn post_visit_query(&mut self, query: &mut Query) -> ControlFlow<Self::Break> {
        query.limit_clause = match query.limit_clause.take() {
            Some(LimitClause::OffsetCommaLimit { offset, limit }) => {
                Some(LimitClause::LimitOffset {
                    limit: Some(limit),
                    offset: Some(Offset {
                        value: offset,
                        rows: OffsetRows::None,
                    }),
                    limit_by: vec![],
                })
            }
            other => other,
        };
        ControlFlow::Continue(())
    }

    /// Rewrite select items that contain backtick-quoted aliases.
    fn post_visit_select(&mut self, select: &mut crate::ast::Select) -> ControlFlow<Self::Break> {
        for item in select.projection.iter_mut() {
            if let crate::ast::SelectItem::ExprWithAlias { alias, .. } = item {
                rewrite_ident(alias);
            }
        }
        ControlFlow::Continue(())
    }
}

/// Rewrite a single identifier: backtick quote style → double-quote.
fn rewrite_ident(ident: &mut Ident) {
    if ident.quote_style == Some('`') {
        ident.quote_style = Some('"');
    }
}

// ---------------------------------------------------------------------------
// Post-serialization backtick→double-quote rewrite
// ---------------------------------------------------------------------------

/// Replace backtick-quoted identifiers with double-quoted identifiers in SQL text.
/// This is safe because backticks never appear inside SQL string literals
/// (single-quoted or double-quoted), so we only need to handle the case where
/// we're inside a backtick-delimited identifier.
fn rewrite_backtick_to_double_quote(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // Skip over single-quoted strings (including '' escapes)
            '\'' => {
                out.push('\'');
                loop {
                    match chars.next() {
                        Some('\'') => {
                            // Check for escaped quote ''
                            if chars.peek() == Some(&'\'') {
                                out.push('\'');
                                out.push('\'');
                                chars.next();
                            } else {
                                out.push('\'');
                                break;
                            }
                        }
                        Some(ch) => out.push(ch),
                        None => break,
                    }
                }
            }
            // Skip over double-quoted identifiers/strings
            '"' => {
                out.push('"');
                loop {
                    match chars.next() {
                        Some('"') => {
                            if chars.peek() == Some(&'"') {
                                out.push('"');
                                out.push('"');
                                chars.next();
                            } else {
                                out.push('"');
                                break;
                            }
                        }
                        Some(ch) => out.push(ch),
                        None => break,
                    }
                }
            }
            // Rewrite backtick-quoted identifiers
            '`' => {
                out.push('"');
                loop {
                    match chars.next() {
                        Some('`') => {
                            if chars.peek() == Some(&'`') {
                                // Escaped backtick `` → escaped double-quote ""
                                out.push('"');
                                out.push('"');
                                chars.next();
                            } else {
                                out.push('"');
                                break;
                            }
                        }
                        Some('"') => {
                            // Literal double-quote inside backtick ident needs escaping
                            out.push('"');
                            out.push('"');
                        }
                        Some(ch) => out.push(ch),
                        None => break,
                    }
                }
            }
            _ => out.push(c),
        }
    }

    out
}

// ---------------------------------------------------------------------------
// MySQL CREATE TABLE option stripping
// ---------------------------------------------------------------------------

fn strip_mysql_create_table_options(stmt: &mut Statement) {
    let Statement::CreateTable(create) = stmt else {
        return;
    };

    fn keep_option(option: &SqlOption) -> bool {
        let uppercase = |ident: &Ident| ident.value.to_ascii_uppercase();
        match option {
            SqlOption::Ident(ident) => !is_mysql_table_option_key(&uppercase(ident)),
            SqlOption::KeyValue { key, .. } => !is_mysql_table_option_key(&uppercase(key)),
            SqlOption::NamedParenthesizedList(list) => {
                !is_mysql_table_option_key(&list.key.value.to_ascii_uppercase())
            }
            _ => true,
        }
    }

    create.table_options = match &create.table_options {
        CreateTableOptions::With(options) => rebuild_create_table_options(
            options.iter().filter(|o| keep_option(o)).cloned().collect(),
            CreateTableOptions::With,
        ),
        CreateTableOptions::Options(options) => rebuild_create_table_options(
            options.iter().filter(|o| keep_option(o)).cloned().collect(),
            CreateTableOptions::Options,
        ),
        CreateTableOptions::Plain(options) => rebuild_create_table_options(
            options.iter().filter(|o| keep_option(o)).cloned().collect(),
            CreateTableOptions::Plain,
        ),
        CreateTableOptions::TableProperties(options) => rebuild_create_table_options(
            options.iter().filter(|o| keep_option(o)).cloned().collect(),
            CreateTableOptions::TableProperties,
        ),
        CreateTableOptions::None => CreateTableOptions::None,
    };

    fn rebuild_create_table_options(
        filtered: Vec<SqlOption>,
        builder: fn(Vec<SqlOption>) -> CreateTableOptions,
    ) -> CreateTableOptions {
        if filtered.is_empty() {
            CreateTableOptions::None
        } else {
            builder(filtered)
        }
    }
}

fn is_mysql_table_option_key(key: &str) -> bool {
    matches!(
        key,
        "ENGINE"
            | "CHARSET"
            | "DEFAULT CHARSET"
            | "CHARACTER SET"
            | "DEFAULT CHARACTER SET"
            | "COLLATE"
            | "DEFAULT COLLATE"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dialect::MySqlDialect, parser::Parser};

    fn emit_sql(sql: &str) -> String {
        let stmt = Parser::parse_sql(&MySqlDialect {}, sql).unwrap().remove(0);
        let mut emitter = PgEmitter::new(EmitOptions::postgres());
        let mut out = String::new();
        emitter.emit_statement(&stmt, &mut out).unwrap();
        out
    }

    fn emit_sql_err(sql: &str) -> EmitError {
        let stmt = Parser::parse_sql(&MySqlDialect {}, sql).unwrap().remove(0);
        let mut emitter = PgEmitter::new(EmitOptions::postgres());
        let mut out = String::new();
        emitter.emit_statement(&stmt, &mut out).unwrap_err()
    }

    #[test]
    fn rewrites_placeholders_and_backtick_identifiers() {
        let emitted = emit_sql(r#"SELECT `user`.`name`, ?, ? FROM `accounts`"#);
        assert_eq!(emitted, r#"SELECT "user"."name", $1, $2 FROM "accounts""#);
    }

    #[test]
    fn rewrites_mysql_double_quoted_strings_to_single_quoted() {
        // In MySQL (no ANSI_QUOTES), "abc" is a string literal.
        // In PG, it must become 'abc'.
        let emitted = emit_sql(r#"SELECT "abc" FROM `t`"#);
        assert_eq!(emitted, r#"SELECT 'abc' FROM "t""#);
    }

    #[test]
    fn preserves_escaped_single_quoted_literals() {
        // O'Reilly contains an escaped single quote; must survive round-trip.
        let emitted = emit_sql("SELECT 'O''Reilly' FROM `t`");
        assert_eq!(emitted, r#"SELECT 'O''Reilly' FROM "t""#);
    }

    #[test]
    fn rewrites_mysql_limit_offset_syntax() {
        let emitted = emit_sql("SELECT * FROM `events` LIMIT 5, 10");
        assert_eq!(emitted, r#"SELECT * FROM "events" LIMIT 10 OFFSET 5"#);
    }

    #[test]
    fn rewrites_nested_mysql_limit_offset_syntax() {
        let emitted =
            emit_sql("SELECT * FROM (SELECT * FROM `events` LIMIT 1, 2) AS `e` LIMIT (3 + 4), 5");
        assert_eq!(
            emitted,
            r#"SELECT * FROM (SELECT * FROM "events" LIMIT 2 OFFSET 1) AS "e" LIMIT 5 OFFSET (3 + 4)"#
        );
    }

    #[test]
    fn preserves_standard_limit_clause() {
        let emitted = emit_sql("SELECT * FROM `events` LIMIT 10");
        assert_eq!(emitted, r#"SELECT * FROM "events" LIMIT 10"#);
    }

    #[test]
    fn preserves_placeholder_limit_clause() {
        let emitted = emit_sql("SELECT * FROM `events` WHERE `id` = ? LIMIT ?");
        assert_eq!(
            emitted,
            r#"SELECT * FROM "events" WHERE "id" = $1 LIMIT $2"#
        );
    }

    #[test]
    fn strips_mysql_create_table_options() {
        let emitted = emit_sql(
            "CREATE TABLE `users` (`id` INT) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin",
        );
        assert_eq!(emitted, r#"CREATE TABLE "users" ("id" INT)"#);
    }

    #[test]
    fn rejects_insert_ignore() {
        let err = emit_sql_err("INSERT IGNORE INTO `t` (`a`) VALUES (1)");
        assert!(matches!(err, EmitError::UnsupportedNode(_)));
        let msg = err.to_string();
        assert!(msg.contains("INSERT IGNORE"), "got: {msg}");
    }

    #[test]
    fn rejects_replace_into() {
        let err = emit_sql_err("REPLACE INTO `t` (`a`) VALUES (1)");
        assert!(matches!(err, EmitError::UnsupportedNode(_)));
        let msg = err.to_string();
        assert!(msg.contains("REPLACE INTO"), "got: {msg}");
    }

    #[test]
    fn rejects_on_duplicate_key_update() {
        let err = emit_sql_err("INSERT INTO `t` (`a`) VALUES (1) ON DUPLICATE KEY UPDATE `a` = 2");
        assert!(matches!(err, EmitError::UnsupportedNode(_)));
        let msg = err.to_string();
        assert!(msg.contains("ON DUPLICATE KEY UPDATE"), "got: {msg}");
    }
}
