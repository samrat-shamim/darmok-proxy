use bytes::{Buf, Bytes};
use darmok_types::error::Result;
use darmok_types::mysql_const::{self, field_type};
use darmok_types::value::{PreparedStatementParamType, Value};
use std::collections::HashMap;
use std::fmt;

use crate::capabilities::CapabilityFlags;
use crate::constants::{
    COM_CHANGE_USER, COM_FIELD_LIST, COM_INIT_DB, COM_PING, COM_PROCESS_INFO, COM_PROCESS_KILL,
    COM_QUERY, COM_QUIT, COM_RESET_CONNECTION, COM_SET_OPTION, COM_STATISTICS, COM_STMT_CLOSE,
    COM_STMT_EXECUTE, COM_STMT_FETCH, COM_STMT_PREPARE, COM_STMT_RESET, COM_STMT_SEND_LONG_DATA,
};
use crate::wire::{
    decode_utf8, ensure_remaining, invalid_packet, read_lenenc_bytes, read_u16_le, read_u32_le,
    take_null_terminated, unsupported,
};

/// A decoded MySQL client command.
#[derive(Clone, PartialEq)]
pub enum Command {
    Quit,
    Ping,
    InitDb(String),
    Query(String),
    FieldList {
        table: String,
        wildcard: String,
    },
    Statistics,
    ProcessInfo,
    ProcessKill {
        id: u32,
    },
    ChangeUser {
        username: String,
        database: Option<String>,
        auth_response: Vec<u8>,
        auth_plugin_name: Option<String>,
        charset_id: Option<u16>,
        connect_attrs: HashMap<String, String>,
    },
    ResetConnection,
    SetOption {
        option: u16,
    },
    StmtPrepare(String),
    StmtExecute {
        stmt_id: u32,
        cursor_flags: u8,
        iteration_count: u32,
        /// Opaque until the session supplies the registered parameter count and types.
        parameter_payload: Bytes,
    },
    StmtClose {
        stmt_id: u32,
    },
    StmtReset {
        stmt_id: u32,
    },
    StmtSendLongData {
        stmt_id: u32,
        param_id: u16,
        data: Bytes,
    },
    StmtFetch {
        stmt_id: u32,
        num_rows: u32,
    },
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quit => f.write_str("Quit"),
            Self::Ping => f.write_str("Ping"),
            Self::InitDb(_) => f.write_str("InitDb(<redacted>)"),
            Self::Query(sql) => f
                .debug_struct("Query")
                .field("sql_bytes", &sql.len())
                .finish(),
            Self::FieldList { .. } => f.write_str("FieldList(<redacted>)"),
            Self::Statistics => f.write_str("Statistics"),
            Self::ProcessInfo => f.write_str("ProcessInfo"),
            Self::ProcessKill { id } => f.debug_struct("ProcessKill").field("id", id).finish(),
            Self::ChangeUser { .. } => f.write_str("ChangeUser(<redacted>)"),
            Self::ResetConnection => f.write_str("ResetConnection"),
            Self::SetOption { option } => {
                f.debug_struct("SetOption").field("option", option).finish()
            }
            Self::StmtPrepare(sql) => f
                .debug_struct("StmtPrepare")
                .field("sql_bytes", &sql.len())
                .finish(),
            Self::StmtExecute {
                stmt_id,
                parameter_payload,
                ..
            } => f
                .debug_struct("StmtExecute")
                .field("stmt_id", stmt_id)
                .field("parameter_bytes", &parameter_payload.len())
                .finish(),
            Self::StmtClose { stmt_id } => f
                .debug_struct("StmtClose")
                .field("stmt_id", stmt_id)
                .finish(),
            Self::StmtReset { stmt_id } => f
                .debug_struct("StmtReset")
                .field("stmt_id", stmt_id)
                .finish(),
            Self::StmtSendLongData {
                stmt_id,
                param_id,
                data,
            } => f
                .debug_struct("StmtSendLongData")
                .field("stmt_id", stmt_id)
                .field("param_id", param_id)
                .field("data_bytes", &data.len())
                .finish(),
            Self::StmtFetch { stmt_id, num_rows } => f
                .debug_struct("StmtFetch")
                .field("stmt_id", stmt_id)
                .field("num_rows", num_rows)
                .finish(),
        }
    }
}

