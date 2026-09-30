// Modified for Darmok: neutral description and explicit alloc imports for no_std builds.
// Original extension attribution is retained in NOTICE.
// Licensed under Apache-2.0 (extension to sqlparser-rs)

//! Translation-aware AST metadata.
//!
//! Embedded during parsing to eliminate separate analysis passes.
//! The parser detects syntax-local facts (statement kind, insert strategy,
//! placeholder positions) so the translator consumes them directly.

use core::ops::ControlFlow;

#[cfg(not(feature = "std"))]
use alloc::{format, string::ToString, vec::Vec};

use crate::ast::{
    Expr, Function, ObjectType, OnInsert, Query, Select, SetExpr, Statement, TableFactor, Value,
    Visit, Visitor,
};
use crate::mysql_mode::{parse_mysql_with_mode, MySqlModeFlags};
use crate::parser::ParserError;

/// Coarse statement classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    /// SELECT query.
    Select,
    /// INSERT statement.
    Insert,
    /// UPDATE statement.
    Update,
    /// DELETE statement.
    Delete,
    /// CREATE TABLE.
    CreateTable,
    /// ALTER TABLE.
    AlterTable,
    /// DROP TABLE.
    DropTable,
    /// SHOW command.
    Show,
    /// SET command.
    Set,
    /// USE database.
    Use,
    /// BEGIN transaction.
    Begin,
    /// COMMIT transaction.
    Commit,
    /// ROLLBACK transaction.
    Rollback,
    /// SAVEPOINT.
    Savepoint,
    /// Anything else.
    Other,
}

/// Insert conflict handling strategy, detected at parse time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertStrategy {
    /// Plain INSERT.
    Plain,
    /// INSERT IGNORE — suppress duplicate key errors.
    Ignore,
    /// REPLACE INTO — delete + insert on conflict.
    Replace,
    /// INSERT ... ON DUPLICATE KEY UPDATE.
    OnDuplicateKeyUpdate,
}

/// Statement-level flags detected during parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StmtFlags(u32);

impl StmtFlags {
    /// Uses SQL_CALC_FOUND_ROWS.
    pub const HAS_FOUND_ROWS: u32 = 1 << 0;
    /// Is a DDL statement.
    pub const IS_DDL: u32 = 1 << 1;
    /// Contains a subquery.
    pub const HAS_SUBQUERY: u32 = 1 << 2;
    /// Uses LAST_INSERT_ID().
    pub const HAS_LAST_INSERT_ID: u32 = 1 << 3;
    /// Uses user variables (@var).
    pub const HAS_USER_VARS: u32 = 1 << 4;
    /// Contains GROUP_CONCAT or similar aggregate needing rewrite.
    pub const HAS_AGGREGATE_REWRITE: u32 = 1 << 5;
    /// Contains an ORDER BY clause.
    pub const HAS_ORDER_BY: u32 = 1 << 6;
    /// Contains a LIMIT clause.
    pub const HAS_LIMIT: u32 = 1 << 7;
    /// INSERT source is a SELECT-like query rather than VALUES/SET.
    pub const IS_INSERT_SELECT: u32 = 1 << 8;

    /// Create empty flags.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Return the raw bits.
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

/// Metadata collected during parsing, attached to the parsed statement.
#[derive(Debug, Clone)]
pub struct TranslationMetadata {
    /// Coarse statement classification.
    pub stmt_kind: StatementKind,
    /// Statement-level flags.
    pub stmt_flags: StmtFlags,
    /// Insert strategy (if applicable).
    pub insert_strategy: Option<InsertStrategy>,
    /// Number of `?` placeholders found.
    pub placeholder_count: u16,
}

impl TranslationMetadata {
    /// Create metadata for a simple statement with no special flags.
    pub fn simple(kind: StatementKind) -> Self {
        Self {
            stmt_kind: kind,
            stmt_flags: StmtFlags::empty(),
            insert_strategy: None,
            placeholder_count: 0,
        }
    }
}

/// A parsed statement with attached translation metadata.
#[derive(Debug, Clone)]
pub struct ParsedStatement {
    /// The parsed AST statement.
    pub stmt: Statement,
    /// Translation metadata detected during parsing.
    pub meta: TranslationMetadata,
}

struct MetadataCollector {
    flags: StmtFlags,
    placeholder_count: u16,
}

impl MetadataCollector {
    fn new() -> Self {
        Self {
            flags: StmtFlags::empty(),
            placeholder_count: 0,
        }
    }

