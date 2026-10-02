//! Binary SQL digits decode directly to bytes, preserving leading zero groups.
//! These are string-context values, not signed/unsigned numeric coercions.

use bytes::Bytes;

use crate::SelectSqlError;

pub(crate) fn hex(digits: &str) -> Result<Bytes, SelectSqlError> {
    decode(digits, 4)
}

pub(crate) fn bits(digits: &str) -> Result<Bytes, SelectSqlError> {
    decode(digits, 1)
}

fn decode(digits: &str, shift: u8) -> Result<Bytes, SelectSqlError> {
    let per_byte = usize::from(8 / shift);
    let width = digits.len().div_ceil(per_byte);
    // Declared protocol width must be representable before allocating output.
    u32::try_from(width).map_err(|_| SelectSqlError::Unsupported)?;
    let mut output = Vec::with_capacity(width);
    let mut offset = 0;
    while offset < digits.len() {
        let size = if offset == 0 && digits.len() % per_byte != 0 {
            digits.len() % per_byte
        } else {
            per_byte
        };
        let mut byte = 0u8;
        for &digit in &digits.as_bytes()[offset..offset + size] {
            let value = match digit {
                b'0'..=b'9' => digit - b'0',
                b'a'..=b'f' => digit - b'a' + 10,
                b'A'..=b'F' => digit - b'A' + 10,
                _ => return Err(SelectSqlError::Unsupported),
            };
            if value >= 1 << shift {
                return Err(SelectSqlError::Unsupported);
            }
            byte = (byte << shift) | value;
        }
        output.push(byte);
        offset += size;
    }
    Ok(Bytes::from(output))
}
