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
    ZeroDateToSentinel,
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

impl TranslatedPlan {
    /// Count distinct PostgreSQL bind parameters referenced by this plan.
    pub fn parameter_count(&self) -> u16 {
        let bytes = self.pg_sql.as_bytes();
        let mut index = 0usize;
        let mut max_placeholder = 0u16;

        while index < bytes.len() {
            match bytes[index] {
                b'\'' => {
                    index += 1;
                    while index < bytes.len() {
                        if bytes[index] == b'\'' && bytes.get(index + 1).copied() == Some(b'\'') {
                            index += 2;
                            continue;
                        }
                        if bytes[index] == b'\'' {
                            index += 1;
                            break;
                        }
                        index += 1;
                    }
                }
                b'"' => {
                    index += 1;
                    while index < bytes.len() {
                        if bytes[index] == b'"' && bytes.get(index + 1).copied() == Some(b'"') {
                            index += 2;
                            continue;
                        }
                        if bytes[index] == b'"' {
                            index += 1;
                            break;
                        }
                        index += 1;
                    }
                }
                b'-' if bytes.get(index + 1).copied() == Some(b'-') => {
                    index += 2;
                    while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                        index += 1;
                    }
                }
                b'/' if bytes.get(index + 1).copied() == Some(b'*') => {
                    index += 2;
                    while index + 1 < bytes.len() {
                        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                            index += 2;
                            break;
                        }
                        index += 1;
                    }
                }
                b'$' => {
                    let start = index + 1;
                    let mut end = start;
                    while end < bytes.len() && bytes[end].is_ascii_digit() {
                        end += 1;
                    }

                    if end > start {
                        let placeholder =
                            self.pg_sql[start..end].parse::<u16>().unwrap_or(u16::MAX);
                        max_placeholder = max_placeholder.max(placeholder);
                        index = end;
                    } else {
                        index += 1;
                    }
                }
                _ => index += 1,
            }
        }

        max_placeholder
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan(pg_sql: &str) -> TranslatedPlan {
        TranslatedPlan {
            pg_sql: pg_sql.to_owned(),
            found_rows_count_sql: None,
            param_coercions: Vec::new(),
            projection: None,
            sidecar: None,
            plan_kind: PlanKind::Select,
            post_statements: Vec::new(),
            catalog_invalidations: Vec::new(),
            dml_target_table: None,
            warnings: Vec::new(),
            flags: PlanFlags::empty(),
            returning_column: None,
            auto_appended_returning: false,
        }
    }

    #[test]
    fn parameter_count_tracks_highest_placeholder_index() {
        let plan = sample_plan("SELECT $1, $3, $2, $3");
        assert_eq!(plan.parameter_count(), 3);
    }

    #[test]
    fn parameter_count_ignores_placeholders_in_strings_and_comments() {
        let plan = sample_plan("SELECT '$9', $2 /* $7 */ -- $8\nFROM t WHERE note = '$1'");
        assert_eq!(plan.parameter_count(), 2);
    }
}
