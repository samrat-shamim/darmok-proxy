// Modified for Darmok: structural identifier emission, explicit option rejection, owned emission, and no_std imports.
// Original extension attribution is retained in NOTICE.
// Licensed under Apache-2.0 (extension to sqlparser-rs)

//! PG-targeted SQL emitter.
//!
//! The upstream sqlparser `Display` impl emits MySQL-flavored SQL.
//! This module provides PostgreSQL syntax emission after semantic lowering:
//! - Backtick identifiers → double-quoted
//! - `?` placeholders → `$N`
//! - `LIMIT offset, count` → `LIMIT count OFFSET offset`
//! - MySQL double-quoted strings → single-quoted strings
//! - Rejects residual MySQL-only table options (ENGINE=, CHARSET=, COLLATE=)
//! - Rejects unsupported MySQL-only constructs (INSERT IGNORE, REPLACE INTO,
//!   ON DUPLICATE KEY UPDATE)
//!
//! The emitter uses the AST-visitor approach (`VisitorMut`) for transformations
//! and the AST `Display` impl for final output, avoiding re-tokenization which
//! would corrupt escaped literals.

use crate::ast::{
    check_ast, drop_ast, AstLimits, CreateTableOptions, Ident, LimitClause, Offset, OffsetRows,
    OnInsert, Query, SqlOption, Statement, Value, VisitMut, VisitorMut,
};
use crate::dialect::{Dialect, PostgreSqlDialect};
use core::fmt;
use core::ops::ControlFlow;

#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
    vec,
};

/// Options controlling PG SQL emission.
#[derive(Debug, Clone)]
pub struct EmitOptions {
    /// Starting parameter number (default 1).
    pub first_param_index: u32,
}

// The compiler can impose tighter limits. Direct emitter callers receive the
// same bounded recursive-processing contract before any clone or formatting.
const EMIT_AST_LIMITS: AstLimits = AstLimits {
    max_depth: 128,
    max_nodes: 1_000_000,
};

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            first_param_index: 1,
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
        check_ast(stmt, EMIT_AST_LIMITS)
            .map_err(|_| EmitError::UnsupportedNode("AST resource limit exceeded".into()))?;
        self.emit_statement_owned(stmt.clone(), out)
    }
}

impl PgEmitter {
    /// Consume an already-owned AST without making another full copy. The
    /// caller must perform semantic and catalog validation before execution.
    pub fn emit_statement_owned<W: fmt::Write>(
        &mut self,
        mut stmt: Statement,
        out: &mut W,
    ) -> Result<(), EmitError> {
        if check_ast(&stmt, EMIT_AST_LIMITS).is_err() {
            drop_ast(stmt);
            return Err(EmitError::UnsupportedNode(
                "AST resource limit exceeded".into(),
            ));
        }
        let mut rewriter = PgRewriter {
            emitter: self,
            in_function_name: false,
        };
        if let ControlFlow::Break(error) = stmt.visit(&mut rewriter) {
            return Err(error);
        }
        write!(out, "{stmt}")?;
        Ok(())
    }
}

