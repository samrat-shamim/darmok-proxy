use crate::wire::{invalid_packet, unsupported};
use bytes::{BufMut, Bytes, BytesMut};
use darmok_types::error::Result;
use darmok_types::mysql_const::{column_flag, field_type};
use darmok_types::query::ColumnMeta;
use darmok_types::value::Value;

use crate::wire::{write_lenenc_bytes, write_lenenc_int};

/// MySQL column definition (COM_QUERY response, column metadata).
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDefinition {
    pub catalog: Bytes,
    pub schema: Bytes,
    pub table: Bytes,
    pub org_table: Bytes,
    pub name: Bytes,
    pub org_name: Bytes,
    pub character_set: u16,
    pub column_length: u32,
    pub column_type: u8,
    pub flags: u16,
    pub decimals: u8,
}

impl ColumnDefinition {
    /// Encode this column definition into the destination buffer.
    pub fn encode(&self, dst: &mut BytesMut) {
        write_lenenc_bytes(dst, self.catalog.as_ref());
        write_lenenc_bytes(dst, self.schema.as_ref());
        write_lenenc_bytes(dst, self.table.as_ref());
        write_lenenc_bytes(dst, self.org_table.as_ref());
        write_lenenc_bytes(dst, self.name.as_ref());
        write_lenenc_bytes(dst, self.org_name.as_ref());
        dst.put_u8(0x0C);
        dst.put_u16_le(self.character_set);
        dst.put_u32_le(self.column_length);
        dst.put_u8(self.column_type);
        dst.put_u16_le(self.flags);
        dst.put_u8(self.decimals);
        dst.put_u16_le(0);
    }
}

/// Encode the result set header (column count) as a length-encoded integer.
///
/// This is the first packet in both text and binary result set responses.
pub fn encode_result_set_header(column_count: u64, dst: &mut BytesMut) {
    write_lenenc_int(dst, column_count);
}

/// Encode a `COM_STMT_PREPARE_OK` payload.
pub fn encode_stmt_prepare_ok(
    stmt_id: u32,
    num_columns: u16,
    num_params: u16,
    warnings: u16,
    dst: &mut BytesMut,
) {
    dst.put_u8(0x00);
    dst.put_u32_le(stmt_id);
    dst.put_u16_le(num_columns);
    dst.put_u16_le(num_params);
    dst.put_u8(0x00);
    dst.put_u16_le(warnings);
}

/// Encode a MySQL binary-protocol NULL bitmap.
pub fn encode_null_bitmap(nulls: &[bool], offset: usize, dst: &mut BytesMut) {
    let bitmap_len = (nulls.len() + offset).div_ceil(8);
    let mut bitmap = vec![0u8; bitmap_len];

    for (index, is_null) in nulls.iter().copied().enumerate() {
        if !is_null {
            continue;
        }

        let bit_index = index + offset;
        bitmap[bit_index / 8] |= 1 << (bit_index % 8);
    }

    dst.extend_from_slice(&bitmap);
}

/// Encode a binary row against the metadata already sent to the client.
/// Integer width and signedness come from the column, never the value's Rust
/// representation. An error restores the destination's original length.
/// No per-cell buffers or NULL-bitmap allocations are required.
pub fn encode_binary_row(
    values: &[Value],
    columns: &[ColumnMeta],
    dst: &mut BytesMut,
) -> Result<()> {
    if values.len() != columns.len() {
        return Err(invalid_packet("binary row column count mismatch"));
    }
    let start = dst.len();
    dst.put_u8(0);
    let bitmap_start = dst.len();
    dst.resize(bitmap_start + (values.len() + 2).div_ceil(8), 0);
    for (index, (value, column)) in values.iter().zip(columns).enumerate() {
        if matches!(value, Value::Null) {
            let bit = index + 2;
            dst[bitmap_start + bit / 8] |= 1 << (bit % 8);
        } else if let Err(error) = encode_binary_value(value, column, dst) {
            dst.truncate(start);
            return Err(error);
        }
    }
    Ok(())
}

