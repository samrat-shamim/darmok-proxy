use bytes::Bytes;

/// Represents a MySQL/PostgreSQL value in transit through the proxy.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    /// Exact decimal representation — no f64 precision loss.
    Decimal(Box<str>),
    String(Box<str>),
    Bytes(Bytes),
    Date {
        year: u16,
        month: u8,
        day: u8,
    },
    Time {
        negative: bool,
        days: u32,
        hours: u8,
        minutes: u8,
        seconds: u8,
        micros: u32,
    },
    DateTime {
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        micros: u32,
    },
}

/// Binary-protocol parameter type metadata last sent by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedStatementParamType {
    pub field_type: u8,
    pub unsigned: bool,
}
