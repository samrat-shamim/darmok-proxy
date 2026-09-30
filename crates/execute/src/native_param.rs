use std::error::Error;
use std::fmt::{self, Write};

use bytes::{BufMut, BytesMut};
use chrono::{NaiveDate, NaiveDateTime};
use darmok_types::Value;
use tokio_postgres::types::{Format, IsNull, ToSql, Type, to_sql_checked};

use crate::native_value::supported_native_type;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeParamError {
    #[error("unsupported native PostgreSQL parameter type (OID {oid})")]
    UnsupportedType { oid: u32 },
    #[error("proxy value cannot represent native PostgreSQL parameter type (OID {oid})")]
    TypeMismatch { oid: u32 },
    #[error("parameter is outside the supported native representation: {kind}")]
    OutOfRange { kind: &'static str },
    #[error("invalid native parameter value: {kind}")]
    InvalidValue { kind: &'static str },
}

fn out_of_range(kind: &'static str) -> NativeParamError {
    NativeParamError::OutOfRange { kind }
}

fn invalid(kind: &'static str) -> NativeParamError {
    NativeParamError::InvalidValue { kind }
}

/// Borrowed native parameter encoding, with DateTime-to-TIMESTAMPTZ explicitly
/// interpreted as UTC. MySQL coercion policies belong outside this component.
#[derive(Clone, Copy)]
pub struct NativeParamUtc<'a> {
    value: &'a Value,
}

impl<'a> NativeParamUtc<'a> {
    pub const fn new(value: &'a Value) -> Self {
        Self { value }
    }
}

impl fmt::Debug for NativeParamUtc<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NativeParamUtc(..)")
    }
}

impl ToSql for NativeParamUtc<'_> {
    fn to_sql(
        &self,
        ty: &Type,
        output: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        if !Self::accepts(ty) {
            return Err(NativeParamError::UnsupportedType { oid: ty.oid() }.into());
        }
        if matches!(self.value, Value::Null) {
            return Ok(IsNull::Yes);
        }
        match *ty {
            Type::BOOL => {
                let Value::Bool(value) = self.value else {
                    return Err(NativeParamError::TypeMismatch { oid: ty.oid() }.into());
                };
                return value.to_sql(ty, output);
            }
            Type::INT2 | Type::INT4 | Type::INT8 | Type::OID => {
                let value = integer(self.value, ty)?;
                let range_error = || out_of_range("integer target range");
                return match *ty {
                    Type::INT2 => i16::try_from(value)
                        .map_err(|_| range_error())?
                        .to_sql(ty, output),
                    Type::INT4 => i32::try_from(value)
                        .map_err(|_| range_error())?
                        .to_sql(ty, output),
                    Type::INT8 => i64::try_from(value)
                        .map_err(|_| range_error())?
                        .to_sql(ty, output),
                    Type::OID => u32::try_from(value)
                        .map_err(|_| range_error())?
                        .to_sql(ty, output),
                    _ => unreachable!("integer target matched above"),
                };
            }
            Type::FLOAT4 | Type::FLOAT8 => {
                let Value::Float(value) = self.value else {
                    return Err(NativeParamError::TypeMismatch { oid: ty.oid() }.into());
                };
                if !value.is_finite() {
                    return Err(out_of_range("finite floating point").into());
                }
                if *ty == Type::FLOAT4 {
                    let single = *value as f32;
                    if f64::from(single) != *value {
                        return Err(out_of_range("exact REAL representation").into());
                    }
                    return single.to_sql(ty, output);
                }
                return value.to_sql(ty, output);
            }
            Type::NUMERIC => match self.value {
                Value::Int(value) => {
                    write!(output, "{value}").expect("BytesMut formatting is infallible");
                }
                Value::UInt(value) => {
                    write!(output, "{value}").expect("BytesMut formatting is infallible");
                }
                Value::Decimal(value) => decimal_parameter(value, output)?,
                _ => return Err(NativeParamError::TypeMismatch { oid: ty.oid() }.into()),
            },
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::JSON | Type::JSONB => {
                let Value::String(value) = self.value else {
                    return Err(NativeParamError::TypeMismatch { oid: ty.oid() }.into());
                };
                if value.contains('\0') {
                    return Err(invalid("PostgreSQL text contains NUL").into());
                }
                output.extend_from_slice(value.as_bytes());
            }
            Type::BYTEA => {
                let Value::Bytes(value) = self.value else {
                    return Err(NativeParamError::TypeMismatch { oid: ty.oid() }.into());
                };
                output.extend_from_slice(value);
            }
            Type::DATE => {
                let Value::Date { year, month, day } = *self.value else {
                    return Err(NativeParamError::TypeMismatch { oid: ty.oid() }.into());
                };
                return date(year, month, day)?.to_sql(ty, output);
            }
            Type::TIME => {
                let Value::Time {
                    negative,
                    days,
                    hours,
                    minutes,
                    seconds,
                    micros,
                } = *self.value
                else {
                    return Err(NativeParamError::TypeMismatch { oid: ty.oid() }.into());
                };
                if negative || hours >= 24 || minutes >= 60 || seconds >= 60 || micros >= 1_000_000
                {
                    return Err(out_of_range("native TIME fields").into());
                }
                if days > 1
                    || (days == 1 && (hours != 0 || minutes != 0 || seconds != 0 || micros != 0))
                {
                    return Err(out_of_range("TIME 00:00:00..24:00:00").into());
                }
                let total_seconds = i64::from(days) * 86_400
                    + i64::from(hours) * 3600
                    + i64::from(minutes) * 60
                    + i64::from(seconds);
                output.put_i64(total_seconds * 1_000_000 + i64::from(micros));
            }
            Type::TIMESTAMP | Type::TIMESTAMPTZ => {
                let value = datetime(self.value, ty)?;
                if *ty == Type::TIMESTAMPTZ {
                    return value.and_utc().to_sql(ty, output);
                }
                return value.to_sql(ty, output);
            }
            _ => return Err(NativeParamError::UnsupportedType { oid: ty.oid() }.into()),
        }
        Ok(IsNull::No)
    }

    fn accepts(ty: &Type) -> bool {
        supported_native_type(ty)
    }

    fn encode_format(&self, ty: &Type) -> Format {
        if matches!(*ty, Type::NUMERIC | Type::JSON | Type::JSONB) {
            Format::Text
        } else {
            Format::Binary
        }
    }

    to_sql_checked!();
}

