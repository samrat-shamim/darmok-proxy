use std::sync::Arc;

use crate::query::{ProjectionMeta, SidecarMeta};
use crate::value::Value;

/// Coarse classification of a translated plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanKind {
    Select,
    Insert,
    Update,
    Delete,
    Ddl,
    SessionCommand,
    TransactionCommand,
    MetaQuery,
    Administrative,
}

/// Per-parameter coercion rule, determined at translation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamCoercion {
    None,
    BoolToInt,
    IntToBool,
    StringToTimestamp,
    /// String parameter bound to a MySQL numeric column. MySQL accepts
    /// string values in numeric write contexts and coerces invalid or
    /// empty strings to 0; PostgreSQL rejects them before assignment.
    /// Set only when schema metadata proves the target column is
    /// numeric. Non-string values pass through unchanged.
    StringToMysqlNumeric,
    /// Integer parameter in a MySQL `LIMIT` row-count position. MySQL's
    /// `LIMIT` accepts an unsigned 64-bit count, and its documented
    /// idiom for "all rows from an offset to the end" is a huge count
    /// (`LIMIT <offset>, 18446744073709551615`). PostgreSQL's `LIMIT`
    /// takes a signed `bigint` (`int8`, max `9223372036854775807`), so
    /// any count exceeding `i64::MAX` overflows with
    /// `bigint out of range`. When the bound value exceeds `i64::MAX`
    /// (i.e. it is MySQL's effectively-unlimited sentinel), serialize
    /// the placeholder as the keyword `ALL` so `LIMIT $n` becomes
    /// `LIMIT ALL` (PostgreSQL's "no row limit"); values within range
    /// pass through unchanged so ordinary pagination still parameterizes
    /// and shares a cached plan. Non-integer values pass through
    /// unchanged.
    MysqlLimitRowCount,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct PlanFlags: u32 {
        const HAS_FOUND_ROWS = 1 << 0;
        const IS_DDL = 1 << 1;
        const USES_TEMP_TABLE = 1 << 2;
        const NEEDS_TRANSACTION_WRAP = 1 << 3;
        const HAS_LAST_INSERT_ID = 1 << 4;
        const SYNTHETIC_OK = 1 << 5;
        const NEEDS_LOCK_READ_SERIALIZATION = 1 << 6;
        const CREATES_TEMP_TABLE = 1 << 7;
        const DROPS_TEMP_TABLE = 1 << 8;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanWarningLevel {
    Note,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanWarning {
    pub level: PlanWarningLevel,
    pub code: u16,
    pub syntax_id: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogInvalidation {
    pub schema_name: Option<String>,
    pub table_name: String,
}

/// The cached result of translating a MySQL query template to PostgreSQL.
/// This is THE critical shared type — stored in cache, consumed by executor,
/// inspectable by middleware.
#[derive(Debug, Clone)]
pub struct TranslatedPlan {
    /// PostgreSQL SQL with $1..$N placeholders.
    pub pg_sql: String,
    /// Backend bind arity produced by the AST compiler, never rediscovered from SQL text.
    pub parameter_count: u16,
    /// Optional sibling query used to emulate MySQL `FOUND_ROWS()`.
    pub found_rows_count_sql: Option<String>,
    /// Per-parameter coercion rules.
    pub param_coercions: Vec<ParamCoercion>,
    /// Result set metadata for MySQL wire encoding.
    pub projection: Option<Arc<ProjectionMeta>>,
    /// Column nullability/flags sidecar.
    pub sidecar: Option<Arc<SidecarMeta>>,
    /// Coarse plan classification.
    pub plan_kind: PlanKind,
    /// Additional statements to execute after the main query (e.g., SELECT FOUND_ROWS()).
    pub post_statements: Vec<String>,
    /// Catalog entries dirtied by successful DDL post-statements.
    pub catalog_invalidations: Vec<CatalogInvalidation>,
    /// Simple target table for the first DML statement, captured from the
    /// parsed MySQL AST before PostgreSQL rewrite/emission.
    pub dml_target_table: Option<String>,
    /// MySQL-visible warnings synthesized by translation choices.
    pub warnings: Vec<PlanWarning>,
    /// Plan-level flags.
    pub flags: PlanFlags,
    /// Column index for RETURNING clause (INSERT/UPDATE with LAST_INSERT_ID).
    pub returning_column: Option<String>,
    /// True when the rewriter auto-appended a RETURNING clause for proxy behavior.
    pub auto_appended_returning: bool,
}

/// Result of parameterizing a raw MySQL query.
#[derive(Debug, Clone)]
pub struct ParameterizedQuery {
    /// The query template with literals replaced by `?` placeholders.
    pub template_sql: String,
    /// Extracted literal values in order.
    pub params: Vec<Value>,
    /// Total count of `?` placeholders in `template_sql`, including
    /// both the ones produced by literal extraction (positions in
    /// `params`) AND any `?` already present in the input SQL (from
    /// a client prepared statement). The binding vector at execute
    /// time is always sized to this value, so per-parameter metadata
    /// — e.g. `TranslatedPlan::param_coercions` — must also be sized
    /// to `total_placeholders` rather than `params.len()`.
    pub total_placeholders: usize,
}