impl Command {
    /// Decode a command from the raw packet payload.
    ///
    /// `capabilities` is the negotiated capability flags so the decoder can
    /// handle protocol variations.
    pub fn decode(payload: &[u8], capabilities: u32) -> Result<Command> {
        let Some((&command, body)) = payload.split_first() else {
            return Err(invalid_packet("empty command packet"));
        };
        let capabilities = CapabilityFlags::from_bits_retain(capabilities);
        if capabilities.contains(CapabilityFlags::CLIENT_QUERY_ATTRIBUTES)
            && matches!(command, COM_QUERY | COM_STMT_EXECUTE)
        {
            return Err(unsupported("query attributes are unsupported"));
        }

        match command {
            COM_QUIT => Ok(Command::Quit),
            COM_PING => Ok(Command::Ping),
            COM_INIT_DB => Ok(Command::InitDb(decode_utf8(
                body.to_vec(),
                "COM_INIT_DB payload",
            )?)),
            COM_QUERY => Ok(Command::Query(decode_utf8(
                body.to_vec(),
                "COM_QUERY payload",
            )?)),
            COM_FIELD_LIST => decode_field_list(body),
            COM_STATISTICS => Ok(Command::Statistics),
            COM_PROCESS_INFO => Ok(Command::ProcessInfo),
            COM_PROCESS_KILL => Ok(Command::ProcessKill {
                id: read_u32_le(body, "COM_PROCESS_KILL statement id")?,
            }),
            COM_CHANGE_USER => decode_change_user(body, capabilities),
            COM_RESET_CONNECTION => Ok(Command::ResetConnection),
            COM_SET_OPTION => Ok(Command::SetOption {
                option: read_u16_le(body, "COM_SET_OPTION option")?,
            }),
            COM_STMT_PREPARE => Ok(Command::StmtPrepare(decode_utf8(
                body.to_vec(),
                "COM_STMT_PREPARE payload",
            )?)),
            COM_STMT_EXECUTE => decode_stmt_execute(body),
            COM_STMT_CLOSE => Ok(Command::StmtClose {
                stmt_id: read_u32_le(body, "COM_STMT_CLOSE statement id")?,
            }),
            COM_STMT_RESET => Ok(Command::StmtReset {
                stmt_id: read_u32_le(body, "COM_STMT_RESET statement id")?,
            }),
            COM_STMT_SEND_LONG_DATA => decode_stmt_send_long_data(body),
            COM_STMT_FETCH => decode_stmt_fetch(body),
            _ => Err(unsupported(format!(
                "unsupported command byte 0x{command:02x}"
            ))),
        }
    }
}

fn decode_field_list(body: &[u8]) -> Result<Command> {
    let mut cursor = body;
    let table = decode_utf8(
        take_null_terminated(&mut cursor, "COM_FIELD_LIST table")?,
        "COM_FIELD_LIST table",
    )?;
    let wildcard = decode_utf8(cursor.to_vec(), "COM_FIELD_LIST wildcard")?;
    Ok(Command::FieldList { table, wildcard })
}

fn decode_change_user(body: &[u8], capabilities: CapabilityFlags) -> Result<Command> {
    let mut cursor = body;
    let username = decode_utf8(
        take_null_terminated(&mut cursor, "COM_CHANGE_USER username")?,
        "COM_CHANGE_USER username",
    )?;

    // Unlike HandshakeResponse41, this command always uses the one-byte
    // secure-connection auth length, even with LENENC_CLIENT_DATA negotiated.
    let auth_response = if capabilities.contains(CapabilityFlags::CLIENT_SECURE_CONNECTION) {
        ensure_remaining(&cursor, 1, "COM_CHANGE_USER auth response length")?;
        let length = usize::from(cursor.get_u8());
        ensure_remaining(&cursor, length, "COM_CHANGE_USER auth response")?;
        let value = cursor[..length].to_vec();
        cursor.advance(length);
        value
    } else {
        take_null_terminated(&mut cursor, "COM_CHANGE_USER auth response")?
    };
    let database = decode_utf8(
        take_null_terminated(&mut cursor, "COM_CHANGE_USER database")?,
        "COM_CHANGE_USER database",
    )?;
    let database = (!database.is_empty()).then_some(database);
    let has_extensions = !cursor.is_empty();
    let charset_id = if has_extensions && capabilities.contains(CapabilityFlags::CLIENT_PROTOCOL_41)
    {
        let charset = read_u16_le(cursor, "COM_CHANGE_USER character set")?;
        cursor.advance(2);
        Some(charset)
    } else {
        None
    };
    let auth_plugin_name =
        if has_extensions && capabilities.contains(CapabilityFlags::CLIENT_PLUGIN_AUTH) {
            Some(decode_utf8(
                take_null_terminated(&mut cursor, "COM_CHANGE_USER auth plugin")?,
                "COM_CHANGE_USER auth plugin",
            )?)
        } else {
            None
        };
    let connect_attrs =
        if has_extensions && capabilities.contains(CapabilityFlags::CLIENT_CONNECT_ATTRS) {
            let mut offset = 0;
            let attrs = crate::handshake::read_connect_attrs(cursor, &mut offset)?;
            cursor.advance(offset);
            attrs
        } else {
            HashMap::new()
        };
    if !cursor.is_empty() {
        return Err(invalid_packet("trailing COM_CHANGE_USER bytes"));
    }

    Ok(Command::ChangeUser {
        username,
        database,
        auth_response,
        auth_plugin_name,
        charset_id,
        connect_attrs,
    })
}

