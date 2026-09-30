use bytes::{Buf, BufMut, Bytes, BytesMut};
use darmok_types::error::{ErrorKind, ProtocolError, ProxyError};

use crate::constants::{
    ER_NET_PACKET_TOO_LARGE, MAX_PACKET_PAYLOAD_LEN, PACKET_HEADER_LEN,
    SQLSTATE_COMMUNICATION_ERROR,
};

/// The 4-byte MySQL packet header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketHeader {
    /// Payload length (24-bit on the wire, stored as u32).
    pub payload_len: u32,
    pub sequence_id: u8,
}

/// A raw, undecoded MySQL packet.
#[derive(Debug, Clone, PartialEq)]
pub struct RawPacket {
    pub header: PacketHeader,
    pub payload: Bytes,
}

impl PacketHeader {
    pub fn new(payload_len: usize, sequence_id: u8) -> Result<Self, ProxyError> {
        let payload_len = u32::try_from(payload_len)
            .map_err(|_| invalid_packet(format!("payload length exceeds u32: {payload_len}")))?;

        Ok(Self {
            payload_len,
            sequence_id,
        })
    }

    pub fn decode(src: &[u8]) -> Result<Self, ProxyError> {
        if src.len() < PACKET_HEADER_LEN {
            return Err(invalid_packet(format!(
                "incomplete packet header: expected {PACKET_HEADER_LEN} bytes, got {}",
                src.len()
            )));
        }

        Ok(Self {
            payload_len: read_uint24_le(src),
            sequence_id: src[3],
        })
    }

    pub fn encode(&self, dst: &mut BytesMut) -> Result<(), ProxyError> {
        let payload_len = usize::try_from(self.payload_len).map_err(|_| {
            invalid_packet(format!(
                "payload length exceeds usize: {}",
                self.payload_len
            ))
        })?;
        if payload_len > MAX_PACKET_PAYLOAD_LEN {
            return Err(invalid_packet(format!(
                "frame payload length {} exceeds MySQL maximum {}",
                self.payload_len, MAX_PACKET_PAYLOAD_LEN
            )));
        }

        write_uint24_le(dst, self.payload_len);
        dst.put_u8(self.sequence_id);
        Ok(())
    }
}

impl RawPacket {
    pub fn new(sequence_id: u8, payload: Bytes) -> Result<Self, ProxyError> {
        let header = PacketHeader::new(payload.len(), sequence_id)?;
        Ok(Self { header, payload })
    }

    pub fn logical_payload_len(&self) -> usize {
        self.payload.len()
    }

    pub fn frame_count(&self) -> usize {
        (self.payload.len() / MAX_PACKET_PAYLOAD_LEN) + 1
    }

    pub fn next_sequence_id(&self) -> u8 {
        self.header
            .sequence_id
            .wrapping_add((self.frame_count() % 256) as u8)
    }

    pub fn encoded_len(&self) -> Result<usize, ProxyError> {
        validate_logical_packet(self)?;
        Ok((self.frame_count() * PACKET_HEADER_LEN) + self.payload.len())
    }

