use std::collections::HashMap;

use bytes::{BufMut, Bytes, BytesMut};
use darmok_types::error::{ProtocolError, ProxyError, Result};

use crate::capabilities::CapabilityFlags;
use crate::ok::StatusFlags;
use crate::packet::RawPacket;
use crate::{read_lenenc_bytes, read_lenenc_int};

/// MySQL Handshake V10 packet sent by the server to the client.
#[derive(Debug, Clone, PartialEq)]
pub struct HandshakeV10 {
    pub protocol_version: u8,
    pub server_version: Bytes,
    pub connection_id: u32,
    pub auth_plugin_data: Bytes,
    pub auth_plugin_data_len: u8,
    pub character_set: u8,
    pub status_flags: StatusFlags,
    pub capabilities: CapabilityFlags,
    pub auth_plugin_name: Bytes,
}

/// TLS upgrade prelude sent by a client before the full handshake response.
#[derive(Debug, Clone, PartialEq)]
pub struct SslRequest {
    pub capabilities: CapabilityFlags,
    pub max_packet_size: u32,
    pub character_set: u8,
}

/// A decoded client packet during the initial handshake exchange.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientHandshake {
    SslRequest(SslRequest),
    Response41(HandshakeResponse41),
}

/// The client's response to the initial handshake (protocol 4.1+).
#[derive(Clone, PartialEq)]
pub struct HandshakeResponse41 {
    pub capabilities: CapabilityFlags,
    pub max_packet_size: u32,
    pub character_set: u8,
    pub username: Bytes,
    pub auth_response: Bytes,
    pub database: Option<Bytes>,
    pub auth_plugin_name: Option<Bytes>,
    pub connect_attrs: HashMap<String, String>,
    pub zstd_compression_level: Option<u8>,
}

impl std::fmt::Debug for HandshakeResponse41 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandshakeResponse41")
            .field("capabilities", &self.capabilities)
            .field("max_packet_size", &self.max_packet_size)
            .field("character_set", &self.character_set)
            .field("connect_attr_count", &self.connect_attrs.len())
            .field("zstd_compression_level", &self.zstd_compression_level)
            .finish_non_exhaustive()
    }
}

/// Context carried forward into the command phase after a successful handshake.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandPhaseContext {
    pub effective_capabilities: CapabilityFlags,
    pub username: String,
    pub default_schema: Option<String>,
    pub character_set: u8,
}

impl HandshakeV10 {
    /// Encode the server greeting packet payload in MySQL HandshakeV10 format.
    pub fn encode(&self, dst: &mut BytesMut) {
        let capabilities = self.capabilities.bits();
        let auth_plugin_data_len = if self
            .capabilities
            .contains(CapabilityFlags::CLIENT_PLUGIN_AUTH)
        {
            if self.auth_plugin_data_len == 0 {
                self.auth_plugin_data
                    .len()
                    .saturating_add(1)
                    .try_into()
                    .unwrap_or(u8::MAX)
            } else {
                self.auth_plugin_data_len
            }
        } else {
            0
        };

        dst.put_u8(self.protocol_version);
        put_null_terminated_bytes(dst, &self.server_version);
        dst.put_u32_le(self.connection_id);

        let part1_len = self.auth_plugin_data.len().min(8);
        dst.extend_from_slice(&self.auth_plugin_data[..part1_len]);
        if part1_len < 8 {
            dst.extend(std::iter::repeat_n(0, 8 - part1_len));
        }

        dst.put_u8(0);
        dst.put_u16_le((capabilities & 0xFFFF) as u16);
        dst.put_u8(self.character_set);
        dst.put_u16_le(self.status_flags.bits());
        dst.put_u16_le(((capabilities >> 16) & 0xFFFF) as u16);
        dst.put_u8(auth_plugin_data_len);
        dst.extend(std::iter::repeat_n(0, 10));

        if self
            .capabilities
            .contains(CapabilityFlags::CLIENT_SECURE_CONNECTION)
        {
            let encoded_len = usize::max(13, usize::from(auth_plugin_data_len).saturating_sub(8));
            let remaining = self.auth_plugin_data.len().saturating_sub(8);
            let available = remaining.min(encoded_len.saturating_sub(1));
            dst.extend_from_slice(&self.auth_plugin_data[8..8 + available]);
            if available < encoded_len.saturating_sub(1) {
                dst.extend(std::iter::repeat_n(0, encoded_len - 1 - available));
            }
            dst.put_u8(0);
        }

        if self
            .capabilities
            .contains(CapabilityFlags::CLIENT_PLUGIN_AUTH)
        {
            put_null_terminated_bytes(dst, &self.auth_plugin_name);
        }
    }
}

