/// PostgreSQL type OIDs.
pub mod oid {
    pub const BOOL: u32 = 16;
    pub const BYTEA: u32 = 17;
    pub const INT8: u32 = 20;
    pub const INT2: u32 = 21;
    pub const INT4: u32 = 23;
    pub const TEXT: u32 = 25;
    pub const OID_TYPE: u32 = 26;
    pub const JSON: u32 = 114;
    pub const FLOAT4: u32 = 700;
    pub const FLOAT8: u32 = 701;
    pub const BPCHAR: u32 = 1042;
    pub const VARCHAR: u32 = 1043;
    pub const DATE: u32 = 1082;
    pub const TIME: u32 = 1083;
    pub const TIMESTAMP: u32 = 1114;
    pub const TIMESTAMPTZ: u32 = 1184;
    pub const NUMERIC: u32 = 1700;
    pub const UUID: u32 = 2950;
    pub const JSONB: u32 = 3802;
}

/// PostgreSQL SQLSTATE values (5-byte ASCII arrays).
pub mod sqlstate {
    pub const SUCCESSFUL_COMPLETION: [u8; 5] = *b"00000";
    pub const NOT_NULL_VIOLATION: [u8; 5] = *b"23502";
    pub const FOREIGN_KEY_VIOLATION: [u8; 5] = *b"23503";
    pub const UNIQUE_VIOLATION: [u8; 5] = *b"23505";
    pub const SERIALIZATION_FAILURE: [u8; 5] = *b"40001";
    pub const DEADLOCK_DETECTED: [u8; 5] = *b"40P01";
    pub const INSUFFICIENT_PRIVILEGE: [u8; 5] = *b"42501";
    pub const SYNTAX_ERROR: [u8; 5] = *b"42601";
    pub const UNDEFINED_COLUMN: [u8; 5] = *b"42703";
    pub const UNDEFINED_TABLE: [u8; 5] = *b"42P01";
    pub const DUPLICATE_TABLE: [u8; 5] = *b"42P07";
    pub const TOO_MANY_CONNECTIONS: [u8; 5] = *b"53300";
    pub const DISK_FULL: [u8; 5] = *b"53100";
    pub const IN_FAILED_SQL_TRANSACTION: [u8; 5] = *b"25P02";
}