fn encode_binary_value(value: &Value, column: &ColumnMeta, dst: &mut BytesMut) -> Result<()> {
    let unsigned = column.flags & column_flag::UNSIGNED != 0;
    let kind = column.mysql_type;
    match kind {
        field_type::TINY
        | field_type::SHORT
        | field_type::LONG
        | field_type::INT24
        | field_type::LONGLONG
        | field_type::YEAR => {
            let value = match value {
                Value::Bool(value) => i128::from(*value),
                Value::Int(value) => i128::from(*value),
                Value::UInt(value) => i128::from(*value),
                _ => return Err(binary_type_error(kind)),
            };
            let range_error = || invalid_packet("integer outside advertised binary column range");
            match (kind, unsigned) {
                (field_type::TINY, false) => {
                    dst.put_i8(i8::try_from(value).map_err(|_| range_error())?)
                }
                (field_type::TINY, true) => {
                    dst.put_u8(u8::try_from(value).map_err(|_| range_error())?)
                }
                (field_type::SHORT | field_type::YEAR, false) => {
                    dst.put_i16_le(i16::try_from(value).map_err(|_| range_error())?)
                }
                (field_type::SHORT | field_type::YEAR, true) => {
                    dst.put_u16_le(u16::try_from(value).map_err(|_| range_error())?)
                }
                (field_type::LONG | field_type::INT24, false) => {
                    dst.put_i32_le(i32::try_from(value).map_err(|_| range_error())?)
                }
                (field_type::LONG | field_type::INT24, true) => {
                    dst.put_u32_le(u32::try_from(value).map_err(|_| range_error())?)
                }
                (_, false) => dst.put_i64_le(i64::try_from(value).map_err(|_| range_error())?),
                (_, true) => dst.put_u64_le(u64::try_from(value).map_err(|_| range_error())?),
            }
        }
        field_type::FLOAT => match value {
            Value::Float(value) => {
                let single = *value as f32;
                if value.is_finite() && !single.is_finite() {
                    return Err(invalid_packet(
                        "float outside advertised binary column range",
                    ));
                }
                dst.put_f32_le(single);
            }
            _ => return Err(binary_type_error(kind)),
        },
        field_type::DOUBLE => match value {
            Value::Float(value) => dst.put_f64_le(*value),
            _ => return Err(binary_type_error(kind)),
        },
        field_type::DECIMAL | field_type::NEWDECIMAL => match value {
            Value::Decimal(value) => write_lenenc_bytes(dst, value.as_bytes()),
            _ => return Err(binary_type_error(kind)),
        },
        field_type::VAR_STRING
        | field_type::STRING
        | field_type::VARCHAR
        | field_type::JSON
        | field_type::BLOB
        | field_type::BIT => match value {
            Value::String(value) => write_lenenc_bytes(dst, value.as_bytes()),
            Value::Bytes(value) => write_lenenc_bytes(dst, value),
            _ => return Err(binary_type_error(kind)),
        },
        field_type::DATE => match value {
            Value::Date { year, month, day } => {
                if *year == 0 && *month == 0 && *day == 0 {
                    dst.put_u8(0);
                } else {
                    dst.put_u8(4);
                    dst.put_u16_le(*year);
                    dst.put_u8(*month);
                    dst.put_u8(*day);
                }
            }
            _ => return Err(binary_type_error(kind)),
        },
        field_type::DATETIME | field_type::TIMESTAMP => match value {
            Value::DateTime {
                year,
                month,
                day,
                hour,
                minute,
                second,
                micros,
            } => {
                let length = if *micros != 0 {
                    11
                } else if *hour != 0 || *minute != 0 || *second != 0 {
                    7
                } else if *year != 0 || *month != 0 || *day != 0 {
                    4
                } else {
                    0
                };
                dst.put_u8(length);
                if length >= 4 {
                    dst.put_u16_le(*year);
                    dst.put_u8(*month);
                    dst.put_u8(*day);
                }
                if length >= 7 {
                    dst.put_u8(*hour);
                    dst.put_u8(*minute);
                    dst.put_u8(*second);
                }
                if length == 11 {
                    dst.put_u32_le(*micros);
                }
            }
            _ => return Err(binary_type_error(kind)),
        },
        field_type::TIME => match value {
            Value::Time {
                negative,
                days,
                hours,
                minutes,
                seconds,
                micros,
            } => {
                let length = if *micros != 0 {
                    12
                } else if *days != 0 || *hours != 0 || *minutes != 0 || *seconds != 0 {
                    8
                } else {
                    0
                };
                dst.put_u8(length);
                if length != 0 {
                    dst.put_u8(u8::from(*negative));
                    dst.put_u32_le(*days);
                    dst.put_u8(*hours);
                    dst.put_u8(*minutes);
                    dst.put_u8(*seconds);
                }
                if length == 12 {
                    dst.put_u32_le(*micros);
                }
            }
            _ => return Err(binary_type_error(kind)),
        },
        _ => return Err(binary_type_error(kind)),
    }
    Ok(())
}

