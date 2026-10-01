//! Native representations, prepared checks, borrowed bindings, observed
//! controls and exclusively owned transaction scopes. Semantically admitted
//! statement execution and wire metadata remain gates.

mod native_backend;
mod native_control;
mod native_param;
mod native_statement;
mod native_transaction;
mod native_value;
mod numeric;

pub use native_backend::{
    NativeBackend, NativeBackendDisposeError, NativeBackendDisposed, NativeBackendError,
    NativeBackendOperation, NativeBackendState, NativeRecovery, NativeScope, NativeScopeBoundary,
};
pub use native_control::{
    NativeControl, NativeControlCompletion, NativeControlFailure, NativeControlMismatch,
    check_native_control,
};
pub use native_param::{NativeParamError, NativeParamUtc};
pub use native_statement::{NativeBindingsUtc, NativeStatementError, NativeStatementUtc};
pub use native_transaction::{NativeIsolation, NativeTransactionAccess, NativeTransactionSpec};
pub use native_value::{NativeValueError, decode_native_row_utc};