fn decode_stmt_send_long_data(body: &[u8]) -> Result<Command> {
    if body.len() < 6 {
        return Err(invalid_packet("invalid COM_STMT_SEND_LONG_DATA packet"));
    }

    Ok(Command::StmtSendLongData {
        stmt_id: read_u32_le(&body[..4], "COM_STMT_SEND_LONG_DATA statement id")?,
        param_id: read_u16_le(&body[4..6], "COM_STMT_SEND_LONG_DATA parameter id")?,
        data: Bytes::copy_from_slice(&body[6..]),
    })
}

fn decode_stmt_fetch(body: &[u8]) -> Result<Command> {
    if body.len() < 8 {
        return Err(invalid_packet("invalid COM_STMT_FETCH packet"));
    }

    Ok(Command::StmtFetch {
        stmt_id: read_u32_le(&body[..4], "COM_STMT_FETCH statement id")?,
        num_rows: read_u32_le(&body[4..8], "COM_STMT_FETCH num_rows")?,
    })
}

fn decode_stmt_execute(body: &[u8]) -> Result<Command> {
    if body.len() < 9 {
        return Err(invalid_packet("invalid COM_STMT_EXECUTE packet"));
    }

    let stmt_id = read_u32_le(&body[..4], "COM_STMT_EXECUTE statement id")?;
    let cursor_flags = body[4];
    let iteration_count = read_u32_le(&body[5..9], "COM_STMT_EXECUTE iteration count")?;
    if cursor_flags != 0 {
        return Err(unsupported(
            "prepared cursors and query attributes are unsupported",
        ));
    }
    if iteration_count != 1 {
        return Err(invalid_packet("COM_STMT_EXECUTE iteration count must be 1"));
    }
    Ok(Command::StmtExecute {
        stmt_id,
        cursor_flags,
        iteration_count,
        parameter_payload: Bytes::copy_from_slice(&body[9..]),
    })
}

/// Decoded bindings. The caller commits `new_types` to the prepared registry
/// only after successful decoding. Textual wire values stay bytes: the session's
/// negotiated character set determines their interpretation, never a UTF-8 guess.
#[derive(Clone, PartialEq)]
pub struct DecodedParameters {
    pub values: Vec<Value>,
    pub new_types: Option<Vec<PreparedStatementParamType>>,
}

impl fmt::Debug for DecodedParameters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodedParameters")
            .field("parameter_count", &self.values.len())
            .field("new_types", &self.new_types)
            .finish()
    }
}

