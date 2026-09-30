//! Native backend value decoding and typed parameter encoding. Connection
//! ownership, statement execution and wire metadata remain integration gates.

mod native_param;
mod native_value;
mod numeric;

pub use native_param::{NativeParamError, NativeParamUtc};
pub use native_value::{NativeValueError, decode_native_row_utc};
