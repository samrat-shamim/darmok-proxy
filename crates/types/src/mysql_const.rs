/// MySQL wire protocol field (column) types.
pub mod field_type {
    pub const DECIMAL: u8 = 0x00;
    pub const TINY: u8 = 0x01;
    pub const SHORT: u8 = 0x02;
    pub const LONG: u8 = 0x03;
    pub const FLOAT: u8 = 0x04;
    pub const DOUBLE: u8 = 0x05;
    pub const NULL: u8 = 0x06;
    pub const TIMESTAMP: u8 = 0x07;
    pub const LONGLONG: u8 = 0x08;
    pub const INT24: u8 = 0x09;
    pub const DATE: u8 = 0x0A;
    pub const TIME: u8 = 0x0B;
    pub const DATETIME: u8 = 0x0C;
    pub const YEAR: u8 = 0x0D;
    pub const VARCHAR: u8 = 0x0F;
    pub const BIT: u8 = 0x10;
    pub const JSON: u8 = 0xF5;
    pub const NEWDECIMAL: u8 = 0xF6;
    pub const BLOB: u8 = 0xFC;
    pub const VAR_STRING: u8 = 0xFD;
    pub const STRING: u8 = 0xFE;
}

/// MySQL column flags (bitmask).
pub mod column_flag {
    pub const NOT_NULL: u16 = 0x0001;
    pub const PRI_KEY: u16 = 0x0002;
    pub const UNIQUE_KEY: u16 = 0x0004;
    pub const MULTIPLE_KEY: u16 = 0x0008;
    pub const BLOB: u16 = 0x0010;
    pub const UNSIGNED: u16 = 0x0020;
    pub const ZEROFILL: u16 = 0x0040;
    pub const BINARY: u16 = 0x0080;
    pub const ENUM: u16 = 0x0100;
    pub const AUTO_INCREMENT: u16 = 0x0200;
    pub const TIMESTAMP: u16 = 0x0400;
    pub const SET: u16 = 0x0800;
    pub const NUM: u16 = 0x8000;
}

/// MySQL charset IDs.
pub mod charset {
    pub const BIG5_CHINESE_CI: u16 = 1;
    pub const KOI8R_GENERAL_CI: u16 = 7;
    pub const LATIN1_SWEDISH_CI: u16 = 8;
    pub const UJIS_JAPANESE_CI: u16 = 12;
    pub const HEBREW_GENERAL_CI: u16 = 16;
    pub const TIS620_THAI_CI: u16 = 18;
    pub const UTF8MB3_GENERAL_CI: u16 = 33;
    pub const UTF8MB4_GENERAL_CI: u16 = 45;
    pub const UTF8MB4_BIN: u16 = 46;
    pub const LATIN1_GENERAL_CI: u16 = 48;
    pub const LATIN1_GENERAL_CS: u16 = 49;
    pub const CP1251_GENERAL_CI: u16 = 51;
    pub const BINARY: u16 = 63;
    pub const UTF8MB3_BIN: u16 = 83;
    pub const UTF8MB3_UNICODE_CI: u16 = 192;
    pub const UTF8MB4_UNICODE_CI: u16 = 224;
    pub const UTF8MB4_0900_AI_CI: u16 = 255;
}

/// MySQL error codes.
pub mod error_code {
    pub const ER_DISK_FULL: u16 = 1021;
    pub const ER_DUP_KEY: u16 = 1022;
    pub const ER_CON_COUNT_ERROR: u16 = 1040;
    pub const ER_TOO_MANY_CONNECTIONS: u16 = 1040;
    pub const ER_DBACCESS_DENIED_ERROR: u16 = 1044;
    pub const ER_WRONG_DB_NAME: u16 = 1044;
    pub const ER_ACCESS_DENIED: u16 = 1045;
    pub const ER_BAD_NULL_ERROR: u16 = 1048;
    pub const ER_BAD_DB_ERROR: u16 = 1049;
    pub const ER_TABLE_EXISTS: u16 = 1050;
    pub const ER_UNKNOWN_TABLE: u16 = 1051;
    pub const ER_BAD_FIELD_ERROR: u16 = 1054;
    pub const ER_DUP_ENTRY: u16 = 1062;
    pub const ER_SYNTAX_ERROR: u16 = 1064;
    /// Alias for `ER_SYNTAX_ERROR`.
    pub const ER_PARSE_ERROR: u16 = 1064;
    pub const ER_UNKNOWN_ERROR: u16 = 1105;
    pub const ER_NO_SUCH_TABLE: u16 = 1146;
    pub const ER_UNKNOWN_SYSTEM_VARIABLE: u16 = 1193;
    pub const ER_LOCK_DEADLOCK: u16 = 1213;
    pub const ER_WRONG_VALUE_FOR_VAR: u16 = 1231;
    pub const ER_NOT_SUPPORTED_YET: u16 = 1235;
    pub const ER_NO_REFERENCED_ROW: u16 = 1452;
    pub const ER_CANT_CHANGE_TX_CHARACTERISTICS: u16 = 1568;
    pub const ER_SERVER_GONE: u16 = 2006;
}

/// MySQL SQLSTATE values (5-byte ASCII arrays).
pub mod sqlstate {
    pub const HY000: [u8; 5] = *b"HY000";
    pub const SYNTAX_ERROR: [u8; 5] = *b"42000";
    pub const AUTH_ERROR: [u8; 5] = *b"28000";
    pub const NO_SUCH_TABLE: [u8; 5] = *b"42S02";
    pub const TOO_MANY_CONNECTIONS: [u8; 5] = *b"08004";
    pub const INVALID_TRANSACTION_STATE: [u8; 5] = *b"25001";
}

/// MySQL server status flags (bitmask).
pub mod server_status {
    pub const IN_TRANS: u16 = 0x0001;
    pub const AUTOCOMMIT: u16 = 0x0002;
    pub const MORE_RESULTS_EXISTS: u16 = 0x0008;
    pub const NO_GOOD_INDEX_USED: u16 = 0x0010;
    pub const CURSOR_EXISTS: u16 = 0x0040;
    pub const LAST_ROW_SENT: u16 = 0x0080;
    pub const SESSION_STATE_CHANGED: u16 = 0x4000;
}
