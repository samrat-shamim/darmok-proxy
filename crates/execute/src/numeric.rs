use std::fmt::Write;

use crate::native_value::{NativeValueError, invalid, out_of_range};

/// Decode PostgreSQL's base-10000 binary NUMERIC without floating point or
/// rounding. Preserve its display scale, within MySQL's 65-digit/30-scale range.
pub(crate) fn decode_numeric(raw: &[u8]) -> Result<String, NativeValueError> {
    let header = raw.get(..8).ok_or_else(|| invalid("NUMERIC header"))?;
    let digits = usize::from(u16::from_be_bytes([header[0], header[1]]));
    let weight = i32::from(i16::from_be_bytes([header[2], header[3]]));
    let sign = u16::from_be_bytes([header[4], header[5]]);
    let scale = usize::from(u16::from_be_bytes([header[6], header[7]]));
    if !matches!(sign, 0 | 0x4000) {
        return Err(out_of_range("non-finite NUMERIC"));
    }
    if scale > 30 {
        return Err(out_of_range("NUMERIC scale <= 30"));
    }
    if raw.len() != 8 + digits * 2 {
        return Err(invalid("NUMERIC digit count"));
    }
    if weight > 16 {
        return Err(out_of_range("NUMERIC precision <= 65"));
    }
    let groups = &raw[8..];
    for group in groups.chunks_exact(2) {
        if u16::from_be_bytes([group[0], group[1]]) >= 10_000 {
            return Err(invalid("NUMERIC base-10000 digit"));
        }
    }
    let group_at = |position: i32| -> u16 {
        let index = weight - position;
        if index < 0 || index as usize >= digits {
            return 0;
        }
        let offset = index as usize * 2;
        u16::from_be_bytes([groups[offset], groups[offset + 1]])
    };
    let mut output = String::with_capacity(68);
    if sign == 0x4000 && groups.iter().any(|byte| *byte != 0) {
        output.push('-');
    }
    let integer_start = output.len();
    if weight < 0 {
        output.push('0');
    } else {
        write!(output, "{}", group_at(weight)).expect("String formatting is infallible");
        for position in (0..weight).rev() {
            write!(output, "{:04}", group_at(position)).expect("String formatting is infallible");
        }
    }
    let integer_digits = output.len() - integer_start;
    // The zero before a decimal point is not a significant precision digit.
    let significant_integer_digits = if &output[integer_start..] == "0" {
        0
    } else {
        integer_digits
    };
    if significant_integer_digits + scale > 65 {
        return Err(out_of_range("NUMERIC precision <= 65"));
    }
    if scale != 0 {
        output.push('.');
        let start = output.len();
        for position in 1..=scale.div_ceil(4) {
            write!(output, "{:04}", group_at(-(position as i32)))
                .expect("String formatting is infallible");
        }
        // PostgreSQL numeric_send pads the final base-10000 group with zeros.
        // Removing those padding zeros preserves every significant digit.
        if output.as_bytes()[start + scale..]
            .iter()
            .any(|byte| *byte != b'0')
        {
            return Err(invalid("NUMERIC digits beyond display scale"));
        }
        output.truncate(start + scale);
    }
    Ok(output)
}
