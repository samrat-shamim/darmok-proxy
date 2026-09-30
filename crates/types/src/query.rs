use std::sync::Arc;

use crate::value::Value;
use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSource {
    /// PostgreSQL schema used for schema-cache lookups. Proxy-side only.
    pub schema: Arc<str>,
    /// PostgreSQL table used for schema-cache lookups. Proxy-side only.
    pub table: Arc<str>,
    /// PostgreSQL column name as returned by the backend.
    pub pg_column_name: Arc<str>,
}

/// Metadata for a single column in a result set.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnMeta {
    pub name: Box<str>,
    pub table: Option<Box<str>>,
    /// Optional backend source information carried alongside the wire metadata.
    /// This is not encoded onto the MySQL wire.
    pub source: Option<ColumnSource>,
    /// MySQL field type (see `mysql_const::field_type`).
    pub mysql_type: u8,
    /// MySQL column flags (see `mysql_const::column_flag`).
    pub flags: u16,
    /// MySQL charset id (see `mysql_const::charset`).
    pub charset_id: u16,
    /// Maximum display length in bytes.
    pub max_length: u32,
    /// Number of decimal digits.
    pub decimals: u8,
}

/// Per-column coercion rule for result mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultCoercion {
    None,
    BoolToTinyInt,
    TimestamptzToDatetime,
    JsonbNormalize,
    NumericToDecimal { scale: u8 },
    ByteaToBlob,
    ZeroDateSentinelToMysql,
    FallbackToText,
}

/// A single row of values.
pub type RowData = Vec<Value>;

/// A single row already encoded as MySQL text-protocol cell payloads.
///
/// `None` represents SQL NULL. `Some(bytes)` is the cell payload before the
/// length-encoded prefix is written by the packet layer.
pub type TextRowData = Vec<Option<Bytes>>;

/// The result of executing a translated query against PostgreSQL.
///
/// Note: streaming result sets are handled at the execute layer (not in this
/// type). This enum represents the fully-materialized result.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryResult {
    /// A statement that does not return rows (INSERT, UPDATE, DELETE, etc.).
    Ok {
        affected_rows: u64,
        /// Value encoded in the OK packet's last-insert-id slot.
        last_insert_id: u64,
        /// Value that should replace the frontend session's
        /// LAST_INSERT_ID() state. MySQL keeps this separate from the OK
        /// packet value for explicit-id INSERT/REPLACE: those packets expose
        /// the explicit id, but LAST_INSERT_ID() remains unchanged.
        session_last_insert_id: Option<u64>,
        status_flags: u16,
        warnings: u16,
    },
    /// A statement that returns a result set (SELECT, SHOW, etc.).
    ResultSet {
        columns: Vec<ColumnMeta>,
        rows: Vec<RowData>,
    },
    /// A result set materialized for COM_QUERY text-protocol encoding.
    TextResultSet {
        columns: Vec<ColumnMeta>,
        rows: Vec<TextRowData>,
    },
    /// Metadata-only result for streamed result sets after all rows have been
    /// sent to the client.
    ResultSetSummary {
        columns: Vec<ColumnMeta>,
        row_count: u64,
    },
}

/// Column metadata for a projected result set (used by the protocol layer).
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionMeta {
    pub columns: Vec<ColumnMeta>,
    pub coercions: Vec<ResultCoercion>,
}

/// Extra per-column metadata obtained from PostgreSQL (nullable info, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarMeta {
    pub nullable: Vec<bool>,
    pub column_flags: Vec<u16>,
    pub pg_oids: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mysql_const;

    fn round_trip(coercion: ResultCoercion) -> ResultCoercion {
        match coercion {
            ResultCoercion::None => ResultCoercion::None,
            ResultCoercion::BoolToTinyInt => ResultCoercion::BoolToTinyInt,
            ResultCoercion::TimestamptzToDatetime => ResultCoercion::TimestamptzToDatetime,
            ResultCoercion::JsonbNormalize => ResultCoercion::JsonbNormalize,
            ResultCoercion::NumericToDecimal { scale } => {
                ResultCoercion::NumericToDecimal { scale }
            }
            ResultCoercion::ByteaToBlob => ResultCoercion::ByteaToBlob,
            ResultCoercion::ZeroDateSentinelToMysql => ResultCoercion::ZeroDateSentinelToMysql,
            ResultCoercion::FallbackToText => ResultCoercion::FallbackToText,
        }
    }

    fn sample_column(name: &str, mysql_type: u8, flags: u16) -> ColumnMeta {
        ColumnMeta {
            name: name.into(),
            table: Some("widgets".into()),
            source: None,
            mysql_type,
            flags,
            charset_id: mysql_const::charset::UTF8MB4_GENERAL_CI,
            max_length: 255,
            decimals: 0,
        }
    }

    #[test]
    fn result_coercion_variants_round_trip() {
        let cases = [
            ResultCoercion::None,
            ResultCoercion::BoolToTinyInt,
            ResultCoercion::TimestamptzToDatetime,
            ResultCoercion::JsonbNormalize,
            ResultCoercion::NumericToDecimal { scale: 4 },
            ResultCoercion::ByteaToBlob,
            ResultCoercion::ZeroDateSentinelToMysql,
            ResultCoercion::FallbackToText,
        ];

        for coercion in cases {
            assert_eq!(round_trip(coercion), coercion);
        }
    }

    #[test]
    fn projection_and_sidecar_metadata_preserve_fields() {
        let columns = vec![
            sample_column(
                "id",
                mysql_const::field_type::LONG,
                mysql_const::column_flag::PRI_KEY,
            ),
            sample_column(
                "name",
                mysql_const::field_type::VAR_STRING,
                mysql_const::column_flag::NOT_NULL,
            ),
        ];

        let projection = ProjectionMeta {
            columns: columns.clone(),
            coercions: vec![ResultCoercion::None, ResultCoercion::FallbackToText],
        };
        let sidecar = SidecarMeta {
            nullable: vec![false, false],
            column_flags: vec![
                mysql_const::column_flag::PRI_KEY,
                mysql_const::column_flag::NOT_NULL,
            ],
            pg_oids: vec![23, 25],
        };

        assert_eq!(projection.columns, columns);
        assert_eq!(projection.columns[0].name.as_ref(), "id");
        assert_eq!(projection.columns[1].table.as_deref(), Some("widgets"));
        assert!(projection.columns[0].source.is_none());
        assert_eq!(projection.coercions[0], ResultCoercion::None);
        assert_eq!(projection.coercions[1], ResultCoercion::FallbackToText);

        assert_eq!(sidecar.nullable, vec![false, false]);
        assert_eq!(
            sidecar.column_flags,
            vec![
                mysql_const::column_flag::PRI_KEY,
                mysql_const::column_flag::NOT_NULL,
            ]
        );
        assert_eq!(sidecar.pg_oids, vec![23, 25]);
    }
}
