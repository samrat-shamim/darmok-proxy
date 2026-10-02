//! Native representations, prepared checks, borrowed bindings, observed
//! controls, exclusively owned transaction scopes and source-admitted selected
//! COM_QUERY SET, transaction controls and local SELECT execution under an owned
//! TCP command phase. Table execution and the executable remain gates.

mod exact_number;
mod frontend_connection;
mod native_backend;
mod native_control;
mod native_param;
mod native_statement;
mod native_transaction;
mod native_value;
mod numeric;
mod query_controller;
mod select_controller;
mod set_controller;
mod transaction_controller;

pub use frontend_connection::{FrontendConnection, FrontendEnd, FrontendError, FrontendReport};
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
pub use query_controller::{QueryExecutionError, QuerySqlError};
#[cfg(test)]
pub(crate) use query_controller::{QueryOutcome, execute_query_command};
pub use select_controller::SelectSqlError;
pub use set_controller::{ServerSetValues, SetSqlError};
pub use transaction_controller::TransactionSqlError;