fn integer(value: &Value, ty: &Type) -> Result<i128, NativeParamError> {
    match value {
        Value::Int(value) => Ok(i128::from(*value)),
        Value::UInt(value) => Ok(i128::from(*value)),
        _ => Err(NativeParamError::TypeMismatch { oid: ty.oid() }),
    }
}

fn date(year: u16, month: u8, day: u8) -> Result<NaiveDate, NativeParamError> {
    if !(1..=9999).contains(&year) {
        return Err(out_of_range("date year 1..9999"));
    }
    NaiveDate::from_ymd_opt(i32::from(year), u32::from(month), u32::from(day))
        .ok_or_else(|| invalid("Gregorian date"))
}

fn datetime(value: &Value, ty: &Type) -> Result<NaiveDateTime, NativeParamError> {
    let Value::DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        micros,
    } = *value
    else {
        return Err(NativeParamError::TypeMismatch { oid: ty.oid() });
    };
    if micros >= 1_000_000 {
        return Err(out_of_range("datetime microseconds < 1000000"));
    }
    date(year, month, day)?
        .and_hms_micro_opt(
            u32::from(hour),
            u32::from(minute),
            u32::from(second),
            micros,
        )
        .ok_or_else(|| invalid("datetime clock fields"))
}

fn decimal_parameter(value: &str, output: &mut BytesMut) -> Result<(), NativeParamError> {
    let (negative, unsigned) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let (integer, fraction) = match unsigned.split_once('.') {
        Some((integer, fraction)) => {
            if fraction.is_empty() {
                return Err(invalid("plain decimal literal"));
            }
            (integer, fraction)
        }
        None => (unsigned, ""),
    };
    if integer.is_empty()
        || !integer.bytes().all(|digit| digit.is_ascii_digit())
        || !fraction.bytes().all(|digit| digit.is_ascii_digit())
    {
        return Err(invalid("plain decimal literal"));
    }
    let integer = integer.trim_start_matches('0');
    if fraction.len() > 30 {
        return Err(out_of_range("NUMERIC scale <= 30"));
    }
    if integer.len() + fraction.len() > 65 {
        return Err(out_of_range("NUMERIC precision <= 65"));
    }
    if negative {
        output.put_u8(b'-');
    }
    output.extend_from_slice(if integer.is_empty() {
        b"0"
    } else {
        integer.as_bytes()
    });
    if !fraction.is_empty() {
        output.put_u8(b'.');
        output.extend_from_slice(fraction.as_bytes());
    }
    Ok(())
}
