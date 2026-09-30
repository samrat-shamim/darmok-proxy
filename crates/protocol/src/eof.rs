use bytes::{BufMut, BytesMut};

use crate::constants::EOF_HEADER;
use crate::ok::StatusFlags;

/// MySQL EOF packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EofPacket {
    pub warnings: u16,
    pub status_flags: StatusFlags,
}

impl EofPacket {
    /// Encode this EOF packet into the destination buffer.
    pub fn encode(&self, dst: &mut BytesMut) {
        dst.put_u8(EOF_HEADER);
        dst.put_u16_le(self.warnings);
        dst.put_u16_le(self.status_flags.bits());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_eof_packet() {
        let packet = EofPacket {
            warnings: 3,
            status_flags: StatusFlags::SERVER_STATUS_AUTOCOMMIT,
        };
        let mut dst = BytesMut::new();
        packet.encode(&mut dst);
        assert_eq!(dst.as_ref(), &[0xFE, 0x03, 0x00, 0x02, 0x00]);
    }
}