fn binary_type_error(field_type: u8) -> darmok_types::error::ProxyError {
    // Never include the value (or a user-controlled identifier) in diagnostics.
    unsupported(format!(
        "value cannot be encoded for binary field type 0x{field_type:02x}"
    ))
}

/// Encode a text-protocol result row.
///
/// Each value is either a length-encoded string (`Some(bytes)`) or NULL
/// (encoded as the single byte `0xFB`). This is the row format used in
/// `COM_QUERY` responses (text protocol).
pub fn encode_text_row(values: &[Option<&[u8]>], dst: &mut BytesMut) {
    for value in values {
        match value {
            Some(bytes) => write_lenenc_bytes(dst, bytes),
            None => dst.put_u8(0xFB),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(mysql_type: u8, unsigned: bool) -> ColumnMeta {
        ColumnMeta {
            name: "value".into(),
            table: None,
            source: None,
            mysql_type,
            flags: if unsigned { column_flag::UNSIGNED } else { 0 },
            charset_id: 63,
            max_length: 0,
            decimals: 0,
        }
    }

    #[test]
    fn binary_integer_widths_signedness_and_float_bytes_match_the_wire() {
        let values = vec![
            Value::Int(-1),
            Value::UInt(65535),
            Value::Int(258),
            Value::UInt(u64::MAX),
            Value::Float(1.5),
            Value::Float(-2.0),
            Value::Null,
        ];
        let columns = vec![
            column(field_type::TINY, false),
            column(field_type::SHORT, true),
            column(field_type::LONG, false),
            column(field_type::LONGLONG, true),
            column(field_type::FLOAT, false),
            column(field_type::DOUBLE, false),
            column(field_type::VAR_STRING, false),
        ];
        let mut dst = BytesMut::new();
        encode_binary_row(&values, &columns, &mut dst).unwrap();
        assert_eq!(
            dst.as_ref(),
            &[
                0, 0, 1, // Row header and two-byte NULL bitmap; column 6 is NULL.
                0xff, 0xff, 0xff, 2, 1, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0,
                0xc0, 0x3f, // f32 1.5
                0, 0, 0, 0, 0, 0, 0, 0xc0, // f64 -2.0
            ]
        );
    }

    #[test]
    fn binary_values_must_fit_advertised_types_and_fail_atomically() {
        for (value, kind, unsigned) in [
            (Value::UInt(128), field_type::TINY, false),
            (Value::Int(-1), field_type::LONG, true),
            (Value::UInt(u64::MAX), field_type::LONGLONG, false),
            (Value::Int(65536), field_type::SHORT, true),
            (Value::Float(f64::MAX), field_type::FLOAT, false),
            (Value::String("secret".into()), field_type::LONG, false),
        ] {
            let mut dst = BytesMut::from(b"prefix".as_slice());
            let error = encode_binary_row(
                &[Value::Int(1), value],
                &[column(field_type::TINY, false), column(kind, unsigned)],
                &mut dst,
            )
            .unwrap_err();
            assert_eq!(dst.as_ref(), b"prefix");
            assert!(!format!("{error:?}").contains("secret"));
        }
        let mut dst = BytesMut::from(b"prefix".as_slice());
        assert!(encode_binary_row(&[], &[column(field_type::LONG, false)], &mut dst).is_err());
        assert_eq!(dst.as_ref(), b"prefix");
    }

    #[test]
    fn binary_temporal_values_use_length_fields_and_little_endian_components() {
        let values = vec![
            Value::Date {
                year: 0,
                month: 0,
                day: 0,
            },
            Value::Date {
                year: 2024,
                month: 4,
                day: 10,
            },
            Value::DateTime {
                year: 2024,
                month: 4,
                day: 10,
                hour: 12,
                minute: 34,
                second: 56,
                micros: 1,
            },
            Value::Time {
                negative: true,
                days: 2,
                hours: 3,
                minutes: 4,
                seconds: 5,
                micros: 6,
            },
        ];
        let columns = vec![
            column(field_type::DATE, false),
            column(field_type::DATE, false),
            column(field_type::DATETIME, false),
            column(field_type::TIME, false),
        ];
        let mut dst = BytesMut::new();
        encode_binary_row(&values, &columns, &mut dst).unwrap();
        assert_eq!(
            dst.as_ref(),
            &[
                0, 0, 0, 4, 0xe8, 7, 4, 10, 11, 0xe8, 7, 4, 10, 12, 34, 56, 1, 0, 0, 0, 12, 1, 2,
                0, 0, 0, 3, 4, 5, 6, 0, 0, 0,
            ]
        );
    }

    #[test]
    fn encodes_column_definition_packet() {
        let column = ColumnDefinition {
            catalog: Bytes::from_static(b"def"),
            schema: Bytes::from_static(b"app"),
            table: Bytes::from_static(b"users"),
            org_table: Bytes::from_static(b"users"),
            name: Bytes::from_static(b"id"),
            org_name: Bytes::from_static(b"id"),
            character_set: 45,
            column_length: 11,
            column_type: 0x03,
            flags: 0x0023,
            decimals: 0,
        };

        let mut dst = BytesMut::new();
        column.encode(&mut dst);

        assert_eq!(
            dst.as_ref(),
            &[
                0x03, b'd', b'e', b'f', 0x03, b'a', b'p', b'p', 0x05, b'u', b's', b'e', b'r', b's',
                0x05, b'u', b's', b'e', b'r', b's', 0x02, b'i', b'd', 0x02, b'i', b'd', 0x0C, 0x2D,
                0x00, 0x0B, 0x00, 0x00, 0x00, 0x03, 0x23, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }

    #[test]
    fn encodes_result_set_header() {
        let mut dst = BytesMut::new();
        encode_result_set_header(3, &mut dst);
        assert_eq!(dst.as_ref(), &[0x03]);

        let mut dst = BytesMut::new();
        encode_result_set_header(300, &mut dst);
        assert_eq!(dst.as_ref(), &[0xFC, 0x2C, 0x01]);
    }

    #[test]
    fn encodes_stmt_prepare_ok() {
        let mut dst = BytesMut::new();
        encode_stmt_prepare_ok(0x1234_5678, 2, 3, 4, &mut dst);

        assert_eq!(
            dst.as_ref(),
            &[
                0x00, 0x78, 0x56, 0x34, 0x12, 0x02, 0x00, 0x03, 0x00, 0x00, 0x04, 0x00
            ]
        );
    }

    #[test]
    fn encodes_null_bitmap_all_null() {
        let mut dst = BytesMut::new();
        encode_null_bitmap(&[true, true, true, true, true, true], 2, &mut dst);

        assert_eq!(dst.as_ref(), &[0xFC]);
    }

    #[test]
    fn encodes_null_bitmap_all_non_null() {
        let mut dst = BytesMut::new();
        encode_null_bitmap(&[false, false, false, false, false], 0, &mut dst);

        assert_eq!(dst.as_ref(), &[0x00]);
    }

    #[test]
    fn encodes_null_bitmap_mixed_values() {
        let mut dst = BytesMut::new();
        encode_null_bitmap(
            &[
                false, true, false, true, false, false, true, false, true, true,
            ],
            2,
            &mut dst,
        );

        assert_eq!(dst.as_ref(), &[0x28, 0x0D]);
    }

    #[test]
    fn encodes_binary_row_with_values_and_nulls() {
        let mut dst = BytesMut::new();
        let columns = vec![column(field_type::VAR_STRING, false); 4];
        encode_binary_row(
            &[
                Value::String("foo".into()),
                Value::Null,
                Value::String("barbaz".into()),
                Value::Null,
            ],
            &columns,
            &mut dst,
        )
        .unwrap();

        assert_eq!(
            dst.as_ref(),
            &[
                0x00, // binary row header
                0x28, // null bitmap with offset 2
                0x03, b'f', b'o', b'o', // lenenc "foo"
                0x06, b'b', b'a', b'r', b'b', b'a', b'z', // lenenc "barbaz"
            ]
        );
    }

    #[test]
    fn encodes_text_row_with_values_and_nulls() {
        let mut dst = BytesMut::new();
        encode_text_row(&[Some(b"hello"), None, Some(b"world")], &mut dst);
        assert_eq!(
            dst.as_ref(),
            &[
                0x05, b'h', b'e', b'l', b'l', b'o', // lenenc "hello"
                0xFB, // NULL
                0x05, b'w', b'o', b'r', b'l', b'd', // lenenc "world"
            ]
        );
    }

    #[test]
    fn encodes_text_row_all_nulls() {
        let mut dst = BytesMut::new();
        encode_text_row(&[None, None], &mut dst);
        assert_eq!(dst.as_ref(), &[0xFB, 0xFB]);
    }

    #[test]
    fn encodes_text_row_empty() {
        let mut dst = BytesMut::new();
        encode_text_row(&[], &mut dst);
        assert!(dst.is_empty());
    }
}
