use bytes::{Buf, BufMut, BytesMut};
use darmok_types::error::{ProtocolError, ProxyError, Result};

pub(crate) fn ensure_remaining(cursor: &&[u8], needed: usize, description: &str) -> Result<()> {
    if cursor.remaining() < needed {
        Err(invalid_packet(format!("truncated {description}")))
    } else {
        Ok(())
    }
}

pub(crate) fn invalid_packet(message: impl Into<String>) -> ProxyError {
    ProxyError::protocol(ProtocolError::InvalidPacket(message.into()))
}

pub(crate) fn unsupported(message: impl Into<String>) -> ProxyError {
    ProxyError::protocol(ProtocolError::UnsupportedCapability(message.into()))
}

pub(crate) fn decode_utf8(bytes: Vec<u8>, description: &str) -> Result<String> {
    String::from_utf8(bytes)
        .map_err(|error| invalid_packet(format!("invalid UTF-8 in {description}: {error}")))
}

pub(crate) fn take_null_terminated(cursor: &mut &[u8], description: &str) -> Result<Vec<u8>> {
    let Some(position) = cursor.iter().position(|byte| *byte == 0) else {
        return Err(invalid_packet(format!(
            "missing null terminator in {description}"
        )));
    };

    let value = cursor[..position].to_vec();
    cursor.advance(position + 1);
    Ok(value)
}

pub(crate) fn read_u16_le(src: &[u8], description: &str) -> Result<u16> {
    if src.len() < 2 {
        return Err(invalid_packet(format!("truncated {description}")));
    }

    Ok(u16::from_le_bytes([src[0], src[1]]))
}

pub(crate) fn read_u32_le(src: &[u8], description: &str) -> Result<u32> {
    if src.len() < 4 {
        return Err(invalid_packet(format!("truncated {description}")));
    }

    Ok(u32::from_le_bytes([src[0], src[1], src[2], src[3]]))
}

pub fn read_lenenc_int(cursor: &mut &[u8]) -> Result<u64> {
    ensure_remaining(cursor, 1, "length-encoded integer")?;
    match cursor.get_u8() {
        value @ 0x00..=0xFA => Ok(u64::from(value)),
        0xFB => Err(invalid_packet(
            "NULL length-encoded integer marker is not valid here",
        )),
        0xFC => {
            ensure_remaining(cursor, 2, "length-encoded integer")?;
            Ok(u64::from(cursor.get_u16_le()))
        }
        0xFD => {
            ensure_remaining(cursor, 3, "length-encoded integer")?;
            let b0 = u64::from(cursor.get_u8());
            let b1 = u64::from(cursor.get_u8());
            let b2 = u64::from(cursor.get_u8());
            Ok(b0 | (b1 << 8) | (b2 << 16))
        }
        0xFE => {
            ensure_remaining(cursor, 8, "length-encoded integer")?;
            Ok(cursor.get_u64_le())
        }
        0xFF => Err(invalid_packet("invalid length-encoded integer marker")),
    }
}

pub fn read_lenenc_bytes(cursor: &mut &[u8], description: &str) -> Result<Vec<u8>> {
    let length = usize::try_from(read_lenenc_int(cursor)?)
        .map_err(|_| invalid_packet(format!("{description} length exceeds usize")))?;
    ensure_remaining(cursor, length, description)?;
    let bytes = cursor[..length].to_vec();
    cursor.advance(length);
    Ok(bytes)
}

pub fn write_lenenc_int(dst: &mut BytesMut, value: u64) {
    match value {
        0x00..=0xFA => dst.put_u8(value as u8),
        0xFB..=0xFFFF => {
            dst.put_u8(0xFC);
            dst.put_u16_le(value as u16);
        }
        0x1_0000..=0xFF_FFFF => {
            dst.put_u8(0xFD);
            dst.put_u8((value & 0xFF) as u8);
            dst.put_u8(((value >> 8) & 0xFF) as u8);
            dst.put_u8(((value >> 16) & 0xFF) as u8);
        }
        _ => {
            dst.put_u8(0xFE);
            dst.put_u64_le(value);
        }
    }
}

pub fn write_lenenc_bytes(dst: &mut BytesMut, value: &[u8]) {
    write_lenenc_int(dst, value.len() as u64);
    dst.extend_from_slice(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lenenc_integer_round_trips_boundary_values() {
        let cases = [
            (0_u64, vec![0x00]),
            (250_u64, vec![0xFA]),
            (251_u64, vec![0xFC, 0xFB, 0x00]),
            (252_u64, vec![0xFC, 0xFC, 0x00]),
            (65_535_u64, vec![0xFC, 0xFF, 0xFF]),
            (65_536_u64, vec![0xFD, 0x00, 0x00, 0x01]),
            (16_777_215_u64, vec![0xFD, 0xFF, 0xFF, 0xFF]),
            (
                16_777_216_u64,
                vec![0xFE, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00],
            ),
        ];

        for (value, expected_bytes) in cases {
            let mut buffer = BytesMut::new();
            write_lenenc_int(&mut buffer, value);
            assert_eq!(buffer.as_ref(), expected_bytes.as_slice());

            let encoded = buffer.freeze();
            let mut cursor = encoded.as_ref();
            let decoded = read_lenenc_int(&mut cursor).unwrap();

            assert_eq!(decoded, value);
            assert!(cursor.is_empty());
        }
    }
}
