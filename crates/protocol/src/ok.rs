use bitflags::bitflags;
use bytes::{BufMut, Bytes, BytesMut};

use crate::constants::OK_HEADER;
use crate::wire::write_lenenc_bytes;
use crate::wire::write_lenenc_int;

bitflags! {
    /// MySQL server status flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StatusFlags: u16 {
        const SERVER_STATUS_IN_TRANS             = 0x0001;
        const SERVER_STATUS_AUTOCOMMIT           = 0x0002;
        const SERVER_MORE_RESULTS_EXISTS         = 0x0008;
        const SERVER_STATUS_NO_GOOD_INDEX_USED   = 0x0010;
        const SERVER_STATUS_CURSOR_EXISTS        = 0x0040;
        const SERVER_STATUS_LAST_ROW_SENT        = 0x0080;
        const SERVER_SESSION_STATE_CHANGED       = 0x4000;
    }
}

/// MySQL OK packet.
#[derive(Debug, Clone, PartialEq)]
pub struct OkPacket {
    pub affected_rows: u64,
    pub last_insert_id: u64,
    pub status_flags: StatusFlags,
    pub warnings: u16,
    pub info: Bytes,
    pub session_state_changes: Option<Bytes>,
}

/// Header byte used for OK-as-EOF packets when `CLIENT_DEPRECATE_EOF` is active.
///
/// When the client negotiates `CLIENT_DEPRECATE_EOF`, result sets no longer end
/// with an EOF packet (`0xFE` header). Instead the server sends an OK packet
/// with the `0xFE` header byte in place of the normal `0x00` OK header. The
/// rest of the packet body is identical to a regular OK packet.
const OK_AS_EOF_HEADER: u8 = 0xFE;

impl OkPacket {
    /// Encode this OK packet into the destination buffer.
    pub fn encode(&self, dst: &mut BytesMut) {
        dst.put_u8(OK_HEADER);
        self.encode_body(dst);
    }

    /// Encode this OK packet as an EOF replacement (`0xFE` header).
    ///
    /// When `CLIENT_DEPRECATE_EOF` is negotiated, the server sends an OK
    /// packet with header byte `0xFE` in the final row-terminator position.
    /// The post-column-definition EOF is omitted entirely. The body layout is
    /// identical to a regular OK packet.
    pub fn encode_ok_as_eof(&self, dst: &mut BytesMut) {
        dst.put_u8(OK_AS_EOF_HEADER);
        self.encode_body(dst);
    }

    /// Encode the OK packet body (everything after the header byte).
    fn encode_body(&self, dst: &mut BytesMut) {
        write_lenenc_int(dst, self.affected_rows);
        write_lenenc_int(dst, self.last_insert_id);
        dst.put_u16_le(self.status_flags.bits());
        dst.put_u16_le(self.warnings);
        write_lenenc_bytes(dst, self.info.as_ref());

        if let Some(session_state_changes) = &self.session_state_changes {
            write_lenenc_bytes(dst, session_state_changes.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_ok_packet_with_session_state() {
        let packet = OkPacket {
            affected_rows: 300,
            last_insert_id: 2,
            status_flags: StatusFlags::SERVER_STATUS_AUTOCOMMIT
                | StatusFlags::SERVER_SESSION_STATE_CHANGED,
            warnings: 1,
            info: Bytes::from_static(b"updated"),
            session_state_changes: Some(Bytes::from_static(b"schema=test")),
        };

        let mut dst = BytesMut::new();
        packet.encode(&mut dst);

        assert_eq!(
            dst.as_ref(),
            &[
                0x00, 0xFC, 0x2C, 0x01, 0x02, 0x02, 0x40, 0x01, 0x00, 0x07, b'u', b'p', b'd', b'a',
                b't', b'e', b'd', 0x0B, b's', b'c', b'h', b'e', b'm', b'a', b'=', b't', b'e', b's',
                b't'
            ]
        );
    }
}