fn validate_statement(stmt: &Statement) -> Result<(), EmitError> {
    match stmt {
        Statement::Insert(insert) => {
            if insert.replace_into {
                return Err(EmitError::UnsupportedNode(
                    "REPLACE INTO requires semantic lowering".into(),
                ));
            }
            if insert.ignore {
                return Err(EmitError::UnsupportedNode(
                    "INSERT IGNORE requires semantic lowering".into(),
                ));
            }
            if let Some(OnInsert::DuplicateKeyUpdate(_)) = &insert.on {
                return Err(EmitError::UnsupportedNode(
                    "ON DUPLICATE KEY UPDATE requires semantic lowering".into(),
                ));
            }
        }
        Statement::CreateTable(create) => {
            let options = match &create.table_options {
                CreateTableOptions::With(options)
                | CreateTableOptions::Options(options)
                | CreateTableOptions::Plain(options)
                | CreateTableOptions::TableProperties(options) => options.as_slice(),
                CreateTableOptions::None => &[],
            };
            for option in options {
                let key = match option {
                    SqlOption::Ident(ident) | SqlOption::KeyValue { key: ident, .. } => {
                        Some(&ident.value)
                    }
                    SqlOption::NamedParenthesizedList(list) => Some(&list.key.value),
                    _ => None,
                };
                if key.is_some_and(|key| is_mysql_table_option_key(&key.to_ascii_uppercase())) {
                    return Err(EmitError::UnsupportedNode(
                        "MySQL table options require semantic lowering".into(),
                    ));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// AST visitor: PgRewriter
// ---------------------------------------------------------------------------

struct PgRewriter<'a> {
    emitter: &'a mut PgEmitter,
    in_function_name: bool,
}

impl VisitorMut for PgRewriter<'_> {
    type Break = EmitError;

    fn pre_visit_statement(&mut self, statement: &mut Statement) -> ControlFlow<Self::Break> {
        match validate_statement(statement) {
            Ok(()) => ControlFlow::Continue(()),
            Err(error) => ControlFlow::Break(error),
        }
    }

    fn pre_visit_function_name(
        &mut self,
        name: &mut crate::ast::ObjectName,
    ) -> ControlFlow<Self::Break> {
        if name
            .0
            .iter()
            .any(|part| !matches!(part, crate::ast::ObjectNamePart::Identifier(_)))
        {
            return ControlFlow::Break(EmitError::UnsupportedNode(
                "dynamic function names require lowering".into(),
            ));
        }
        let qualifier_count = name.0.len().saturating_sub(1);
        for part in name.0.iter_mut().take(qualifier_count) {
            if let crate::ast::ObjectNamePart::Identifier(ident) = part {
                ident.quote_style = Some('"');
            }
        }
        self.in_function_name = true;
        ControlFlow::Continue(())
    }

    fn post_visit_function_name(
        &mut self,
        _name: &mut crate::ast::ObjectName,
    ) -> ControlFlow<Self::Break> {
        self.in_function_name = false;
        ControlFlow::Continue(())
    }

    /// Every identifier has a visitor hook, including column definitions,
    /// aliases and constraint names. Quoting never rewrites serialized SQL.
    fn pre_visit_ident(&mut self, ident: &mut Ident) -> ControlFlow<Self::Break> {
        if ident.quote_style.is_some()
            || !self.in_function_name
            || !can_emit_unquoted_function(&ident.value)
        {
            ident.quote_style = Some('"');
        }
        ControlFlow::Continue(())
    }

    /// Rewrite values: ?→$N placeholders, double-quoted strings→single-quoted.
    fn post_visit_value(&mut self, value: &mut Value) -> ControlFlow<Self::Break> {
        match value {
            Value::Placeholder(p) if p == "?" => {
                *value = Value::Placeholder(self.emitter.next_placeholder());
            }
            Value::DoubleQuotedString(s) => {
                // In MySQL without ANSI_QUOTES, double-quoted strings are string
                // literals. In PG, double-quotes denote identifiers. Convert to
                // single-quoted string.
                *value = Value::SingleQuotedString(s.clone());
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
}

fn can_emit_unquoted_function(value: &str) -> bool {
    let dialect = PostgreSqlDialect {};
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !dialect.is_identifier_start(first) || !characters.all(|ch| dialect.is_identifier_part(ch)) {
        return false;
    }
    true
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
    fn preserves_identifier_contents_in_column_definitions_and_aliases() {
        assert_eq!(
            emit_sql("CREATE TABLE `a``b` (`c``d` INT, `e\"f` TEXT)"),
            "CREATE TABLE \"a`b\" (\"c`d\" INT, \"e\"\"f\" TEXT)"
        );
        assert_eq!(
            emit_sql("SELECT 'a`b' AS `c``d` FROM `records` AS `e\"f`"),
            "SELECT 'a`b' AS \"c`d\" FROM \"records\" AS \"e\"\"f\""
        );
    }

    #[test]
    fn unquoted_object_names_are_quoted_structurally_without_case_folding() {
        let statement = Parser::parse_sql(&MySqlDialect {}, "SELECT Id FROM Records AS R")
            .unwrap()
            .remove(0);
        let mut emitter = PgEmitter::new(EmitOptions::postgres());
        let mut sql = String::new();
        emitter.emit_statement_owned(statement, &mut sql).unwrap();
        assert_eq!(sql, "SELECT \"Id\" FROM \"Records\" AS \"R\"");
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
    fn rejects_unlowered_mysql_create_table_options() {
        let error = emit_sql_err(
            "CREATE TABLE `users` (`id` INT) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin",
        );
        assert!(matches!(error, EmitError::UnsupportedNode(_)));
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
