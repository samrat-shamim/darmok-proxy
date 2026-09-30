// MySQL wire protocol command bytes.
pub const COM_QUIT: u8 = 0x01;
pub const COM_INIT_DB: u8 = 0x02;
pub const COM_QUERY: u8 = 0x03;
pub const COM_FIELD_LIST: u8 = 0x04;
pub const COM_CREATE_DB: u8 = 0x05;
pub const COM_DROP_DB: u8 = 0x06;
pub const COM_REFRESH: u8 = 0x07;
pub const COM_SHUTDOWN: u8 = 0x08;
pub const COM_STATISTICS: u8 = 0x09;
pub const COM_PROCESS_INFO: u8 = 0x0A;
pub const COM_PROCESS_KILL: u8 = 0x0C;
pub const COM_DEBUG: u8 = 0x0D;
pub const COM_PING: u8 = 0x0E;
pub const COM_CHANGE_USER: u8 = 0x11;
pub const COM_RESET_CONNECTION: u8 = 0x1F;
pub const COM_SET_OPTION: u8 = 0x1B;
pub const COM_STMT_PREPARE: u8 = 0x16;
pub const COM_STMT_EXECUTE: u8 = 0x17;
// Keep these aligned with the MySQL wire protocol spec:
// SEND_LONG_DATA = 0x18, CLOSE = 0x19.
pub const COM_STMT_SEND_LONG_DATA: u8 = 0x18;
pub const COM_STMT_CLOSE: u8 = 0x19;
pub const COM_STMT_RESET: u8 = 0x1A;
pub const COM_STMT_FETCH: u8 = 0x1C;

// Packet framing constants.
pub const PACKET_HEADER_LEN: usize = 4;
pub const MAX_PACKET_PAYLOAD_LEN: usize = 0x00FF_FFFF;
pub const DEFAULT_MAX_ALLOWED_PACKET: u32 = 64 * 1024 * 1024;

// Packet header markers.
pub const OK_HEADER: u8 = 0x00;
pub const ERR_HEADER: u8 = 0xFF;
pub const EOF_HEADER: u8 = 0xFE;
pub const LOCAL_INFILE_HEADER: u8 = 0xFB;

// Packet layout markers.
pub const SQL_STATE_MARKER: u8 = b'#';

// MySQL protocol-level errors not yet exposed in `darmok-types`.
pub const ER_NET_PACKET_TOO_LARGE: u16 = 1153;
pub const SQLSTATE_COMMUNICATION_ERROR: [u8; 5] = *b"08S01";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stmt_long_data_and_close_match_mysql_wire_spec() {
        assert_eq!(COM_STMT_SEND_LONG_DATA, 0x18);
        assert_eq!(COM_STMT_CLOSE, 0x19);
    }
}