/// Decode one COM_STMT_EXECUTE binding payload using authoritative prepared
/// metadata. Query attributes and long-data substitution are not accepted by
/// this decoder. Malformed packets leave the caller's cached types unchanged.
pub fn decode_stmt_execute_parameters(
    payload: &[u8],
    param_count: u16,
    cached_types: &[PreparedStatementParamType],
) -> Result<DecodedParameters> {
    let param_count = usize::from(param_count);
    if param_count == 0 {
        if !payload.is_empty() {
            return Err(invalid_packet(
                "parameter bytes for zero-parameter statement",
            ));
        }
        return Ok(DecodedParameters {
            values: Vec::new(),
            new_types: None,
        });
    }
    let mut cursor = payload;
    let null_bitmap_len = param_count.div_ceil(8);
    ensure_remaining(
        &cursor,
        null_bitmap_len + 1,
        "COM_STMT_EXECUTE parameter metadata",
    )?;
    let null_bitmap = &cursor[..null_bitmap_len];
    cursor.advance(null_bitmap_len);
    let new_types = match cursor.get_u8() {
        0 => {
            if cached_types.len() != param_count {
                return Err(invalid_packet(
                    "parameter types have not been bound for this statement",
                ));
            }
            None
        }
        1 => {
            ensure_remaining(&cursor, param_count * 2, "COM_STMT_EXECUTE parameter types")?;
            let mut types = Vec::with_capacity(param_count);
            for _ in 0..param_count {
                let field_type = cursor.get_u8();
                let flags = cursor.get_u8();
                if flags != 0 && flags != 0x80 {
                    return Err(invalid_packet(
                        "invalid COM_STMT_EXECUTE parameter type flags",
                    ));
                }
                types.push(PreparedStatementParamType {
                    field_type,
                    unsigned: flags == 0x80,
                });
            }
            Some(types)
        }
        _ => {
            return Err(invalid_packet(
                "invalid COM_STMT_EXECUTE new-params-bound flag",
            ));
        }
    };
    let types = new_types.as_deref().unwrap_or(cached_types);
    let mut values = Vec::with_capacity(param_count);
    for (index, parameter_type) in types.iter().enumerate() {
        validate_parameter_type(parameter_type.field_type)?;
        values.push(if bitmap_is_set(null_bitmap, index) {
            Value::Null
        } else {
            decode_binary_parameter(
                &mut cursor,
                parameter_type.field_type,
                parameter_type.unsigned,
            )?
        });
    }
    if !cursor.is_empty() {
        return Err(invalid_packet("trailing COM_STMT_EXECUTE parameter bytes"));
    }
    Ok(DecodedParameters { values, new_types })
}

fn validate_parameter_type(field_type: u8) -> Result<()> {
    match field_type {
        field_type::TINY
        | field_type::SHORT
        | field_type::LONG
        | field_type::INT24
        | field_type::LONGLONG
        | field_type::YEAR
        | field_type::FLOAT
        | field_type::DOUBLE
        | field_type::DECIMAL
        | field_type::NEWDECIMAL
        | field_type::VAR_STRING
        | field_type::STRING
        | field_type::VARCHAR
        | field_type::JSON
        | field_type::BLOB
        | field_type::BIT
        | field_type::DATE
        | field_type::TIME
        | field_type::DATETIME
        | field_type::TIMESTAMP
        | field_type::NULL => Ok(()),
        _ => Err(unsupported(format!(
            "unsupported binary parameter type 0x{field_type:02x}"
        ))),
    }
}

fn decode_binary_parameter(cursor: &mut &[u8], field_type: u8, unsigned: bool) -> Result<Value> {
    match field_type {
        mysql_const::field_type::TINY => {
            ensure_remaining(cursor, 1, "TINY parameter")?;
            let value = cursor.get_i8();
            if unsigned {
                Ok(Value::UInt(value as u8 as u64))
            } else {
                Ok(Value::Int(i64::from(value)))
            }
        }
        mysql_const::field_type::SHORT | mysql_const::field_type::YEAR => {
            ensure_remaining(cursor, 2, "SHORT parameter")?;
            if unsigned {
                Ok(Value::UInt(u64::from(cursor.get_u16_le())))
            } else {
                Ok(Value::Int(i64::from(cursor.get_i16_le())))
            }
        }
        mysql_const::field_type::LONG | mysql_const::field_type::INT24 => {
            ensure_remaining(cursor, 4, "LONG parameter")?;
            if unsigned {
                Ok(Value::UInt(u64::from(cursor.get_u32_le())))
            } else {
                Ok(Value::Int(i64::from(cursor.get_i32_le())))
            }
        }
        mysql_const::field_type::LONGLONG => {
            ensure_remaining(cursor, 8, "LONGLONG parameter")?;
            if unsigned {
                Ok(Value::UInt(cursor.get_u64_le()))
            } else {
                Ok(Value::Int(cursor.get_i64_le()))
            }
        }
        mysql_const::field_type::FLOAT => {
            ensure_remaining(cursor, 4, "FLOAT parameter")?;
            Ok(Value::Float(f64::from(cursor.get_f32_le())))
        }
        mysql_const::field_type::DOUBLE => {
            ensure_remaining(cursor, 8, "DOUBLE parameter")?;
            Ok(Value::Float(cursor.get_f64_le()))
        }
        field_type::DECIMAL | field_type::NEWDECIMAL => {
            let bytes = read_lenenc_bytes(cursor, "DECIMAL parameter")?;
            let value = decode_utf8(bytes, "DECIMAL parameter")?;
            Ok(Value::Decimal(value.into_boxed_str()))
        }
        mysql_const::field_type::VAR_STRING
        | mysql_const::field_type::STRING
        | mysql_const::field_type::VARCHAR
        | mysql_const::field_type::JSON => {
            let bytes = read_lenenc_bytes(cursor, "string parameter")?;
            Ok(Value::Bytes(Bytes::from(bytes)))
        }
        mysql_const::field_type::BLOB | mysql_const::field_type::BIT => {
            let bytes = read_lenenc_bytes(cursor, "bytes parameter")?;
            Ok(Value::Bytes(Bytes::from(bytes)))
        }
        mysql_const::field_type::DATE => decode_date(cursor),
        mysql_const::field_type::TIME => decode_time(cursor),
        mysql_const::field_type::DATETIME | mysql_const::field_type::TIMESTAMP => {
            decode_datetime(cursor)
        }
        mysql_const::field_type::NULL => Ok(Value::Null),
        _ => Err(unsupported(format!(
            "unsupported binary parameter type 0x{field_type:02x}"
        ))),
    }
}

