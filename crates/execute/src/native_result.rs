use std::fmt::{self, Write};

use bytes::{BufMut, BytesMut};
use darmok_protocol::{ColumnDefinition, encode_binary_row, write_lenenc_bytes};
use darmok_session::charset::lookup_charset;
use darmok_types::Value;
use darmok_types::mysql_const::{charset, column_flag, field_type};
use tokio_postgres::{Row, types::Type};

use crate::{NativeStatementError, NativeStatementUtc};

/// Row payload format, independent of packet framing and result completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRowFormat {
    Text,
    Binary,
}

/// Indices are zero-based. Errors describe representations, not cell contents.
#[derive(Debug, thiserror::Error)]
pub enum NativeResultError {
    #[error("a native result set must have at least one column")]
    EmptyDescription,
    #[error("frontend result column count mismatch: expected {expected}, received {actual}")]
    ColumnCount { expected: usize, actual: usize },
    #[error("unsupported frontend representation for native column {column}: {reason}")]
    Metadata { column: usize, reason: &'static str },
    #[error("native column {column} cannot use its frontend representation: {reason}")]
    Encoding { column: usize, reason: &'static str },
    #[error(transparent)]
    Statement(#[from] NativeStatementError),
    #[error(transparent)]
    Protocol(#[from] darmok_types::ProxyError),
}

/// A checked native description bound to the exact immutable frontend metadata.
/// This represents output only: it cannot admit SQL, execute, or finish a scope.
#[derive(Clone, Copy)]
pub struct NativeResultUtc<'statement, 'columns> {
    description: NativeStatementUtc<'statement>,
    columns: &'columns [ColumnDefinition],
}

impl fmt::Debug for NativeResultUtc<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeResultUtc")
            .field("description", &self.description)
            .field("columns", &self.columns.len())
            .finish()
    }
}

impl<'statement, 'columns> NativeResultUtc<'statement, 'columns> {
    /// Check encoding metadata before Bind/Execute, including for empty results
    /// and NULL-only columns. Names, origins and semantic admission are inputs,
    /// not facts derived by this component from a native RowDescription.
    pub fn new(
        description: NativeStatementUtc<'statement>,
        columns: &'columns [ColumnDefinition],
    ) -> Result<Self, NativeResultError> {
        let native = description.statement().columns();
        if native.is_empty() {
            return Err(NativeResultError::EmptyDescription);
        }
        if native.len() != columns.len() {
            return Err(NativeResultError::ColumnCount {
                expected: native.len(),
                actual: columns.len(),
            });
        }
        for (index, (native, column)) in native.iter().zip(columns).enumerate() {
            check_metadata(native.type_(), column).map_err(|reason| {
                NativeResultError::Metadata {
                    column: index,
                    reason,
                }
            })?;
        }
        Ok(Self {
            description,
            columns,
        })
    }

    pub fn columns(&self) -> &'columns [ColumnDefinition] {
        self.columns
    }

    /// Validate native decoding and every advertised representation before
    /// writing a row payload. Any error preserves the original destination.
    /// The caller must keep its statement scope open until output and native
    /// completion are validated; encoding a row is not permission to commit.
    pub fn encode_row(
        &self,
        row: &Row,
        format: NativeRowFormat,
        dst: &mut BytesMut,
    ) -> Result<(), NativeResultError> {
        let values = self.description.decode_row(row)?;
        for (index, (value, column)) in values.iter().zip(self.columns).enumerate() {
            check_value(value, column).map_err(|reason| NativeResultError::Encoding {
                column: index,
                reason,
            })?;
        }
        let start = dst.len();
        let result = match format {
            NativeRowFormat::Binary => {
                encode_binary_row(&values, self.columns, dst).map_err(NativeResultError::Protocol)
            }
            NativeRowFormat::Text => encode_text_values(&values, self.columns, dst),
        };
        if result.is_err() {
            dst.truncate(start);
        }
        result
    }
}

fn integer_kind(kind: u8) -> bool {
    matches!(
        kind,
        field_type::TINY
            | field_type::SHORT
            | field_type::LONG
            | field_type::INT24
            | field_type::LONGLONG
            | field_type::YEAR
    )
}

fn string_kind(kind: u8) -> bool {
    matches!(
        kind,
        field_type::STRING | field_type::VAR_STRING | field_type::VARCHAR | field_type::BLOB
    )
}

fn check_metadata(ty: &Type, column: &ColumnDefinition) -> Result<(), &'static str> {
    let kind = column.column_type;
    if column.flags & (column_flag::ZEROFILL | column_flag::ENUM | column_flag::SET) != 0 {
        return Err("ZEROFILL, ENUM and SET output is not supported");
    }
    if column.flags & column_flag::UNSIGNED != 0
        && !integer_kind(kind)
        && !matches!(kind, field_type::DECIMAL | field_type::NEWDECIMAL)
    {
        return Err("UNSIGNED requires an integer or decimal representation");
    }
    if kind == field_type::NULL {
        if column.flags & column_flag::NOT_NULL != 0 {
            return Err("NULL representation cannot be NOT NULL");
        }
        return binary_metadata(column, 0);
    }
    let compatible = match *ty {
        Type::BOOL => kind == field_type::TINY,
        Type::INT2 | Type::INT4 | Type::INT8 | Type::OID => integer_kind(kind),
        Type::FLOAT4 | Type::FLOAT8 => matches!(kind, field_type::FLOAT | field_type::DOUBLE),
        Type::NUMERIC => matches!(kind, field_type::DECIMAL | field_type::NEWDECIMAL),
        Type::BYTEA | Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => string_kind(kind),
        Type::JSON | Type::JSONB => string_kind(kind) || kind == field_type::JSON,
        Type::DATE => kind == field_type::DATE,
        Type::TIME => kind == field_type::TIME,
        // TIMESTAMP output's frontend range and session timezone policy are
        // separate gates. Native timestamp components are explicit DATETIME.
        Type::TIMESTAMP | Type::TIMESTAMPTZ => kind == field_type::DATETIME,
        _ => false,
    };
    if !compatible {
        return Err("native type and frontend field type do not match");
    }
    match kind {
        field_type::FLOAT | field_type::DOUBLE => binary_metadata(column, 31),
        field_type::DECIMAL | field_type::NEWDECIMAL => {
            if column.character_set != charset::BINARY {
                return Err("numeric output requires binary charset metadata");
            }
            decimal_precision(column).map(|_| ())
        }
        field_type::TIME | field_type::DATETIME => {
            if column.character_set != charset::BINARY || column.decimals > 6 {
                return Err(
                    "temporal output requires binary charset and fractional precision 0..6",
                );
            }
            Ok(())
        }
        field_type::JSON => binary_metadata(column, 0),
        _ if string_kind(kind) => {
            if !matches!(column.decimals, 0 | 31) {
                return Err("string output requires decimals 0 or 31");
            }
            if *ty == Type::BYTEA {
                if column.character_set != charset::BINARY {
                    return Err("BYTEA requires binary charset metadata");
                }
            } else {
                match lookup_charset(column.character_set).map(|info| info.name) {
                    Some("binary" | "utf8mb3" | "utf8mb4") => {}
                    _ => return Err("native text output charset has no supported encoding"),
                }
            }
            Ok(())
        }
        _ => binary_metadata(column, 0),
    }
}