    pub fn encode_into(&self, dst: &mut BytesMut) -> Result<(), ProxyError> {
        validate_logical_packet(self)?;
        dst.reserve(self.encoded_len()?);

        let mut sequence_id = self.header.sequence_id;
        let mut remaining = self.payload.as_ref();
        loop {
            let chunk_len = remaining.len().min(MAX_PACKET_PAYLOAD_LEN);
            PacketHeader::new(chunk_len, sequence_id)?.encode(dst)?;

            if chunk_len > 0 {
                dst.put_slice(&remaining[..chunk_len]);
                remaining = &remaining[chunk_len..];
            }

            if chunk_len < MAX_PACKET_PAYLOAD_LEN {
                break;
            }

            sequence_id = sequence_id.wrapping_add(1);
            if remaining.is_empty() {
                PacketHeader::new(0, sequence_id)?.encode(dst)?;
                break;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PacketAssembler {
    payload: BytesMut,
    first_sequence_id: Option<u8>,
    next_sequence_id: Option<u8>,
}

impl PacketAssembler {
    pub(crate) fn push_frame(
        &mut self,
        header: PacketHeader,
        payload: Bytes,
        max_packet_size: usize,
    ) -> Result<Option<RawPacket>, ProxyError> {
        if usize::try_from(header.payload_len).unwrap_or(usize::MAX) != payload.len() {
            self.reset();
            return Err(invalid_packet(format!(
                "header length {} does not match payload length {}",
                header.payload_len,
                payload.len()
            )));
        }

        if let Some(expected) = self.next_sequence_id {
            if header.sequence_id != expected {
                self.reset();
                return Err(sequence_mismatch(expected, header.sequence_id));
            }
        } else {
            self.first_sequence_id = Some(header.sequence_id);
        }

        let next_sequence_id = header.sequence_id.wrapping_add(1);
        let payload_len = payload.len();
        let is_continuation = payload_len == MAX_PACKET_PAYLOAD_LEN;

        let accumulated_len = self
            .payload
            .len()
            .checked_add(payload_len)
            .ok_or_else(|| packet_too_large(usize::MAX))?;

        if accumulated_len > max_packet_size {
            self.reset();
            return Err(packet_too_large(accumulated_len));
        }

        if self.payload.is_empty() && !is_continuation {
            self.reset();
            return Ok(Some(RawPacket { header, payload }));
        }

        self.payload.reserve(payload_len);
        self.payload.extend_from_slice(&payload);

        if is_continuation {
            self.next_sequence_id = Some(next_sequence_id);
            return Ok(None);
        }

        let packet = RawPacket {
            header: PacketHeader {
                payload_len: u32::try_from(self.payload.len()).map_err(|_| {
                    invalid_packet(format!(
                        "reassembled payload length exceeds u32: {}",
                        self.payload.len()
                    ))
                })?,
                sequence_id: self.first_sequence_id.unwrap_or(header.sequence_id),
            },
            payload: self.payload.split().freeze(),
        };
        self.first_sequence_id = None;
        self.next_sequence_id = None;

        Ok(Some(packet))
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.next_sequence_id.is_none()
    }

    pub(crate) fn reset(&mut self) {
        self.payload.clear();
        self.first_sequence_id = None;
        self.next_sequence_id = None;
    }
}

pub(crate) fn try_decode_frame(
    src: &mut BytesMut,
) -> Result<Option<(PacketHeader, Bytes)>, ProxyError> {
    if src.len() < PACKET_HEADER_LEN {
        return Ok(None);
    }

    let header = PacketHeader::decode(&src[..PACKET_HEADER_LEN])?;
    let frame_len = PACKET_HEADER_LEN
        .checked_add(usize::try_from(header.payload_len).map_err(|_| {
            invalid_packet(format!(
                "payload length exceeds usize: {}",
                header.payload_len
            ))
        })?)
        .ok_or_else(|| invalid_packet("packet frame length overflow"))?;

    if src.len() < frame_len {
        return Ok(None);
    }

    let mut frame = src.split_to(frame_len);
    frame.advance(PACKET_HEADER_LEN);
    Ok(Some((header, frame.freeze())))
}

pub(crate) fn read_uint24_le(src: &[u8]) -> u32 {
    u32::from(src[0]) | (u32::from(src[1]) << 8) | (u32::from(src[2]) << 16)
}

pub(crate) fn write_uint24_le(dst: &mut BytesMut, value: u32) {
    dst.put_u8((value & 0xFF) as u8);
    dst.put_u8(((value >> 8) & 0xFF) as u8);
    dst.put_u8(((value >> 16) & 0xFF) as u8);
}

fn validate_logical_packet(packet: &RawPacket) -> Result<(), ProxyError> {
    if usize::try_from(packet.header.payload_len).unwrap_or(usize::MAX) != packet.payload.len() {
        return Err(invalid_packet(format!(
            "logical payload length {} does not match payload buffer length {}",
            packet.header.payload_len,
            packet.payload.len()
        )));
    }

    Ok(())
}

fn invalid_packet(message: impl Into<String>) -> ProxyError {
    ProxyError::protocol(ProtocolError::InvalidPacket(message.into()))
}

fn sequence_mismatch(expected: u8, got: u8) -> ProxyError {
    invalid_packet(format!("sequence mismatch: expected {expected}, got {got}"))
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
    use crate::constants::MAX_PACKET_PAYLOAD_LEN;

    #[test]
    fn single_frame_round_trip_preserves_header_and_payload() {
        let packet = RawPacket::new(7, Bytes::from_static(b"select 1")).unwrap();
        let mut encoded = BytesMut::new();
        packet.encode_into(&mut encoded).unwrap();

        let (header, payload) = try_decode_frame(&mut encoded).unwrap().unwrap();
        assert_eq!(header, packet.header);
        assert_eq!(payload, packet.payload);
        assert!(encoded.is_empty());
    }

    #[test]
    fn exact_max_payload_encodes_with_empty_terminator() {
        let packet = RawPacket::new(42, Bytes::from(vec![0xAB; MAX_PACKET_PAYLOAD_LEN])).unwrap();
        let mut encoded = BytesMut::new();
        packet.encode_into(&mut encoded).unwrap();

        let (first_header, first_payload) = try_decode_frame(&mut encoded).unwrap().unwrap();
        assert_eq!(
            usize::try_from(first_header.payload_len).unwrap(),
            MAX_PACKET_PAYLOAD_LEN
        );
        assert_eq!(first_header.sequence_id, 42);
        assert_eq!(first_payload.len(), MAX_PACKET_PAYLOAD_LEN);

        let (second_header, second_payload) = try_decode_frame(&mut encoded).unwrap().unwrap();
        assert_eq!(second_header.payload_len, 0);
        assert_eq!(second_header.sequence_id, 43);
        assert!(second_payload.is_empty());
        assert!(encoded.is_empty());
    }

    #[test]
    fn assembler_reassembles_split_payload() {
        let packet =
            RawPacket::new(9, Bytes::from(vec![0x55; MAX_PACKET_PAYLOAD_LEN + 11])).unwrap();
        let mut encoded = BytesMut::new();
        packet.encode_into(&mut encoded).unwrap();

        let mut assembler = PacketAssembler::default();
        let mut decoded = None;
        while let Some((header, payload)) = try_decode_frame(&mut encoded).unwrap() {
            decoded = assembler
                .push_frame(header, payload, u32::MAX as usize)
                .unwrap()
                .or(decoded);
        }

        let decoded = decoded.expect("reassembled packet");
        assert_eq!(decoded.header.sequence_id, 9);
        assert_eq!(
            decoded.header.payload_len as usize,
            MAX_PACKET_PAYLOAD_LEN + 11
        );
        assert_eq!(decoded.payload.len(), MAX_PACKET_PAYLOAD_LEN + 11);
        assert!(decoded.payload.iter().all(|byte| *byte == 0x55));
    }

    #[test]
    fn assembler_rejects_sequence_mismatch() {
        let mut assembler = PacketAssembler::default();
        let first_header = PacketHeader::new(MAX_PACKET_PAYLOAD_LEN, 1).unwrap();
        let second_header = PacketHeader::new(3, 9).unwrap();

        assert!(
            assembler
                .push_frame(
                    first_header,
                    Bytes::from(vec![0; MAX_PACKET_PAYLOAD_LEN]),
                    u32::MAX as usize
                )
                .unwrap()
                .is_none()
        );

        let err = assembler
            .push_frame(second_header, Bytes::from_static(b"bad"), u32::MAX as usize)
            .unwrap_err();
        assert!(err.to_string().contains("sequence mismatch"));
    }
}