impl HandshakeResponse41 {
    /// Decode either an SSL request packet or a full HandshakeResponse41 packet.
    pub fn decode(packet: &RawPacket) -> Result<ClientHandshake> {
        let payload = packet.payload.as_ref();
        if payload.len() < 32 {
            return Err(invalid_packet("handshake response shorter than 32 bytes"));
        }

        let capabilities = CapabilityFlags::from_bits_retain(read_u32_le(payload, 0)?);
        let max_packet_size = read_u32_le(payload, 4)?;
        let character_set = payload[8];

        if payload.len() == 32 {
            if capabilities.contains(CapabilityFlags::CLIENT_SSL) {
                return Ok(ClientHandshake::SslRequest(SslRequest {
                    capabilities,
                    max_packet_size,
                    character_set,
                }));
            }

            return Err(invalid_packet(
                "truncated handshake response without CLIENT_SSL",
            ));
        }

        let mut offset = 32;
        let username = read_null_terminated_bytes(payload, &mut offset)?;
        let auth_response =
            if capabilities.contains(CapabilityFlags::CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA) {
                let mut cursor = &payload[offset..];
                let auth_response =
                    Bytes::from(read_lenenc_bytes(&mut cursor, "handshake auth response")?);
                offset = payload.len() - cursor.len();
                auth_response
            } else if capabilities.contains(CapabilityFlags::CLIENT_SECURE_CONNECTION) {
                let auth_len = read_u8(payload, &mut offset)?;
                take_bytes(payload, &mut offset, usize::from(auth_len))?
            } else {
                read_null_terminated_bytes(payload, &mut offset)?
            };

        let database = if capabilities.contains(CapabilityFlags::CLIENT_CONNECT_WITH_DB) {
            Some(read_null_terminated_bytes(payload, &mut offset)?)
        } else {
            None
        };

        let auth_plugin_name = if capabilities.contains(CapabilityFlags::CLIENT_PLUGIN_AUTH) {
            Some(read_null_terminated_bytes(payload, &mut offset)?)
        } else {
            None
        };

        let connect_attrs = if capabilities.contains(CapabilityFlags::CLIENT_CONNECT_ATTRS) {
            read_connect_attrs(payload, &mut offset)?
        } else {
            HashMap::new()
        };

        let zstd_compression_level =
            if capabilities.contains(CapabilityFlags::CLIENT_ZSTD_COMPRESSION_ALGORITHM) {
                Some(read_u8(payload, &mut offset)?)
            } else {
                None
            };

        if offset != payload.len() {
            return Err(invalid_packet(
                "unexpected trailing bytes in handshake response",
            ));
        }

        Ok(ClientHandshake::Response41(Self {
            capabilities,
            max_packet_size,
            character_set,
            username,
            auth_response,
            database,
            auth_plugin_name,
            connect_attrs,
            zstd_compression_level,
        }))
    }
}

fn invalid_packet(message: impl Into<String>) -> ProxyError {
    ProxyError::protocol(ProtocolError::InvalidPacket(message.into()))
}

fn read_u32_le(payload: &[u8], offset: usize) -> Result<u32> {
    let bytes = payload
        .get(offset..offset + 4)
        .ok_or_else(|| invalid_packet("unexpected end of packet while reading u32"))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u8(payload: &[u8], offset: &mut usize) -> Result<u8> {
    let value = *payload
        .get(*offset)
        .ok_or_else(|| invalid_packet("unexpected end of packet while reading u8"))?;
    *offset += 1;
    Ok(value)
}

fn take_bytes(payload: &[u8], offset: &mut usize, len: usize) -> Result<Bytes> {
    let bytes = payload
        .get(*offset..*offset + len)
        .ok_or_else(|| invalid_packet("unexpected end of packet while reading bytes"))?;
    *offset += len;
    Ok(Bytes::copy_from_slice(bytes))
}

fn read_null_terminated_bytes(payload: &[u8], offset: &mut usize) -> Result<Bytes> {
    let start = *offset;
    let relative_end = payload[start..]
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| invalid_packet("missing NUL terminator"))?;
    let end = start + relative_end;
    *offset = end + 1;
    Ok(Bytes::copy_from_slice(&payload[start..end]))
}

