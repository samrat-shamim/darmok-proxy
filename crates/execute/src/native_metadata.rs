//! Native scalar representations for MySQL result fields. Semantic identities,
//! nullability, keys and execution validity belong to admission, not this map.

use darmok_session::charset::lookup_charset;
use darmok_types::mysql_const::{charset, column_flag as flag, field_type as ty};
use tokio_postgres::{Column, types::Type};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeMetadataError {
    #[error("native output has unsupported PostgreSQL type OID {oid}")]
    UnsupportedType { oid: u32 },
    #[error("native output has unsupported text collation {id}")]
    UnsupportedTextCollation { id: u16 },
    #[error("native type OID {oid} has unsupported modifier {modifier}: {reason}")]
    Modifier {
        oid: u32,
        modifier: i32,
        reason: &'static str,
    },
    #[error("unconstrained native NUMERIC has no fixed MySQL precision and scale")]
    UnconstrainedNumeric,
    #[error("native NUMERIC({precision},{scale}) cannot use MySQL DECIMAL metadata")]
    NumericDeclaration { precision: u16, scale: i16 },
}

/// An immutable scalar representation, not a complete ColumnDefinition or an
/// executable plan. Admission must supply names, origins and semantic flags.
/// In particular, neither a RowDescription nor a type proves NOT NULL or keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeTypeMetadataUtc {
    column_type: u8,
    character_set: u16,
    column_length: u32,
    decimals: u8,
    flags: u16,
}

