/// Native isolation choices. These are PostgreSQL characteristics, not a
/// declaration of equivalent MySQL snapshot or locking behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeIsolation {
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeTransactionAccess {
    ReadWrite,
    ReadOnly,
}

/// Complete choices for starting an owned native transaction. There is no
/// implicit default. Every variant explicitly uses NOT DEFERRABLE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeTransactionSpec {
    pub isolation: NativeIsolation,
    pub access: NativeTransactionAccess,
}

impl NativeTransactionSpec {
    pub(crate) fn begin_sql(self) -> &'static str {
        match (self.isolation, self.access) {
            (NativeIsolation::ReadCommitted, NativeTransactionAccess::ReadWrite) => {
                "BEGIN ISOLATION LEVEL READ COMMITTED READ WRITE NOT DEFERRABLE"
            }
            (NativeIsolation::ReadCommitted, NativeTransactionAccess::ReadOnly) => {
                "BEGIN ISOLATION LEVEL READ COMMITTED READ ONLY NOT DEFERRABLE"
            }
            (NativeIsolation::RepeatableRead, NativeTransactionAccess::ReadWrite) => {
                "BEGIN ISOLATION LEVEL REPEATABLE READ READ WRITE NOT DEFERRABLE"
            }
            (NativeIsolation::RepeatableRead, NativeTransactionAccess::ReadOnly) => {
                "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY NOT DEFERRABLE"
            }
            (NativeIsolation::Serializable, NativeTransactionAccess::ReadWrite) => {
                "BEGIN ISOLATION LEVEL SERIALIZABLE READ WRITE NOT DEFERRABLE"
            }
            (NativeIsolation::Serializable, NativeTransactionAccess::ReadOnly) => {
                "BEGIN ISOLATION LEVEL SERIALIZABLE READ ONLY NOT DEFERRABLE"
            }
        }
    }
}
