//! Exact MySQL numeric literals, independent of native NUMERIC decoding.
//! Values borrow validated source digits until their final text-row allocation.

use bytes::Bytes;
use darmok_types::mysql_const::{column_flag as flag, field_type as ty};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumberKind {
    Signed,
    Unsigned,
    Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactNumberError;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExactNumber<'a> {
    integer: &'a str,
    fraction: &'a str,
    magnitude: Option<u64>,
    kind: NumberKind,
    negative: bool,
    zero: bool,
    width: u32,
    scale: u8,
    // Literal declarations retain their own precision. Derived numeric
    // expressions use MySQL's separate maximum-65 precision declaration.
    literal_precision: Option<u32>,
}

pub(crate) struct ExactNumberMetadata {
    pub(crate) mysql_type: u8,
    pub(crate) flags: u16,
    pub(crate) width: u32,
    pub(crate) scale: u8,
}

impl<'a> ExactNumber<'a> {
    pub(crate) fn from_token(source: &'a str) -> Result<Self, ExactNumberError> {
        let (integer, fraction, point) = match source.split_once('.') {
            Some((integer, fraction)) => (integer, fraction, true),
            None => (source, "", false),
        };
        if integer.is_empty() && fraction.is_empty()
            || !integer.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ExactNumberError);
        }
        let normalized = integer.trim_start_matches('0');
        if !point && let Ok(value) = source.parse::<u64>() {
            let unsigned = value > i64::MAX as u64;
            let precision = u32::try_from(source.len()).map_err(|_| ExactNumberError)?;
            let width = precision
                .checked_add(u32::from(!unsigned))
                .ok_or(ExactNumberError)?;
            return Ok(Self {
                integer: normalized,
                fraction,
                magnitude: Some(value),
                kind: if unsigned {
                    NumberKind::Unsigned
                } else {
                    NumberKind::Signed
                },
                negative: false,
                zero: value == 0,
                width,
                scale: 0,
                literal_precision: Some(precision),
            });
        }
        // This is the exact literal contract, not the 30-scale column/native
        // NUMERIC contract. Fractional zeros remain part of the display scale.
        if normalized
            .len()
            .checked_add(fraction.len())
            .ok_or(ExactNumberError)?
            > 65
        {
            return Err(ExactNumberError);
        }
        let scale = u8::try_from(fraction.len()).map_err(|_| ExactNumberError)?;
        // MySQL decimal literal declarations retain one leading integer zero,
        // while their row text removes all redundant leading integer zeros.
        let declared_integer = normalized.len() + usize::from(integer.starts_with('0'));
        let precision =
            u32::try_from(declared_integer + fraction.len()).map_err(|_| ExactNumberError)?;
        let width = precision + u32::from(scale != 0) + 1;
        Ok(Self {
            integer: normalized,
            fraction,
            magnitude: None,
            kind: NumberKind::Decimal,
            negative: false,
            zero: normalized.is_empty() && fraction.bytes().all(|byte| byte == b'0'),
            width,
            scale,
            literal_precision: Some(precision),
        })
    }

    pub(crate) fn negate(mut self) -> Result<Self, ExactNumberError> {
        let previous_kind = self.kind;
        let new_kind = match previous_kind {
            NumberKind::Unsigned => {
                if self.magnitude.ok_or(ExactNumberError)? <= 1_u64 << 63 {
                    NumberKind::Signed
                } else {
                    NumberKind::Decimal
                }
            }
            // MySQL promotes every nonzero negative signed constant operand,
            // even when its mathematical negation would fit a signed integer.
            // The result's declared family is distinct from its value range.
            NumberKind::Signed if self.negative => NumberKind::Decimal,
            kind => kind,
        };
        if new_kind == NumberKind::Decimal && previous_kind != NumberKind::Decimal {
            let precision = self.literal_precision.unwrap_or_else(|| {
                (self.width - u32::from(previous_kind != NumberKind::Unsigned)).clamp(1, 65)
            });
            self.width = precision.checked_add(1).ok_or(ExactNumberError)?;
        } else if previous_kind == NumberKind::Unsigned {
            self.width = self.width.checked_add(1).ok_or(ExactNumberError)?;
        }
        self.kind = new_kind;
        self.literal_precision = None;
        self.negative = !self.zero && !self.negative;
        Ok(self)
    }

    pub(crate) fn metadata(self) -> ExactNumberMetadata {
        ExactNumberMetadata {
            mysql_type: if self.kind == NumberKind::Decimal {
                ty::NEWDECIMAL
            } else {
                ty::LONGLONG
            },
            flags: flag::NOT_NULL
                | flag::BINARY
                | if self.kind == NumberKind::Unsigned {
                    flag::UNSIGNED
                } else {
                    0
                },
            width: self.width,
            scale: self.scale,
        }
    }

    pub(crate) fn into_text(self) -> Bytes {
        let mut result = String::with_capacity(
            self.integer.len().max(1)
                + self.fraction.len()
                + usize::from(self.scale != 0)
                + usize::from(self.negative),
        );
        if self.negative {
            result.push('-');
        }
        result.push_str(if self.integer.is_empty() {
            "0"
        } else {
            self.integer
        });
        if self.scale != 0 {
            result.push('.');
            result.push_str(self.fraction);
        }
        Bytes::from(result)
    }
}