fn decode_date(cursor: &mut &[u8]) -> Result<Value> {
    ensure_remaining(cursor, 1, "DATE parameter length")?;
    match cursor.get_u8() {
        0 => Ok(Value::Date {
            year: 0,
            month: 0,
            day: 0,
        }),
        4 => {
            ensure_remaining(cursor, 4, "DATE parameter")?;
            Ok(Value::Date {
                year: cursor.get_u16_le(),
                month: cursor.get_u8(),
                day: cursor.get_u8(),
            })
        }
        length => Err(invalid_packet(format!(
            "invalid DATE parameter length {length}"
        ))),
    }
}

fn decode_datetime(cursor: &mut &[u8]) -> Result<Value> {
    ensure_remaining(cursor, 1, "DATETIME parameter length")?;
    match cursor.get_u8() {
        0 => Ok(Value::DateTime {
            year: 0,
            month: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            micros: 0,
        }),
        4 => {
            ensure_remaining(cursor, 4, "DATETIME parameter")?;
            Ok(Value::DateTime {
                year: cursor.get_u16_le(),
                month: cursor.get_u8(),
                day: cursor.get_u8(),
                hour: 0,
                minute: 0,
                second: 0,
                micros: 0,
            })
        }
        7 => {
            ensure_remaining(cursor, 7, "DATETIME parameter")?;
            Ok(Value::DateTime {
                year: cursor.get_u16_le(),
                month: cursor.get_u8(),
                day: cursor.get_u8(),
                hour: cursor.get_u8(),
                minute: cursor.get_u8(),
                second: cursor.get_u8(),
                micros: 0,
            })
        }
        11 => {
            ensure_remaining(cursor, 11, "DATETIME parameter")?;
            Ok(Value::DateTime {
                year: cursor.get_u16_le(),
                month: cursor.get_u8(),
                day: cursor.get_u8(),
                hour: cursor.get_u8(),
                minute: cursor.get_u8(),
                second: cursor.get_u8(),
                micros: cursor.get_u32_le(),
            })
        }
        length => Err(invalid_packet(format!(
            "invalid DATETIME parameter length {length}"
        ))),
    }
}

fn decode_time(cursor: &mut &[u8]) -> Result<Value> {
    ensure_remaining(cursor, 1, "TIME parameter length")?;
    match cursor.get_u8() {
        0 => Ok(Value::Time {
            negative: false,
            days: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
            micros: 0,
        }),
        8 => {
            ensure_remaining(cursor, 8, "TIME parameter")?;
            Ok(Value::Time {
                negative: decode_time_sign(cursor)?,
                days: cursor.get_u32_le(),
                hours: cursor.get_u8(),
                minutes: cursor.get_u8(),
                seconds: cursor.get_u8(),
                micros: 0,
            })
        }
        12 => {
            ensure_remaining(cursor, 12, "TIME parameter")?;
            Ok(Value::Time {
                negative: decode_time_sign(cursor)?,
                days: cursor.get_u32_le(),
                hours: cursor.get_u8(),
                minutes: cursor.get_u8(),
                seconds: cursor.get_u8(),
                micros: cursor.get_u32_le(),
            })
        }
        length => Err(invalid_packet(format!(
            "invalid TIME parameter length {length}"
        ))),
    }
}

fn decode_time_sign(cursor: &mut &[u8]) -> Result<bool> {
    match cursor.get_u8() {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid_packet("invalid TIME parameter sign")),
    }
}