fn binary_metadata(column: &ColumnDefinition, decimals: u8) -> Result<(), &'static str> {
    if column.character_set != charset::BINARY || column.decimals != decimals {
        return Err("field requires binary charset and its supported decimals value");
    }
    Ok(())
}

fn decimal_precision(column: &ColumnDefinition) -> Result<usize, &'static str> {
    let extra =
        u32::from(column.decimals != 0) + u32::from(column.flags & column_flag::UNSIGNED == 0);
    let precision = column
        .column_length
        .checked_sub(extra)
        .ok_or("decimal metadata length is smaller than sign and decimal point")?;
    if !(1..=65).contains(&precision)
        || column.decimals > 30
        || u32::from(column.decimals) > precision
    {
        return Err("decimal metadata requires precision 1..65 and scale 0..min(precision,30)");
    }
    Ok(precision as usize)
}

fn check_value(value: &Value, column: &ColumnDefinition) -> Result<(), &'static str> {
    if matches!(value, Value::Null) {
        return if column.flags & column_flag::NOT_NULL != 0 {
            Err("NULL contradicts NOT NULL metadata")
        } else {
            Ok(())
        };
    }
    let kind = column.column_type;
    if integer_kind(kind) {
        let value = match value {
            Value::Bool(value) => i128::from(*value),
            Value::Int(value) => i128::from(*value),
            Value::UInt(value) => i128::from(*value),
            _ => return Err("integer representation requires an integer value"),
        };
        if kind == field_type::YEAR {
            return if value == 0 || (1901..=2155).contains(&value) {
                Ok(())
            } else {
                Err("value is outside YEAR 0 or 1901..2155")
            };
        }
        let bits = match kind {
            field_type::TINY => 8,
            field_type::SHORT => 16,
            field_type::INT24 => 24,
            field_type::LONG => 32,
            field_type::LONGLONG => 64,
            _ => return Err("unsupported integer width"),
        };
        let (min, max) = if column.flags & column_flag::UNSIGNED != 0 {
            (0, (1_i128 << bits) - 1)
        } else {
            (-(1_i128 << (bits - 1)), (1_i128 << (bits - 1)) - 1)
        };
        return if (min..=max).contains(&value) {
            Ok(())
        } else {
            Err("integer is outside its advertised width or signedness")
        };
    }
    match (kind, value) {
        (field_type::FLOAT | field_type::DOUBLE, Value::Float(value)) => {
            if !value.is_finite() {
                return Err("non-finite floating point output is not supported");
            }
            if kind == field_type::FLOAT && f64::from(*value as f32).to_bits() != value.to_bits() {
                return Err("FLOAT output would change the native value");
            }
            Ok(())
        }
        (field_type::DECIMAL | field_type::NEWDECIMAL, Value::Decimal(value)) => {
            let precision = decimal_precision(column)?;
            if value.starts_with('-') && column.flags & column_flag::UNSIGNED != 0 {
                return Err("negative decimal contradicts UNSIGNED metadata");
            }
            let unsigned = value.strip_prefix('-').unwrap_or(value);
            let (integer, fractional) = unsigned.split_once('.').unwrap_or((unsigned, ""));
            if fractional.len() != usize::from(column.decimals) {
                return Err("decimal output scale differs from its metadata");
            }
            if integer.trim_start_matches('0').len() > precision - fractional.len() {
                return Err("decimal is outside its advertised precision");
            }
            Ok(())
        }
        (field_type::DATE, Value::Date { .. }) => Ok(()),
        (field_type::TIME, Value::Time { micros, .. })
        | (field_type::DATETIME, Value::DateTime { micros, .. }) => {
            if micros % 10_u32.pow(u32::from(6 - column.decimals)) != 0 {
                return Err("temporal output would lose fractional precision");
            }
            Ok(())
        }
        (_, Value::String(value)) if string_kind(kind) || kind == field_type::JSON => {
            if value.len() as u64 > u64::from(column.column_length) {
                return Err("text exceeds its advertised byte length");
            }
            if lookup_charset(column.character_set).is_some_and(|info| info.name == "utf8mb3")
                && value.chars().any(|ch| u32::from(ch) > 0xffff)
            {
                return Err("text cannot be represented in utf8mb3");
            }
            Ok(())
        }
        (_, Value::Bytes(value)) if string_kind(kind) => {
            if value.len() as u64 > u64::from(column.column_length) {
                return Err("binary data exceeds its advertised byte length");
            }
            Ok(())
        }
        _ => Err("value does not match its advertised field type"),
    }
}

