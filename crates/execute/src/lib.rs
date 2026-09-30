//! Native backend representations, prepared description checks and borrowed
//! bindings. Connection ownership, execution and wire metadata remain gates.

mod native_param;
mod native_statement;
mod native_value;
mod numeric;

pub use native_param::{NativeParamError, NativeParamUtc};
pub use native_statement::{NativeBindingsUtc, NativeStatementError, NativeStatementUtc};
pub use native_value::{NativeValueError, decode_native_row_utc};
