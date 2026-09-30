use bitflags::bitflags;

bitflags! {
    /// MySQL client/server capability flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CapabilityFlags: u32 {
        const CLIENT_LONG_PASSWORD                  = 0x0000_0001;
        const CLIENT_FOUND_ROWS                     = 0x0000_0002;
        const CLIENT_LONG_FLAG                      = 0x0000_0004;
        const CLIENT_CONNECT_WITH_DB                = 0x0000_0008;
        const CLIENT_NO_SCHEMA                      = 0x0000_0010;
        const CLIENT_COMPRESS                       = 0x0000_0020;
        const CLIENT_ODBC                           = 0x0000_0040;
        const CLIENT_LOCAL_FILES                    = 0x0000_0080;
        const CLIENT_IGNORE_SPACE                   = 0x0000_0100;
        const CLIENT_PROTOCOL_41                    = 0x0000_0200;
        const CLIENT_INTERACTIVE                    = 0x0000_0400;
        const CLIENT_SSL                            = 0x0000_0800;
        const CLIENT_TRANSACTIONS                   = 0x0000_2000;
        const CLIENT_SECURE_CONNECTION              = 0x0000_8000;
        const CLIENT_MULTI_STATEMENTS               = 0x0001_0000;
        const CLIENT_MULTI_RESULTS                  = 0x0002_0000;
        const CLIENT_PS_MULTI_RESULTS               = 0x0004_0000;
        const CLIENT_PLUGIN_AUTH                    = 0x0008_0000;
        const CLIENT_CONNECT_ATTRS                  = 0x0010_0000;
        const CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA = 0x0020_0000;
        const CLIENT_CAN_HANDLE_EXPIRED_PASSWORDS   = 0x0040_0000;
        const CLIENT_SESSION_TRACK                  = 0x0080_0000;
        const CLIENT_DEPRECATE_EOF                  = 0x0100_0000;
        const CLIENT_OPTIONAL_RESULTSET_METADATA    = 0x0200_0000;
        const CLIENT_ZSTD_COMPRESSION_ALGORITHM     = 0x0400_0000;
        const CLIENT_QUERY_ATTRIBUTES               = 0x0800_0000;
    }
}

/// The default set of capabilities advertised by the proxy during handshake.
pub const SERVER_DEFAULT: CapabilityFlags = CapabilityFlags::CLIENT_LONG_PASSWORD
    .union(CapabilityFlags::CLIENT_LONG_FLAG)
    .union(CapabilityFlags::CLIENT_CONNECT_WITH_DB)
    .union(CapabilityFlags::CLIENT_PROTOCOL_41)
    .union(CapabilityFlags::CLIENT_TRANSACTIONS)
    .union(CapabilityFlags::CLIENT_SECURE_CONNECTION)
    .union(CapabilityFlags::CLIENT_MULTI_RESULTS)
    .union(CapabilityFlags::CLIENT_PS_MULTI_RESULTS)
    .union(CapabilityFlags::CLIENT_PLUGIN_AUTH)
    .union(CapabilityFlags::CLIENT_CONNECT_ATTRS)
    .union(CapabilityFlags::CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA)
    .union(CapabilityFlags::CLIENT_SESSION_TRACK)
    .union(CapabilityFlags::CLIENT_DEPRECATE_EOF);