fn read_connect_attrs(payload: &[u8], offset: &mut usize) -> Result<HashMap<String, String>> {
    let mut cursor = &payload[*offset..];
    let attr_len = read_lenenc_int(&mut cursor)?;
    let attr_len = usize::try_from(attr_len)
        .map_err(|_| invalid_packet("connection attributes length too large"))?;
    *offset = payload.len() - cursor.len();

    let end = (*offset)
        .checked_add(attr_len)
        .ok_or_else(|| invalid_packet("connection attributes length overflow"))?;
    if end > payload.len() {
        return Err(invalid_packet("connection attributes exceed packet length"));
    }

    let mut attrs = HashMap::new();
    let mut cursor = &payload[*offset..end];
    while !cursor.is_empty() {
        let key = read_lenenc_bytes(&mut cursor, "connection attribute key")?;
        let value = read_lenenc_bytes(&mut cursor, "connection attribute value")?;
        let key = String::from_utf8(key)
            .map_err(|_| invalid_packet("connection attribute key is not valid UTF-8"))?;
        let value = String::from_utf8(value)
            .map_err(|_| invalid_packet("connection attribute value is not valid UTF-8"))?;
        attrs.insert(key, value);
    }

    *offset = end;
    Ok(attrs)
}

fn put_null_terminated_bytes(dst: &mut BytesMut, bytes: &[u8]) {
    dst.extend_from_slice(bytes);
    dst.put_u8(0);
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;

    use super::*;
    use crate::capabilities::SERVER_DEFAULT;
    use crate::packet::PacketHeader;
    use crate::{write_lenenc_bytes, write_lenenc_int};

    #[test]
    fn handshake_debug_redacts_authentication_and_client_supplied_attributes() {
        let secret = Bytes::from_static(b"sensitive-credential");
        let response = HandshakeResponse41 {
            capabilities: CapabilityFlags::CLIENT_PROTOCOL_41,
            max_packet_size: 1024,
            character_set: 45,
            username: secret.clone(),
            auth_response: secret.clone(),
            database: Some(secret.clone()),
            auth_plugin_name: Some(secret),
            connect_attrs: HashMap::from([(
                "sensitive-credential".into(),
                "sensitive-credential".into(),
            )]),
            zstd_compression_level: None,
        };
        let debug = format!("{:?}", ClientHandshake::Response41(response));
        assert!(!debug.contains("sensitive-credential"));
        assert!(!debug.contains("115, 101, 110"));
    }

    #[test]
    fn encodes_handshake_v10_packet() {
        let handshake = HandshakeV10 {
            protocol_version: 0x0A,
            server_version: Bytes::from_static(b"8.4.0-proxy"),
            connection_id: 0x7856_3412,
            auth_plugin_data: Bytes::from_static(&[
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            ]),
            auth_plugin_data_len: 21,
            character_set: 0xFF,
            status_flags: StatusFlags::SERVER_STATUS_AUTOCOMMIT,
            capabilities: SERVER_DEFAULT | CapabilityFlags::CLIENT_SSL,
            auth_plugin_name: Bytes::from_static(b"caching_sha2_password"),
        };

        let mut encoded = BytesMut::new();
        handshake.encode(&mut encoded);

        let encoded = encoded.freeze();
        let version_end = encoded
            .iter()
            .position(|byte| *byte == 0)
            .expect("server version must be NUL terminated");
        assert_eq!(encoded[0], 0x0A);
        assert_eq!(&encoded[1..version_end], b"8.4.0-proxy");
        assert_eq!(
            &encoded[version_end + 1..version_end + 5],
            &0x7856_3412_u32.to_le_bytes()
        );
        assert_eq!(
            &encoded[version_end + 5..version_end + 13],
            &[1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert_eq!(encoded[version_end + 13], 0);

        let lower = u16::from_le_bytes([encoded[version_end + 14], encoded[version_end + 15]]);
        let upper = u16::from_le_bytes([encoded[version_end + 19], encoded[version_end + 20]]);
        let capabilities = u32::from(lower) | (u32::from(upper) << 16);
        assert_eq!(
            capabilities,
            (SERVER_DEFAULT | CapabilityFlags::CLIENT_SSL).bits()
        );
        assert_eq!(encoded[version_end + 16], 0xFF);
        assert_eq!(
            u16::from_le_bytes([encoded[version_end + 17], encoded[version_end + 18]]),
            StatusFlags::SERVER_STATUS_AUTOCOMMIT.bits()
        );
        assert_eq!(encoded[version_end + 21], 21);

        let auth_part2_start = version_end + 32;
        let auth_part2_end = auth_part2_start + 13;
        assert_eq!(
            &encoded[auth_part2_start..auth_part2_end],
            &[9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 0]
        );
        assert_eq!(
            &encoded[auth_part2_end..encoded.len() - 1],
            b"caching_sha2_password"
        );
        assert_eq!(encoded[encoded.len() - 1], 0);
    }

    #[test]
    fn detects_ssl_request_packets() {
        let capabilities = CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_SSL;
        let mut payload = BytesMut::new();
        payload.put_u32_le(capabilities.bits());
        payload.put_u32_le(1024);
        payload.put_u8(0x21);
        payload.extend(std::iter::repeat_n(0, 23));

        let packet = RawPacket {
            header: PacketHeader {
                payload_len: payload.len() as u32,
                sequence_id: 1,
            },
            payload: payload.freeze(),
        };

        let decoded = HandshakeResponse41::decode(&packet).expect("SSL request should decode");
        assert_eq!(
            decoded,
            ClientHandshake::SslRequest(SslRequest {
                capabilities,
                max_packet_size: 1024,
                character_set: 0x21,
            })
        );
    }

    #[test]
    fn decodes_secure_connection_handshake_response() {
        let capabilities = CapabilityFlags::CLIENT_PROTOCOL_41
            | CapabilityFlags::CLIENT_SECURE_CONNECTION
            | CapabilityFlags::CLIENT_CONNECT_WITH_DB
            | CapabilityFlags::CLIENT_PLUGIN_AUTH;
        let mut payload = BytesMut::new();
        payload.put_u32_le(capabilities.bits());
        payload.put_u32_le(16 * 1024 * 1024);
        payload.put_u8(0x21);
        payload.extend(std::iter::repeat_n(0, 23));
        payload.extend_from_slice(b"demo-user");
        payload.put_u8(0);
        payload.put_u8(4);
        payload.extend_from_slice(b"auth");
        payload.extend_from_slice(b"blog");
        payload.put_u8(0);
        payload.extend_from_slice(b"mysql_native_password");
        payload.put_u8(0);

        let packet = RawPacket {
            header: PacketHeader {
                payload_len: payload.len() as u32,
                sequence_id: 1,
            },
            payload: payload.freeze(),
        };

        let decoded =
            HandshakeResponse41::decode(&packet).expect("secure connection response should parse");

        assert_eq!(
            decoded,
            ClientHandshake::Response41(HandshakeResponse41 {
                capabilities,
                max_packet_size: 16 * 1024 * 1024,
                character_set: 0x21,
                username: Bytes::from_static(b"demo-user"),
                auth_response: Bytes::from_static(b"auth"),
                database: Some(Bytes::from_static(b"blog")),
                auth_plugin_name: Some(Bytes::from_static(b"mysql_native_password")),
                connect_attrs: HashMap::new(),
                zstd_compression_level: None,
            })
        );
    }

    #[test]
    fn decodes_lenenc_auth_response_with_attrs_and_zstd() {
        let capabilities = CapabilityFlags::CLIENT_PROTOCOL_41
            | CapabilityFlags::CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA
            | CapabilityFlags::CLIENT_PLUGIN_AUTH
            | CapabilityFlags::CLIENT_CONNECT_ATTRS
            | CapabilityFlags::CLIENT_ZSTD_COMPRESSION_ALGORITHM;
        let mut payload = BytesMut::new();
        payload.put_u32_le(capabilities.bits());
        payload.put_u32_le(1024);
        payload.put_u8(0x2D);
        payload.extend(std::iter::repeat_n(0, 23));
        payload.extend_from_slice(b"attrs-user");
        payload.put_u8(0);
        write_lenenc_bytes(&mut payload, b"scramble-token");
        payload.extend_from_slice(b"caching_sha2_password");
        payload.put_u8(0);

        let mut attrs = BytesMut::new();
        write_lenenc_bytes(&mut attrs, b"_client_name");
        write_lenenc_bytes(&mut attrs, b"mysql_async");
        write_lenenc_bytes(&mut attrs, b"program_name");
        write_lenenc_bytes(&mut attrs, b"wp-cli");
        write_lenenc_int(&mut payload, attrs.len() as u64);
        payload.extend_from_slice(&attrs);
        payload.put_u8(7);

        let packet = RawPacket {
            header: PacketHeader {
                payload_len: payload.len() as u32,
                sequence_id: 2,
            },
            payload: payload.freeze(),
        };

        let decoded =
            HandshakeResponse41::decode(&packet).expect("lenenc handshake response should parse");
        let ClientHandshake::Response41(decoded) = decoded else {
            panic!("expected a full HandshakeResponse41");
        };

        assert_eq!(decoded.capabilities, capabilities);
        assert_eq!(decoded.max_packet_size, 1024);
        assert_eq!(decoded.character_set, 0x2D);
        assert_eq!(decoded.username, Bytes::from_static(b"attrs-user"));
        assert_eq!(decoded.auth_response, Bytes::from_static(b"scramble-token"));
        assert_eq!(decoded.database, None);
        assert_eq!(
            decoded.auth_plugin_name,
            Some(Bytes::from_static(b"caching_sha2_password"))
        );
        assert_eq!(
            decoded.connect_attrs.get("_client_name"),
            Some(&String::from("mysql_async"))
        );
        assert_eq!(
            decoded.connect_attrs.get("program_name"),
            Some(&String::from("wp-cli"))
        );
        assert_eq!(decoded.zstd_compression_level, Some(7));
    }
}
