use std::error::Error;

use bytes::Bytes;
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Timelike, Utc};
use darmok_types::{RowData, Value};
use tokio_postgres::Row;
use tokio_postgres::types::{FromSql, Type};

use crate::numeric::decode_numeric;

/// Errors identify the failed representation without including cell values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeValueError {
    #[error("unsupported native PostgreSQL type (OID {oid})")]
    UnsupportedType { oid: u32 },
    #[error("native value is outside the supported representation: {kind}")]
    OutOfRange { kind: &'static str },
    #[error("invalid native PostgreSQL representation: {kind}")]
    InvalidRepresentation { kind: &'static str },
    #[error("cannot decode native PostgreSQL column {column}")]
    ColumnDecode { column: usize },
    #[error("native PostgreSQL column {column}: {cause}")]
    ColumnValue {
        column: usize,
        #[source]
        cause: Box<NativeValueError>,
    },
}

pub(crate) fn invalid(kind: &'static str) -> NativeValueError {
    NativeValueError::InvalidRepresentation { kind }
}
pub(crate) fn out_of_range(kind: &'static str) -> NativeValueError {
    NativeValueError::OutOfRange { kind }
}

/// Decode a backend binary row using its actual PostgreSQL scalar types.
/// TIMESTAMPTZ is represented as UTC; no unvalidated session timezone is used.
/// Unsupported types are rejected even when their cell is SQL NULL.
pub fn decode_native_row_utc(row: &Row) -> Result<RowData, NativeValueError> {
    row.columns()
        .iter()
        .enumerate()
        .map(|(column, metadata)| {
            if !NativeValue::accepts(metadata.type_()) {
                return Err(NativeValueError::UnsupportedType {
                    oid: metadata.type_().oid(),
                });
            }
            row.try_get::<_, Option<NativeValue>>(column)
                .map(|value| value.map_or(Value::Null, |value| value.0))
                .map_err(|error| {
                    match error
                        .source()
                        .and_then(|cause| cause.downcast_ref::<NativeValueError>())
                    {
                        Some(cause) => NativeValueError::ColumnValue {
                            column,
                            cause: Box::new(cause.clone()),
                        },
                        None => NativeValueError::ColumnDecode { column },
                    }
                })
        })
        .collect()
}

struct NativeValue(Value);

impl<'a> FromSql<'a> for NativeValue {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let value = match *ty {
            Type::BOOL => Value::Bool(bool::from_sql(ty, raw)?),
            Type::INT2 => Value::Int(i64::from(i16::from_sql(ty, raw)?)),
            Type::INT4 => Value::Int(i64::from(i32::from_sql(ty, raw)?)),
            Type::INT8 => Value::Int(i64::from_sql(ty, raw)?),
            Type::OID => Value::UInt(u64::from(u32::from_sql(ty, raw)?)),
            Type::FLOAT4 => finite_float(f64::from(f32::from_sql(ty, raw)?))?,
            Type::FLOAT8 => finite_float(f64::from_sql(ty, raw)?)?,
            Type::NUMERIC => Value::Decimal(decode_numeric(raw)?.into_boxed_str()),
            Type::BYTEA => Value::Bytes(Bytes::copy_from_slice(raw)),
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::JSON => Value::String(
                std::str::from_utf8(raw)
                    .map_err(|_| invalid("UTF-8 text"))?
                    .into(),
            ),
            Type::JSONB => {
                let Some((&1, text)) = raw.split_first() else {
                    return Err(invalid("JSONB version").into());
                };
                Value::String(
                    std::str::from_utf8(text)
                        .map_err(|_| invalid("UTF-8 JSONB"))?
                        .into(),
                )
            }
            Type::DATE => date_value(NaiveDate::from_sql(ty, raw)?)?,
            Type::TIME => time_value(raw)?,
            Type::TIMESTAMP => datetime_value(NaiveDateTime::from_sql(ty, raw)?)?,
            Type::TIMESTAMPTZ => datetime_value(DateTime::<Utc>::from_sql(ty, raw)?.naive_utc())?,
            _ => return Err(NativeValueError::UnsupportedType { oid: ty.oid() }.into()),
        };
        Ok(Self(value))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(
            *ty,
            Type::BOOL
                | Type::INT2
                | Type::INT4
                | Type::INT8
                | Type::OID
                | Type::FLOAT4
                | Type::FLOAT8
                | Type::NUMERIC
                | Type::BYTEA
                | Type::TEXT
                | Type::VARCHAR
                | Type::BPCHAR
                | Type::NAME
                | Type::JSON
                | Type::JSONB
                | Type::DATE
                | Type::TIME
                | Type::TIMESTAMP
                | Type::TIMESTAMPTZ
        )
    }
}

fn finite_float(value: f64) -> Result<Value, NativeValueError> {
    if !value.is_finite() {
        return Err(out_of_range("non-finite floating point"));
    }
    Ok(Value::Float(value))
}

fn date_value(date: NaiveDate) -> Result<Value, NativeValueError> {
    if !(1..=9999).contains(&date.year()) {
        return Err(out_of_range("date year 1..9999"));
    }
    Ok(Value::Date {
        year: date.year() as u16,
        month: date.month() as u8,
        day: date.day() as u8,
    })
}

fn datetime_value(datetime: NaiveDateTime) -> Result<Value, NativeValueError> {
    if !(1..=9999).contains(&datetime.year()) {
        return Err(out_of_range("datetime year 1..9999"));
    }
    Ok(Value::DateTime {
        year: datetime.year() as u16,
        month: datetime.month() as u8,
        day: datetime.day() as u8,
        hour: datetime.hour() as u8,
        minute: datetime.minute() as u8,
        second: datetime.second() as u8,
        micros: datetime.nanosecond() / 1000,
    })
}

fn time_value(raw: &[u8]) -> Result<Value, NativeValueError> {
    // PostgreSQL TIME includes 24:00:00. Chrono's NaiveTime decoder wraps that
    // endpoint to midnight, so retain its duration explicitly instead.
    const DAY_MICROS: i64 = 86_400_000_000;
    let micros = i64::from_be_bytes(raw.try_into().map_err(|_| invalid("TIME width"))?);
    if !(0..=DAY_MICROS).contains(&micros) {
        return Err(out_of_range("TIME 00:00:00..24:00:00"));
    }
    let seconds = micros / 1_000_000;
    Ok(Value::Time {
        negative: false,
        days: (seconds / 86_400) as u32,
        hours: ((seconds / 3600) % 24) as u8,
        minutes: ((seconds / 60) % 60) as u8,
        seconds: (seconds % 60) as u8,
        micros: (micros % 1_000_000) as u32,
    })
}
