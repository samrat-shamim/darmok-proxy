use bytes::BytesMut;
use darmok_types::error::{ErrorKind, ProtocolError, ProxyError};
use tokio_util::codec::{Decoder, Encoder};

use crate::constants::{
    DEFAULT_MAX_ALLOWED_PACKET, ER_NET_PACKET_TOO_LARGE, SQLSTATE_COMMUNICATION_ERROR,
};
use crate::packet::{PacketAssembler, RawPacket, try_decode_frame};

/// Framing codec that splits a byte stream into MySQL packets.
#[derive(Debug, Clone)]
pub struct MySqlPacketCodec {
    pub max_packet_size: u32,
    assembler: PacketAssembler,
    next_sequence_id: Option<u8>,
}

impl MySqlPacketCodec {
    pub fn new(max_packet_size: u32) -> Self {
        Self {
            max_packet_size: if max_packet_size == 0 {
                DEFAULT_MAX_ALLOWED_PACKET
            } else {
                max_packet_size
            },
            assembler: PacketAssembler::default(),
            next_sequence_id: None,
        }
    }

    /// Handshake packets are parsed outside the codec so TLS upgrades can
    /// swap the underlying transport mid-stream. Reset sequence tracking
    /// before entering the normal command loop.
    pub fn reset_sequence(&mut self) {
        self.assembler.reset();
        self.next_sequence_id = None;
    }
}

impl Decoder for MySqlPacketCodec {
    type Item = RawPacket;
    type Error = ProxyError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            let starting_new_logical_packet = self.assembler.is_idle();
            let Some((header, payload)) = try_decode_frame(src)? else {
                return Ok(None);
            };

            if starting_new_logical_packet
                && let Some(expected) = self.next_sequence_id
                && header.sequence_id != expected
                && header.sequence_id != 0
            {
                self.assembler.reset();
                return Err(sequence_mismatch(expected, header.sequence_id));
            }

            match self
                .assembler
                .push_frame(header, payload, self.max_packet_size as usize)?
            {
                Some(packet) => {
                    self.next_sequence_id = Some(packet.next_sequence_id());
                    return Ok(Some(packet));
                }
                None => continue,
            }
        }
    }
}

impl Encoder<RawPacket> for MySqlPacketCodec {
    type Error = ProxyError;

    fn encode(&mut self, item: RawPacket, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if item.logical_payload_len() > self.max_packet_size as usize {
            return Err(packet_too_large(item.logical_payload_len()));
        }

        if let Some(expected) = self.next_sequence_id
            && item.header.sequence_id != expected
        {
            return Err(sequence_mismatch(expected, item.header.sequence_id));
        }

        item.encode_into(dst)?;
        self.next_sequence_id = Some(item.next_sequence_id());
        Ok(())
    }
}

fn sequence_mismatch(expected: u8, got: u8) -> ProxyError {
    ProxyError::protocol(ProtocolError::InvalidPacket(format!(
        "sequence mismatch: expected {expected}, got {got}"
    )))
}

