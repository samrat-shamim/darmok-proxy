use bytes::{BufMut, Bytes, BytesMut};
use darmok_types::error::ProxyError;

use crate::constants::{ERR_HEADER, SQL_STATE_MARKER};

/// MySQL ERR packet.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrPacket {
    pub error_code: u16,
    pub sql_state: [u8; 5],
    pub message: Bytes,
}

impl ErrPacket {
    /// Encode this ERR packet into the destination buffer.
    pub fn encode(&self, dst: &mut BytesMut) {
        dst.put_u8(ERR_HEADER);
        dst.put_u16_le(self.error_code);
        dst.put_u8(SQL_STATE_MARKER);
        dst.extend_from_slice(&self.sql_state);
        dst.extend_from_slice(self.message.as_ref());
    }

    /// Build an `ErrPacket` from a `ProxyError`.
    pub fn from_proxy_error(err: &ProxyError) -> Self {
        Self {
            error_code: err.mysql_error_code,
            sql_state: err.sqlstate,
            message: Bytes::from(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darmok_types::error::ProxyError;

    #[test]
    fn encodes_err_packet() {
        let packet = ErrPacket {
            error_code: 1064,
            sql_state: *b"42000",
            message: Bytes::from_static(b"syntax error"),
        };
        let mut dst = BytesMut::new();
        packet.encode(&mut dst);

        assert_eq!(
            dst.as_ref(),
            &[
                0xFF, 0x28, 0x04, b'#', b'4', b'2', b'0', b'0', b'0', b's', b'y', b'n', b't', b'a',
                b'x', b' ', b'e', b'r', b'r', b'o', b'r'
            ]
        );
    }

    #[test]
    fn converts_proxy_error_losslessly() {
        let err = ProxyError::translation(darmok_types::error::TranslationError::Parse(
            "bad sql".to_owned(),
        ));
        let packet = ErrPacket::from_proxy_error(&err);

        assert_eq!(packet.error_code, err.mysql_error_code);
        assert_eq!(packet.sql_state, err.sqlstate);
        assert_eq!(packet.message.as_ref(), err.to_string().as_bytes());
    }
}