    fn finish(self) -> (StmtFlags, u16) {
        (self.flags, self.placeholder_count)
    }
}

impl Visitor for MetadataCollector {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
        if query.order_by.is_some() {
            self.flags.insert(StmtFlags::HAS_ORDER_BY);
        }
        if query.limit_clause.is_some() {
            self.flags.insert(StmtFlags::HAS_LIMIT);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_select(&mut self, select: &Select) -> ControlFlow<Self::Break> {
        if select
            .select_modifiers
            .as_ref()
            .is_some_and(|mods| mods.sql_calc_found_rows)
        {
            self.flags.insert(StmtFlags::HAS_FOUND_ROWS);
        }
        if !select.sort_by.is_empty() {
            self.flags.insert(StmtFlags::HAS_ORDER_BY);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, table_factor: &TableFactor) -> ControlFlow<Self::Break> {
        if matches!(table_factor, TableFactor::Derived { .. }) {
            self.flags.insert(StmtFlags::HAS_SUBQUERY);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        match expr {
            Expr::Subquery(_)
            | Expr::Exists { .. }
            | Expr::InSubquery { .. }
            | Expr::AnyOp { .. }
            | Expr::AllOp { .. } => self.flags.insert(StmtFlags::HAS_SUBQUERY),
            Expr::Function(fun) => record_function_flags(fun, &mut self.flags),
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<Self::Break> {
        match statement {
            Statement::Update(update) => {
                if !update.order_by.is_empty() {
                    self.flags.insert(StmtFlags::HAS_ORDER_BY);
                }
                if update.limit.is_some() {
                    self.flags.insert(StmtFlags::HAS_LIMIT);
                }
            }
            Statement::Delete(delete) => {
                if !delete.order_by.is_empty() {
                    self.flags.insert(StmtFlags::HAS_ORDER_BY);
                }
                if delete.limit.is_some() {
                    self.flags.insert(StmtFlags::HAS_LIMIT);
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_value(&mut self, value: &crate::ast::ValueWithSpan) -> ControlFlow<Self::Break> {
        if matches!(value.value, Value::Placeholder(_)) {
            self.placeholder_count = self.placeholder_count.saturating_add(1);
        }
        ControlFlow::Continue(())
    }
}

fn record_function_flags(function: &Function, flags: &mut StmtFlags) {
    let Some(name) = function.name.0.last() else {
        return;
    };
    let function_name = name.to_string().to_ascii_uppercase();
    match function_name.as_str() {
        "LAST_INSERT_ID" => flags.insert(StmtFlags::HAS_LAST_INSERT_ID),
        "GROUP_CONCAT" => flags.insert(StmtFlags::HAS_AGGREGATE_REWRITE),
        _ => {}
    }
}

fn classify_statement(stmt: &Statement) -> StatementKind {
    match stmt {
        Statement::Query(_) => StatementKind::Select,
        Statement::Insert(_) => StatementKind::Insert,
        Statement::Update(_) => StatementKind::Update,
        Statement::Delete(_) => StatementKind::Delete,
        Statement::CreateTable(_) => StatementKind::CreateTable,
        Statement::AlterTable(_) => StatementKind::AlterTable,
        Statement::Drop {
            object_type: ObjectType::Table,
            ..
        } => StatementKind::DropTable,
        Statement::ShowFunctions { .. }
        | Statement::ShowVariable { .. }
        | Statement::ShowStatus { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowDatabases { .. }
        | Statement::ShowProcessList { .. }
        | Statement::ShowSchemas { .. }
        | Statement::ShowCharset(_)
        | Statement::ShowObjects(_)
        | Statement::ShowTables { .. }
        | Statement::ShowTableStatus { .. }
        | Statement::ShowIndex { .. }
        | Statement::ShowViews { .. }
        | Statement::ShowCollation { .. } => StatementKind::Show,
        Statement::Set(_) => StatementKind::Set,
        Statement::Use(_) => StatementKind::Use,
        Statement::StartTransaction { .. } => StatementKind::Begin,
        Statement::Commit { .. } => StatementKind::Commit,
        Statement::Rollback { .. } => StatementKind::Rollback,
        Statement::Savepoint { .. } | Statement::ReleaseSavepoint { .. } => {
            StatementKind::Savepoint
        }
        _ => StatementKind::Other,
    }
}

fn is_ddl_statement(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::CreateView(_)
            | Statement::CreateTable(_)
            | Statement::CreateVirtualTable { .. }
            | Statement::CreateIndex(_)
            | Statement::CreateRole(_)
            | Statement::CreateSecret { .. }
            | Statement::CreateServer(_)
            | Statement::CreatePolicy(_)
            | Statement::CreateConnector(_)
            | Statement::CreateOperator(_)
            | Statement::CreateOperatorFamily(_)
            | Statement::CreateOperatorClass(_)
            | Statement::AlterTable(_)
            | Statement::AlterSchema(_)
            | Statement::AlterIndex { .. }
            | Statement::AlterView { .. }
            | Statement::AlterFunction(_)
            | Statement::AlterType(_)
            | Statement::AlterOperator(_)
            | Statement::AlterOperatorFamily(_)
            | Statement::AlterOperatorClass(_)
            | Statement::AlterRole { .. }
            | Statement::AlterPolicy(_)
            | Statement::AlterConnector { .. }
            | Statement::Drop { .. }
            | Statement::DropFunction(_)
            | Statement::DropDomain(_)
            | Statement::DropProcedure { .. }
            | Statement::DropSecret { .. }
            | Statement::DropPolicy(_)
            | Statement::DropConnector { .. }
            | Statement::CreateExtension(_)
            | Statement::DropExtension(_)
            | Statement::DropOperator(_)
            | Statement::DropOperatorFamily(_)
            | Statement::DropOperatorClass(_)
            | Statement::CreateSchema { .. }
            | Statement::CreateDatabase { .. }
            | Statement::CreateFunction(_)
            | Statement::CreateTrigger(_)
            | Statement::DropTrigger(_)
            | Statement::CreateProcedure { .. }
            | Statement::CreateMacro { .. }
            | Statement::CreateStage { .. }
            | Statement::CreateSequence { .. }
            | Statement::CreateDomain(_)
            | Statement::CreateType { .. }
            | Statement::Truncate(_)
    )
}

fn insert_strategy(stmt: &Statement) -> Option<InsertStrategy> {
    let Statement::Insert(insert) = stmt else {
        return None;
    };

    let strategy = if insert.replace_into {
        InsertStrategy::Replace
    } else if insert.ignore {
        InsertStrategy::Ignore
    } else if matches!(insert.on, Some(OnInsert::DuplicateKeyUpdate(_))) {
        InsertStrategy::OnDuplicateKeyUpdate
    } else {
        InsertStrategy::Plain
    };

    Some(strategy)
}

fn is_insert_select(stmt: &Statement) -> bool {
    let Statement::Insert(insert) = stmt else {
        return false;
    };
    let Some(source) = insert.source.as_ref() else {
        return false;
    };
    !matches!(source.body.as_ref(), SetExpr::Values(_))
}

fn build_parsed_statement(stmt: Statement) -> ParsedStatement {
    let mut collector = MetadataCollector::new();
    let _ = stmt.visit(&mut collector);
    let (mut stmt_flags, placeholder_count) = collector.finish();
    if is_ddl_statement(&stmt) {
        stmt_flags.insert(StmtFlags::IS_DDL);
    }
    if is_insert_select(&stmt) {
        stmt_flags.insert(StmtFlags::IS_INSERT_SELECT);
    }

    let meta = TranslationMetadata {
        stmt_kind: classify_statement(&stmt),
        stmt_flags,
        insert_strategy: insert_strategy(&stmt),
        placeholder_count,
    };

    ParsedStatement { stmt, meta }
}

/// Parse MySQL SQL using the session's mode flags and attach translation
/// metadata to every parsed statement.
pub fn parse_mysql_statements(
    sql: &str,
    flags: MySqlModeFlags,
) -> Result<Vec<ParsedStatement>, ParserError> {
    parse_mysql_with_mode(sql, flags)
        .map(|statements| statements.into_iter().map(build_parsed_statement).collect())
}

/// Parse MySQL SQL using the session's mode flags and attach translation
/// metadata to every parsed statement.
pub fn parse_mysql_sql(
    sql: &str,
    flags: MySqlModeFlags,
) -> Result<Vec<ParsedStatement>, ParserError> {
    parse_mysql_statements(sql, flags)
}

/// Primary metadata-aware parsing API for callers that expect exactly one
/// statement: parse SQL once and return the statement plus translation hints.
pub fn parse_mysql_statement(
    sql: &str,
    flags: MySqlModeFlags,
) -> Result<ParsedStatement, ParserError> {
    let mut statements = parse_mysql_statements(sql, flags)?;
    if statements.len() != 1 {
        return Err(ParserError::ParserError(format!(
            "expected exactly one statement, found {}",
            statements.len()
        )));
    }

    Ok(statements.pop().expect("checked length"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_collects_select_flags_and_placeholders() {
        let parsed = parse_mysql_statement(
            "SELECT SQL_CALC_FOUND_ROWS * FROM users ORDER BY created_at LIMIT ?, ?",
            MySqlModeFlags::empty(),
        )
        .unwrap();

        assert_eq!(parsed.meta.stmt_kind, StatementKind::Select);
        assert!(parsed.meta.stmt_flags.contains(StmtFlags::HAS_FOUND_ROWS));
        assert!(parsed.meta.stmt_flags.contains(StmtFlags::HAS_ORDER_BY));
        assert!(parsed.meta.stmt_flags.contains(StmtFlags::HAS_LIMIT));
        assert_eq!(parsed.meta.placeholder_count, 2);
    }

    #[test]
    fn metadata_detects_insert_select_and_strategy() {
        let parsed = parse_mysql_statement(
            "INSERT IGNORE INTO dst (id) SELECT id FROM src",
            MySqlModeFlags::empty(),
        )
        .unwrap();

        assert_eq!(parsed.meta.stmt_kind, StatementKind::Insert);
        assert!(parsed.meta.stmt_flags.contains(StmtFlags::IS_INSERT_SELECT));
        assert_eq!(parsed.meta.insert_strategy, Some(InsertStrategy::Ignore));
    }

    #[test]
    fn metadata_detects_on_duplicate_key_update() {
        let parsed = parse_mysql_statement(
            "INSERT INTO dst (id) VALUES (?) ON DUPLICATE KEY UPDATE id = VALUES(id)",
            MySqlModeFlags::empty(),
        )
        .unwrap();

        assert_eq!(
            parsed.meta.insert_strategy,
            Some(InsertStrategy::OnDuplicateKeyUpdate)
        );
        assert_eq!(parsed.meta.placeholder_count, 1);
    }

    #[test]
    fn metadata_marks_create_table_as_ddl() {
        let parsed =
            parse_mysql_statement("CREATE TABLE t (id INT)", MySqlModeFlags::empty()).unwrap();

        assert_eq!(parsed.meta.stmt_kind, StatementKind::CreateTable);
        assert!(parsed.meta.stmt_flags.contains(StmtFlags::IS_DDL));
    }

    #[test]
    fn parse_mysql_sql_collects_metadata_for_each_statement() {
        let parsed = parse_mysql_sql(
            "SELECT ?; INSERT IGNORE INTO dst (id) VALUES (?)",
            MySqlModeFlags::empty(),
        )
        .unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].meta.stmt_kind, StatementKind::Select);
        assert_eq!(parsed[0].meta.placeholder_count, 1);
        assert_eq!(parsed[1].meta.stmt_kind, StatementKind::Insert);
        assert_eq!(parsed[1].meta.insert_strategy, Some(InsertStrategy::Ignore));
        assert_eq!(parsed[1].meta.placeholder_count, 1);
    }

    #[test]
    fn parse_mysql_statement_rejects_multi_statement_input() {
        let err = parse_mysql_statement("SELECT 1; SELECT 2", MySqlModeFlags::empty()).unwrap_err();
        assert!(err
            .to_string()
            .contains("expected exactly one statement, found 2"));
    }
}