fn packet_too_large(size: usize) -> ProxyError {
    ProxyError {
        kind: ErrorKind::Protocol(ProtocolError::PacketTooLarge { size }),
        fatal: true,
        mysql_error_code: ER_NET_PACKET_TOO_LARGE,
        sqlstate: SQLSTATE_COMMUNICATION_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{Buf, Bytes};

    use crate::constants::{MAX_PACKET_PAYLOAD_LEN, PACKET_HEADER_LEN};
    use crate::packet::{PacketHeader, read_uint24_le};

    #[test]
    fn decoder_reads_single_frame_packets() {
        let mut codec = MySqlPacketCodec::new(u32::MAX);
        let packet = RawPacket::new(0, Bytes::from_static(b"ping")).unwrap();
        let mut encoded = BytesMut::new();
        packet.encode_into(&mut encoded).unwrap();

        let decoded = codec.decode(&mut encoded).unwrap().unwrap();
        assert_eq!(decoded, packet);
        assert!(encoded.is_empty());
    }

    #[test]
    fn decoder_reassembles_exact_max_payload_with_empty_terminator() {
        let mut codec = MySqlPacketCodec::new((MAX_PACKET_PAYLOAD_LEN + 1) as u32);
        let packet = RawPacket::new(3, Bytes::from(vec![0x11; MAX_PACKET_PAYLOAD_LEN])).unwrap();
        let mut encoded = BytesMut::new();
        packet.encode_into(&mut encoded).unwrap();

        let decoded = codec.decode(&mut encoded).unwrap().unwrap();
        assert_eq!(decoded.header.sequence_id, 3);
        assert_eq!(decoded.payload.len(), MAX_PACKET_PAYLOAD_LEN);
        assert!(decoded.payload.iter().all(|byte| *byte == 0x11));
        assert!(encoded.is_empty());
    }

    #[test]
    fn encoder_splits_payloads_and_wraps_sequence_numbers() {
        let mut codec = MySqlPacketCodec::new(u32::MAX);
        let packet =
            RawPacket::new(255, Bytes::from(vec![0x22; MAX_PACKET_PAYLOAD_LEN + 1])).unwrap();
        let mut encoded = BytesMut::new();

        codec.encode(packet, &mut encoded).unwrap();

        let first_header = PacketHeader::decode(&encoded[..PACKET_HEADER_LEN]).unwrap();
        assert_eq!(first_header.sequence_id, 255);
        assert_eq!(
            usize::try_from(first_header.payload_len).unwrap(),
            MAX_PACKET_PAYLOAD_LEN
        );

        encoded.advance(PACKET_HEADER_LEN + MAX_PACKET_PAYLOAD_LEN);
        let second_header = PacketHeader::decode(&encoded[..PACKET_HEADER_LEN]).unwrap();
        assert_eq!(second_header.sequence_id, 0);
        assert_eq!(read_uint24_le(&encoded[..3]), 1);
    }

    #[test]
    fn decoder_allows_client_command_cycle_reset_to_zero() {
        let mut codec = MySqlPacketCodec::new(u32::MAX);

        let first = RawPacket::new(1, Bytes::from_static(b"auth")).unwrap();
        let mut first_bytes = BytesMut::new();
        first.encode_into(&mut first_bytes).unwrap();
        let decoded = codec.decode(&mut first_bytes).unwrap().unwrap();
        assert_eq!(decoded.header.sequence_id, 1);

        let mut response_bytes = BytesMut::new();
        codec
            .encode(
                RawPacket::new(2, Bytes::from_static(b"ok")).unwrap(),
                &mut response_bytes,
            )
            .unwrap();
        assert!(!response_bytes.is_empty());

        let second = RawPacket::new(0, Bytes::from_static(b"\x03select 1")).unwrap();
        let mut second_bytes = BytesMut::new();
        second.encode_into(&mut second_bytes).unwrap();
        let decoded = codec.decode(&mut second_bytes).unwrap().unwrap();
        assert_eq!(decoded.header.sequence_id, 0);
    }

    #[test]
    fn encoder_rejects_unexpected_sequence_id() {
        let mut codec = MySqlPacketCodec::new(u32::MAX);

        let inbound = RawPacket::new(0, Bytes::from_static(b"\x0eping")).unwrap();
        let mut inbound_bytes = BytesMut::new();
        inbound.encode_into(&mut inbound_bytes).unwrap();
        let _ = codec.decode(&mut inbound_bytes).unwrap().unwrap();

        let err = codec
            .encode(
                RawPacket::new(9, Bytes::from_static(b"bad")).unwrap(),
                &mut BytesMut::new(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("sequence mismatch"));
    }

    #[test]
    fn decoder_rejects_single_frame_packet_over_max_allowed_packet() {
        let mut codec = MySqlPacketCodec::new(4);
        let packet = RawPacket::new(0, Bytes::from_static(b"select 1")).unwrap();
        let mut encoded = BytesMut::new();
        packet.encode_into(&mut encoded).unwrap();

        let err = codec.decode(&mut encoded).unwrap_err();

        assert!(err.to_string().contains("packet too large"));
    }
}