fn bitmap_is_set(bitmap: &[u8], index: usize) -> bool {
    let byte_index = index / 8;
    let bit_index = index % 8;
    bitmap
        .get(byte_index)
        .map(|byte| (byte & (1 << bit_index)) != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_parameter_sign_must_be_zero_or_one() {
        for length in [8, 12] {
            for sign in 0..=u8::MAX {
                let mut payload = vec![0, 1, field_type::TIME, 0, length, sign];
                payload.resize(5 + usize::from(length), 0);
                assert_eq!(
                    decode_stmt_execute_parameters(&payload, 1, &[]).is_ok(),
                    sign <= 1
                );
            }
        }
    }

    #[test]
    fn change_user_always_reads_database_full_charset_and_connection_attributes() {
        let capabilities = CapabilityFlags::CLIENT_PROTOCOL_41
            | CapabilityFlags::CLIENT_SECURE_CONNECTION
            | CapabilityFlags::CLIENT_PLUGIN_AUTH
            | CapabilityFlags::CLIENT_CONNECT_ATTRS;
        // CONNECT_WITH_DB is deliberately absent. Charset 0x012d must not be
        // confused with the beginning of the plugin name.
        let payload = b"\x11alice\0\x00reporting\0\x2d\x01caching_sha2_password\0\x04\x01k\x01v";
        let Command::ChangeUser {
            database,
            charset_id,
            auth_plugin_name,
            connect_attrs,
            ..
        } = Command::decode(payload, capabilities.bits()).unwrap()
        else {
            panic!("wrong command");
        };
        assert_eq!(database.as_deref(), Some("reporting"));
        assert_eq!(charset_id, Some(301));
        assert_eq!(auth_plugin_name.as_deref(), Some("caching_sha2_password"));
        assert_eq!(connect_attrs.get("k").map(String::as_str), Some("v"));
        let mut trailing = payload.to_vec();
        trailing.push(1);
        assert!(Command::decode(&trailing, capabilities.bits()).is_err());
        assert!(Command::decode(&payload[..payload.len() - 1], capabilities.bits()).is_err());
    }

    #[test]
    fn change_user_secure_auth_length_is_a_byte_even_with_lenenc_handshake_capability() {
        let capabilities = CapabilityFlags::CLIENT_SECURE_CONNECTION
            | CapabilityFlags::CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA;
        let mut payload = b"\x11alice\0\xfb".to_vec();
        payload.extend_from_slice(&[7; 251]);
        payload.push(0); // Mandatory empty database.
        let Command::ChangeUser {
            auth_response,
            database,
            ..
        } = Command::decode(&payload, capabilities.bits()).unwrap()
        else {
            panic!("wrong command");
        };
        assert_eq!(auth_response, vec![7; 251]);
        assert!(database.is_none());
        assert!(Command::decode(&payload[..payload.len() - 1], capabilities.bits()).is_err());
        payload.push(45); // A partial two-byte charset is invalid.
        assert!(
            Command::decode(
                &payload,
                (capabilities | CapabilityFlags::CLIENT_PROTOCOL_41).bits()
            )
            .is_err()
        );
    }

    #[test]
    fn execute_uses_cached_types_and_the_unsigned_high_bit() {
        let fresh = [0, 1, field_type::TINY, 0x80, 255];
        let decoded = decode_stmt_execute_parameters(&fresh, 1, &[]).unwrap();
        assert_eq!(decoded.values, vec![Value::UInt(255)]);
        let types = decoded.new_types.unwrap();
        let reused = decode_stmt_execute_parameters(&[0, 0, 254], 1, &types).unwrap();
        assert_eq!(reused.values, vec![Value::UInt(254)]);
        assert!(reused.new_types.is_none());
        assert!(decode_stmt_execute_parameters(&[0, 0, 254], 1, &[]).is_err());
    }

    #[test]
    fn execute_rejects_truncation_trailing_bytes_and_invalid_metadata() {
        let valid = [0, 1, field_type::LONG, 0, 42, 0, 0, 0];
        for end in 0..valid.len() {
            assert!(decode_stmt_execute_parameters(&valid[..end], 1, &[]).is_err());
        }
        let mut trailing = valid.to_vec();
        trailing.push(0);
        assert!(decode_stmt_execute_parameters(&trailing, 1, &[]).is_err());
        for malformed in [
            vec![0, 2, field_type::TINY, 0, 1],
            vec![0, 1, field_type::TINY, 1, 1],
            vec![1, 1, 0xff, 0], // NULL cannot hide an unsupported type.
            vec![1, 1, field_type::NULL, 0, 0], // No speculative NULL padding.
        ] {
            assert!(decode_stmt_execute_parameters(&malformed, 1, &[]).is_err());
        }
        assert!(decode_stmt_execute_parameters(&[], 0, &[]).is_ok());
        assert!(decode_stmt_execute_parameters(&[0], 0, &[]).is_err());
    }

    #[test]
    fn execute_keeps_text_as_bytes_for_session_charset_decoding() {
        for data in [b"hi".as_slice(), &[0xff, 0xfe]] {
            let mut payload = vec![0, 1, field_type::VAR_STRING, 0, 2];
            payload.extend_from_slice(data);
            let decoded = decode_stmt_execute_parameters(&payload, 1, &[]).unwrap();
            assert_eq!(
                decoded.values,
                vec![Value::Bytes(Bytes::copy_from_slice(data))]
            );
        }
    }

    #[test]
    fn execute_requires_supported_cursor_flags_iteration_and_capabilities() {
        let packet = [COM_STMT_EXECUTE, 1, 0, 0, 0, 0, 1, 0, 0, 0];
        assert!(Command::decode(&packet, 0).is_ok());
        for flag in [1, 2, 4, 8, 255] {
            let mut bad = packet;
            bad[5] = flag;
            assert!(Command::decode(&bad, 0).is_err());
        }
        let mut bad = packet;
        bad[6] = 0;
        assert!(Command::decode(&bad, 0).is_err());
        assert!(Command::decode(&packet, CapabilityFlags::CLIENT_QUERY_ATTRIBUTES.bits()).is_err());
    }

    #[test]
    fn command_debug_never_prints_query_authentication_or_parameter_contents() {
        let secret = "sensitive-credential-and-query-value";
        let commands = [
            Command::Query(secret.into()),
            Command::StmtPrepare(secret.into()),
            Command::ChangeUser {
                username: secret.into(),
                database: Some(secret.into()),
                auth_response: secret.as_bytes().to_vec(),
                auth_plugin_name: Some(secret.into()),
                charset_id: None,
                connect_attrs: HashMap::new(),
            },
            Command::StmtExecute {
                stmt_id: 1,
                cursor_flags: 0,
                iteration_count: 1,
                parameter_payload: Bytes::copy_from_slice(secret.as_bytes()),
            },
            Command::StmtSendLongData {
                stmt_id: 1,
                param_id: 0,
                data: Bytes::copy_from_slice(secret.as_bytes()),
            },
        ];
        for command in commands {
            let debug = format!("{command:?}");
            assert!(!debug.contains(secret));
            assert!(!debug.contains("115, 101, 110"));
        }
        let bindings = DecodedParameters {
            values: vec![Value::String(secret.into())],
            new_types: None,
        };
        assert!(!format!("{bindings:?}").contains(secret));
    }

    #[test]
    fn decodes_simple_commands() {
        assert_eq!(Command::decode(&[COM_QUIT], 0).unwrap(), Command::Quit);
        assert_eq!(Command::decode(&[COM_PING], 0).unwrap(), Command::Ping);
        assert_eq!(
            Command::decode(&[COM_QUERY, b's', b'e', b'l', b'e', b'c', b't'], 0).unwrap(),
            Command::Query("select".to_owned())
        );
        assert_eq!(
            Command::decode(&[COM_INIT_DB, b'a', b'p', b'p'], 0).unwrap(),
            Command::InitDb("app".to_owned())
        );
    }

    #[test]
    fn decodes_field_list_and_change_user() {
        let field_list = [
            COM_FIELD_LIST,
            b'u',
            b's',
            b'e',
            b'r',
            b's',
            0,
            b'i',
            b'd',
            b'%',
            0,
        ];
        assert_eq!(
            Command::decode(&field_list, 0).unwrap(),
            Command::FieldList {
                table: "users".to_owned(),
                wildcard: "id%\0".to_owned(),
            }
        );

        let capabilities = CapabilityFlags::CLIENT_SECURE_CONNECTION
            | CapabilityFlags::CLIENT_CONNECT_WITH_DB
            | CapabilityFlags::CLIENT_PROTOCOL_41
            | CapabilityFlags::CLIENT_PLUGIN_AUTH;
        let change_user = [
            COM_CHANGE_USER,
            b'a',
            b'l',
            b'i',
            b'c',
            b'e',
            0,
            0x03,
            b'x',
            b'y',
            b'z',
            b'a',
            b'p',
            b'p',
            0,
            45,
            0,
            b'm',
            b'y',
            b's',
            b'q',
            b'l',
            b'_',
            b'n',
            b'a',
            b't',
            b'i',
            b'v',
            b'e',
            0,
        ];
        assert_eq!(
            Command::decode(&change_user, capabilities.bits()).unwrap(),
            Command::ChangeUser {
                username: "alice".to_owned(),
                database: Some("app".to_owned()),
                auth_response: b"xyz".to_vec(),
                auth_plugin_name: Some("mysql_native".to_owned()),
                charset_id: Some(45),
                connect_attrs: HashMap::new(),
            }
        );
    }

    #[test]
    fn decodes_stmt_variants() {
        assert_eq!(
            Command::decode(&[COM_SET_OPTION, 0x01, 0x00], 0).unwrap(),
            Command::SetOption { option: 1 }
        );
        assert_eq!(
            Command::decode(&[COM_STMT_CLOSE, 0x04, 0x00, 0x00, 0x00], 0).unwrap(),
            Command::StmtClose { stmt_id: 4 }
        );
        assert_eq!(
            Command::decode(&[COM_STMT_RESET, 0x05, 0x00, 0x00, 0x00], 0).unwrap(),
            Command::StmtReset { stmt_id: 5 }
        );
        assert_eq!(
            Command::decode(
                &[
                    COM_STMT_FETCH,
                    0x06,
                    0x00,
                    0x00,
                    0x00,
                    0x10,
                    0x00,
                    0x00,
                    0x00
                ],
                0
            )
            .unwrap(),
            Command::StmtFetch {
                stmt_id: 6,
                num_rows: 16
            }
        );
        assert_eq!(
            Command::decode(
                &[
                    COM_STMT_SEND_LONG_DATA,
                    0x07,
                    0x00,
                    0x00,
                    0x00,
                    0x02,
                    0x00,
                    0xAA,
                    0xBB
                ],
                0
            )
            .unwrap(),
            Command::StmtSendLongData {
                stmt_id: 7,
                param_id: 2,
                data: Bytes::from_static(&[0xAA, 0xBB]),
            }
        );
    }

    #[test]
    fn decodes_stmt_execute_with_explicit_prepared_metadata() {
        let payload = [
            COM_STMT_EXECUTE,
            0x07,
            0x00,
            0x00,
            0x00,
            0x00,
            0x01,
            0x00,
            0x00,
            0x00,
            0x00,
            0x01,
            mysql_const::field_type::LONG,
            0x00,
            mysql_const::field_type::VAR_STRING,
            0x00,
            0x2A,
            0x00,
            0x00,
            0x00,
            0x02,
            b'h',
            b'i',
        ];
        assert_eq!(
            Command::decode(&payload, 0).unwrap(),
            Command::StmtExecute {
                stmt_id: 7,
                cursor_flags: 0x00,
                iteration_count: 1,
                parameter_payload: Bytes::copy_from_slice(&payload[10..]),
            }
        );
        let decoded = decode_stmt_execute_parameters(&payload[10..], 2, &[]).unwrap();
        assert_eq!(
            decoded.values,
            vec![Value::Int(42), Value::Bytes(Bytes::from_static(b"hi"))]
        );
        assert_eq!(decoded.new_types.unwrap().len(), 2);
    }

    #[test]
    fn decodes_stmt_execute_datetime_and_nulls() {
        let payload = [
            COM_STMT_EXECUTE,
            0x03,
            0x00,
            0x00,
            0x00,
            0x00,
            0x01,
            0x00,
            0x00,
            0x00,
            0x01,
            0x01,
            mysql_const::field_type::NULL,
            0x00,
            mysql_const::field_type::DATETIME,
            0x00,
            0x07,
            0xE8,
            0x07,
            0x04,
            0x0A,
            0x0C,
            0x22,
            0x38,
        ];
        assert_eq!(
            Command::decode(&payload, 0).unwrap(),
            Command::StmtExecute {
                stmt_id: 3,
                cursor_flags: 0x00,
                iteration_count: 1,
                parameter_payload: Bytes::copy_from_slice(&payload[10..]),
            }
        );
        let decoded = decode_stmt_execute_parameters(&payload[10..], 2, &[]).unwrap();
        assert_eq!(
            decoded.values,
            vec![
                Value::Null,
                Value::DateTime {
                    year: 2024,
                    month: 4,
                    day: 10,
                    hour: 12,
                    minute: 34,
                    second: 56,
                    micros: 0,
                }
            ]
        );
    }

    #[test]
    fn rejects_empty_or_unsupported_commands() {
        assert!(Command::decode(&[], 0).is_err());
        assert!(Command::decode(&[0xFF], 0).is_err());
    }
}
