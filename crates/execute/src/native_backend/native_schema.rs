use tokio_postgres::TransactionState;

use super::{NativeBackend, NativeBackendError, NativeBackendOperation, NativeBackendState};
use crate::{NativeControl, NativeControlCompletion};

pub(super) const SCHEMA_SQL: &str = include_str!("../../sql/compatibility-schema.sql");
const CREATE_MARKER: &str = "allow_create constant pg_catalog.bool := true;";

/// Version of the functional schema manifest checked by this component.
pub const NATIVE_SCHEMA_VERSION: i32 = 1;

/// An observed committed initialization or read-only functional verification.
/// It does not certify grants, credentials, runtime admission or catalog leases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct NativeSchemaCompletion {
    completion: NativeControlCompletion,
}

impl NativeSchemaCompletion {
    pub fn version(self) -> i32 {
        NATIVE_SCHEMA_VERSION
    }

    pub fn completion(self) -> NativeControlCompletion {
        self.completion
    }
}

/// The original failure and, when required by confirmed failed-transaction
/// readiness, the separately awaited rollback outcome. No implicit retry occurs.
#[derive(Debug, thiserror::Error)]
#[error("native schema operation failed: {original}")]
pub struct NativeSchemaFailure {
    #[source]
    original: NativeBackendError,
    cleanup: Option<Result<NativeControlCompletion, NativeBackendError>>,
}

impl NativeSchemaFailure {
    pub fn original(&self) -> &NativeBackendError {
        &self.original
    }

    pub fn cleanup(&self) -> Option<&Result<NativeControlCompletion, NativeBackendError>> {
        self.cleanup.as_ref()
    }
}

impl NativeBackend {
    /// Explicitly install the reserved schema in this physical database, or
    /// validate an exact existing installation. Conflicts are errors, never
    /// repairs. Only confirmed idle may start this owned transaction.
    pub async fn initialize_schema(
        &mut self,
    ) -> Result<NativeSchemaCompletion, NativeSchemaFailure> {
        self.schema_operation(true).await
    }

    /// Verify the functional installation in a read-only transaction. This
    /// never creates objects and cannot join an existing caller transaction.
    pub async fn verify_schema(&mut self) -> Result<NativeSchemaCompletion, NativeSchemaFailure> {
        self.schema_operation(false).await
    }

    async fn schema_operation(
        &mut self,
        initialize: bool,
    ) -> Result<NativeSchemaCompletion, NativeSchemaFailure> {
        let (operation, control, access) = if initialize {
            (
                NativeBackendOperation::InitializeSchema,
                NativeControl::InitializeSchema,
                "READ WRITE",
            )
        } else {
            (
                NativeBackendOperation::VerifySchema,
                NativeControl::VerifySchema,
                "READ ONLY",
            )
        };
        self.require(operation, &[TransactionState::Idle])
            .map_err(|original| NativeSchemaFailure {
                original,
                cleanup: None,
            })?;
        let body = if initialize {
            SCHEMA_SQL.to_owned()
        } else {
            // The only substitution is an internal constant, never caller SQL.
            assert_eq!(
                SCHEMA_SQL.matches(CREATE_MARKER).count(),
                1,
                "schema manifest must have exactly one creation switch"
            );
            SCHEMA_SQL.replacen(
                CREATE_MARKER,
                "allow_create constant pg_catalog.bool := false;",
                1,
            )
        };
        let sql = format!(
            "BEGIN ISOLATION LEVEL READ COMMITTED {access} NOT DEFERRABLE;\n{body}\nCOMMIT"
        );
        match self.control(control, &sql).await {
            Ok(completion) => Ok(NativeSchemaCompletion { completion }),
            Err(original) => {
                let cleanup = if self.state
                    == NativeBackendState::Ready(TransactionState::FailedTransaction)
                {
                    Some(self.rollback().await)
                } else {
                    None
                };
                Err(NativeSchemaFailure { original, cleanup })
            }
        }
    }
}