// Rust Display for a finite f64 needs at most 327 bytes (a negative minimum
// subnormal in fixed decimal notation). All other formatted native scalars are
// shorter. One checked stack buffer is reused, never allocated per cell.
struct ScalarText {
    bytes: [u8; 384],
    len: usize,
}

impl Write for ScalarText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        let dst = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        dst.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

fn encode_text_values(
    values: &[Value],
    columns: &[ColumnDefinition],
    dst: &mut BytesMut,
) -> Result<(), NativeResultError> {
    let mut text = ScalarText {
        bytes: [0; 384],
        len: 0,
    };
    for (index, (value, column)) in values.iter().zip(columns).enumerate() {
        match value {
            Value::Null => dst.put_u8(0xfb),
            Value::String(value) | Value::Decimal(value) => {
                write_lenenc_bytes(dst, value.as_bytes())
            }
            Value::Bytes(value) => write_lenenc_bytes(dst, value),
            _ => {
                text.len = 0;
                format_scalar(value, column, &mut text).map_err(|_| {
                    NativeResultError::Encoding {
                        column: index,
                        reason: "scalar text does not fit its supported representation",
                    }
                })?;
                write_lenenc_bytes(dst, &text.bytes[..text.len]);
            }
        }
    }
    Ok(())
}

fn format_scalar(value: &Value, column: &ColumnDefinition, dst: &mut ScalarText) -> fmt::Result {
    match value {
        Value::Bool(value) => write!(dst, "{}", u8::from(*value)),
        Value::Int(value) if column.column_type == field_type::YEAR => write!(dst, "{value:04}"),
        Value::UInt(value) if column.column_type == field_type::YEAR => write!(dst, "{value:04}"),
        Value::Int(value) => write!(dst, "{value}"),
        Value::UInt(value) => write!(dst, "{value}"),
        Value::Float(value) if column.column_type == field_type::FLOAT => {
            write!(dst, "{}", *value as f32)
        }
        Value::Float(value) => write!(dst, "{value}"),
        Value::Date { year, month, day } => write!(dst, "{year:04}-{month:02}-{day:02}"),
        Value::Time {
            negative,
            days,
            hours,
            minutes,
            seconds,
            micros,
        } => {
            if *negative {
                dst.write_char('-')?;
            }
            let hours = u64::from(*days) * 24 + u64::from(*hours);
            write!(dst, "{hours:02}:{minutes:02}:{seconds:02}")?;
            format_fraction(*micros, column.decimals, dst)
        }
        Value::DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            micros,
        } => {
            write!(
                dst,
                "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
            )?;
            format_fraction(*micros, column.decimals, dst)
        }
        _ => Err(fmt::Error),
    }
}

fn format_fraction(micros: u32, scale: u8, dst: &mut ScalarText) -> fmt::Result {
    if scale == 0 {
        return Ok(());
    }
    let fractional = micros / 10_u32.pow(u32::from(6 - scale));
    write!(dst, ".{fractional:0width$}", width = usize::from(scale))
}
