//! Backend value decoding. Connection ownership, statement execution and wire
//! metadata construction remain separate integration gates.

mod native_value;
mod numeric;

pub use native_value::{NativeValueError, decode_native_row_utc};