impl NativeTypeMetadataUtc {
    /// Derive representations from the reported native type and typmod, never
    /// a name, sample value or a stale declared type. Only profile-declared
    /// UTF-8 and binary text encodings are accepted. This does not establish
    /// collation comparison equivalence, range validity or catalog coherence.
    pub fn new(
        native_type: &Type,
        modifier: i32,
        text_collation: u16,
    ) -> Result<Self, NativeMetadataError> {
        let text = lookup_charset(text_collation)
            .filter(|info| matches!(info.name, "utf8mb3" | "utf8mb4" | "binary"))
            .ok_or(NativeMetadataError::UnsupportedTextCollation { id: text_collation })?;
        let binary = |kind, length, decimals, flags| Self {
            column_type: kind,
            character_set: charset::BINARY,
            column_length: length,
            decimals,
            flags: flags | flag::BINARY,
        };
        let error = |reason| NativeMetadataError::Modifier {
            oid: native_type.oid(),
            modifier,
            reason,
        };
        let plain = |kind, length, decimals, flags| {
            if modifier != -1 {
                Err(error("this native scalar has no type modifier"))
            } else {
                Ok(binary(kind, length, decimals, flags))
            }
        };
        match *native_type {
            Type::BOOL => plain(ty::TINY, 1, 0, 0),
            Type::INT2 => plain(ty::SHORT, 6, 0, 0),
            Type::INT4 => plain(ty::LONG, 11, 0, 0),
            Type::INT8 => plain(ty::LONGLONG, 20, 0, 0),
            Type::OID => plain(ty::LONG, 10, 0, flag::UNSIGNED),
            Type::FLOAT4 => plain(ty::FLOAT, 12, 31, 0),
            Type::FLOAT8 => plain(ty::DOUBLE, 22, 31, 0),
            Type::BYTEA => plain(ty::BLOB, u32::MAX, 0, flag::BLOB),
            Type::JSON | Type::JSONB => plain(ty::JSON, u32::MAX, 0, flag::BLOB),
            Type::DATE => plain(ty::DATE, 10, 0, 0),
            Type::NUMERIC => {
                if modifier == -1 {
                    return Err(NativeMetadataError::UnconstrainedNumeric);
                }
                let packed = modifier
                    .checked_sub(4)
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| error("numeric modifier must include its four-byte header"))?;
                // PostgreSQL uses 16 precision bits and a signed 11-bit scale;
                // the intervening five bits are reserved, not extra scale bits.
                if packed & 0xf800 != 0 {
                    return Err(error("numeric modifier has reserved scale bits"));
                }
                let precision = ((packed >> 16) & 0xffff) as u16;
                let scale = (((packed & 0x7ff) ^ 0x400) - 0x400) as i16;
                if !(1..=65).contains(&precision)
                    || !(0..=30).contains(&scale)
                    || i32::from(scale) > i32::from(precision)
                {
                    return Err(NativeMetadataError::NumericDeclaration { precision, scale });
                }
                Ok(binary(
                    ty::NEWDECIMAL,
                    u32::from(precision) + 1 + u32::from(scale != 0),
                    scale as u8,
                    0,
                ))
            }
            Type::TIME | Type::TIMESTAMP | Type::TIMESTAMPTZ => {
                let precision = if modifier == -1 { 6 } else { modifier };
                if !(0..=6).contains(&precision) {
                    return Err(error("temporal precision must be unspecified or 0..6"));
                }
                let (kind, width) = if *native_type == Type::TIME {
                    (ty::TIME, 10)
                } else {
                    (ty::DATETIME, 19)
                };
                Ok(binary(
                    kind,
                    width
                        + if precision == 0 {
                            0
                        } else {
                            precision as u32 + 1
                        },
                    precision as u8,
                    0,
                ))
            }
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => {
                let length = if matches!(*native_type, Type::VARCHAR | Type::BPCHAR) {
                    if modifier == -1 {
                        u32::MAX
                    } else {
                        let characters = modifier
                            .checked_sub(4)
                            .filter(|value| *value > 0)
                            .ok_or_else(|| {
                                error("character modifier must declare a positive length")
                            })?;
                        // Binary output still sends native UTF-8 bytes: a binary
                        // session's max_len=1 is not the native storage width.
                        let bytes_per_character = if text.name == "utf8mb3" { 3 } else { 4 };
                        (characters as u32)
                            .checked_mul(bytes_per_character)
                            .ok_or_else(|| error("character byte length exceeds the MySQL field"))?
                    }
                } else {
                    if modifier != -1 {
                        return Err(error("this native text scalar has no type modifier"));
                    }
                    // No server NAMEDATALEN or varlena limit is guessed here.
                    u32::MAX
                };
                let blob = *native_type == Type::TEXT;
                Ok(Self {
                    column_type: if blob { ty::BLOB } else { ty::VAR_STRING },
                    character_set: text.id,
                    column_length: length,
                    decimals: 0,
                    flags: if blob { flag::BLOB } else { 0 }
                        | if text.name == "binary" {
                            flag::BINARY
                        } else {
                            0
                        },
                })
            }
            _ => Err(NativeMetadataError::UnsupportedType {
                oid: native_type.oid(),
            }),
        }
    }

    /// Use an actual native result description. This does not prove its origin
    /// or authorize using a prepared description after external DDL.
    pub fn from_column(column: &Column, text_collation: u16) -> Result<Self, NativeMetadataError> {
        Self::new(column.type_(), column.type_modifier(), text_collation)
    }

    pub fn column_type(self) -> u8 {
        self.column_type
    }
    pub fn character_set(self) -> u16 {
        self.character_set
    }
    /// A display width for numbers/temporal fields, a byte bound for strings.
    pub fn column_length(self) -> u32 {
        self.column_length
    }
    pub fn decimals(self) -> u8 {
        self.decimals
    }
    /// Representation flags only. No nullability, keys, identity or defaults.
    pub fn flags(self) -> u16 {
        self.flags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numeric_modifier(precision: i32, scale: i32) -> i32 {
        4 + (precision << 16) + (scale & 0x7ff)
    }

    #[test]
    fn numeric_declarations_do_not_lose_sign_or_reserved_bits() {
        for (precision, scale) in [(2, -1), (65, -1000), (2, 3), (66, 0), (65, 31), (0, 0)] {
            assert_eq!(
                NativeTypeMetadataUtc::new(&Type::NUMERIC, numeric_modifier(precision, scale), 45),
                Err(NativeMetadataError::NumericDeclaration {
                    precision: precision as u16,
                    scale: scale as i16,
                })
            );
        }
        for modifier in [i32::MIN, -2, 0, 3, numeric_modifier(12, 3) | 0x8000] {
            assert!(matches!(
                NativeTypeMetadataUtc::new(&Type::NUMERIC, modifier, 45),
                Err(NativeMetadataError::Modifier { .. })
            ));
        }
        assert_eq!(
            NativeTypeMetadataUtc::new(&Type::NUMERIC, -1, 45),
            Err(NativeMetadataError::UnconstrainedNumeric)
        );
    }

    #[test]
    fn native_utf8_bytes_keep_their_width_in_binary_sessions() {
        for (collation, length) in [(45, 8), (33, 6), (63, 8)] {
            let metadata = NativeTypeMetadataUtc::new(&Type::VARCHAR, 6, collation).unwrap();
            assert_eq!(metadata.column_length(), length);
            assert_eq!(metadata.character_set(), collation);
            assert_eq!(
                metadata.flags(),
                if collation == 63 { flag::BINARY } else { 0 }
            );
        }
        for modifier in [i32::MIN, -2, 0, 4, i32::MAX] {
            assert!(matches!(
                NativeTypeMetadataUtc::new(&Type::BPCHAR, modifier, 45),
                Err(NativeMetadataError::Modifier { .. })
            ));
        }
    }

    #[test]
    fn scalar_modifiers_and_output_encodings_are_not_guessed() {
        for native_type in [Type::INT4, Type::BOOL, Type::JSONB, Type::NAME, Type::BYTEA] {
            assert!(matches!(
                NativeTypeMetadataUtc::new(&native_type, 0, 45),
                Err(NativeMetadataError::Modifier { .. })
            ));
        }
        for modifier in [-2, 7, i32::MAX] {
            assert!(matches!(
                NativeTypeMetadataUtc::new(&Type::TIMESTAMPTZ, modifier, 45),
                Err(NativeMetadataError::Modifier { .. })
            ));
        }
        for id in [0, 8, 51, u16::MAX] {
            assert_eq!(
                NativeTypeMetadataUtc::new(&Type::INT4, -1, id),
                Err(NativeMetadataError::UnsupportedTextCollation { id })
            );
        }
        for native_type in [Type::UUID, Type::INT4_ARRAY, Type::INTERVAL, Type::RECORD] {
            assert_eq!(
                NativeTypeMetadataUtc::new(&native_type, -1, 45),
                Err(NativeMetadataError::UnsupportedType {
                    oid: native_type.oid()
                })
            );
        }
    }
}
