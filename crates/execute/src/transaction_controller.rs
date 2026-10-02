//! Ordered native controls for original-source frontend transactions. A plan
//! admits every control before effects, including a replacement transaction.

use crate::{
    NativeBackend, NativeIsolation, NativeTransactionAccess, NativeTransactionSpec,
    QueryExecutionError,
};
use darmok_session::{
    FrontendCompletionType, FrontendIsolation, FrontendTransactionAccess,
    FrontendTransactionBoundary, FrontendTransactionCommand, SessionCommandStage, SessionState,
    TransactionCharacteristics, TransactionCompletion, TransactionSettingsError,
    TransactionSettingsSnapshot,
};
use sqlparser::ast::{
    BeginTransactionKind, Statement, TransactionAccessMode, TransactionCompletionOptions,
    TransactionMode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionSqlError {
    Unsupported,
    UnsupportedIsolation,
    ConsistentSnapshot,
    Release,
}

impl TransactionSqlError {
    pub fn code(self) -> u16 {
        1235
    }
    pub fn sql_state(self) -> [u8; 5] {
        *b"42000"
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::Unsupported => "This transaction statement is not implemented",
            Self::UnsupportedIsolation => "READ UNCOMMITTED transactions are not implemented",
            Self::ConsistentSnapshot => "WITH CONSISTENT SNAPSHOT is not implemented",
            Self::Release => "Transaction RELEASE is not implemented",
        }
    }
}

pub(crate) enum TransactionAdmissionError {
    Sql(TransactionSqlError),
    Settings(TransactionSettingsError),
}
impl From<TransactionSqlError> for TransactionAdmissionError {
    fn from(value: TransactionSqlError) -> Self {
        Self::Sql(value)
    }
}
impl From<TransactionSettingsError> for TransactionAdmissionError {
    fn from(value: TransactionSettingsError) -> Self {
        Self::Settings(value)
    }
}

#[derive(Clone, Copy)]
enum Control {
    Begin(NativeTransactionSpec),
    Commit,
    Rollback,
    /// The exclusive owner is already idle; this frontend end still resets
    /// one-shot choices. It must not be inferred from an arbitrary native idle.
    IdleCompletion,
}
#[derive(Clone, Copy)]
struct Step {
    boundary: FrontendTransactionBoundary,
    control: Control,
}

pub(crate) struct TransactionPlan {
    command: FrontendTransactionCommand,
    steps: [Option<Step>; 2],
}

pub(crate) fn admit_transaction(
    state: &SessionState,
    input: &Statement,
) -> Result<TransactionPlan, TransactionAdmissionError> {
    let before = state.transaction_settings()?;
    let command = match input {
        Statement::StartTransaction {
            modes,
            begin,
            transaction,
            modifier,
            statements,
            exception,
            has_end_keyword,
        } => {
            if modifier.is_some()
                || !statements.is_empty()
                || exception.is_some()
                || *has_end_keyword
                || (*begin
                    && (!modes.is_empty()
                        || !matches!(transaction, None | Some(BeginTransactionKind::Work))))
                || (!*begin && !matches!(transaction, Some(BeginTransactionKind::Transaction)))
            {
                return Err(TransactionSqlError::Unsupported.into());
            }
            let mut access = None;
            for mode in modes {
                let next = match mode {
                    TransactionMode::AccessMode(TransactionAccessMode::ReadOnly) => {
                        FrontendTransactionAccess::ReadOnly
                    }
                    TransactionMode::AccessMode(TransactionAccessMode::ReadWrite) => {
                        FrontendTransactionAccess::ReadWrite
                    }
                    TransactionMode::ConsistentSnapshot => {
                        return Err(TransactionSqlError::ConsistentSnapshot.into());
                    }
                    TransactionMode::IsolationLevel(_) => {
                        return Err(TransactionSqlError::Unsupported.into());
                    }
                };
                if access.is_some_and(|previous| previous != next) {
                    return Err(TransactionSqlError::Unsupported.into());
                }
                access = Some(next);
            }
            FrontendTransactionCommand::BeginExplicit { access }
        }
        Statement::Commit {
            options,
            end: false,
            modifier: None,
        } => completion_command(before, options, TransactionCompletion::Commit)?,
        Statement::Rollback {
            options,
            savepoint: None,
        } => completion_command(before, options, TransactionCompletion::Rollback)?,
        _ => return Err(TransactionSqlError::Unsupported.into()),
    };
    let boundaries = before.plan_command(command)?;
    let mut steps = [None; 2];
    let mut active = before.active.is_some();
    for (slot, boundary) in steps.iter_mut().zip(boundaries) {
        let Some(boundary) = boundary else {
            continue;
        };
        let control = match boundary {
            FrontendTransactionBoundary::Started(pair) => {
                let spec = native_spec(pair)?;
                active = true;
                Control::Begin(spec)
            }
            FrontendTransactionBoundary::Ended(completion) => {
                let control = if active {
                    match completion {
                        TransactionCompletion::Commit => Control::Commit,
                        TransactionCompletion::Rollback => Control::Rollback,
                    }
                } else {
                    Control::IdleCompletion
                };
                active = false;
                control
            }
            _ => return Err(TransactionSqlError::Unsupported.into()),
        };
        *slot = Some(Step { boundary, control });
    }
    Ok(TransactionPlan { command, steps })
}

fn completion_command(
    before: TransactionSettingsSnapshot,
    options: &TransactionCompletionOptions,
    completion: TransactionCompletion,
) -> Result<FrontendTransactionCommand, TransactionSqlError> {
    if !matches!(options.transaction, None | Some(BeginTransactionKind::Work)) {
        return Err(TransactionSqlError::Unsupported);
    }
    let chain = options
        .chain
        .unwrap_or(before.completion_type == FrontendCompletionType::Chain);
    let release = options
        .release
        .unwrap_or(before.completion_type == FrontendCompletionType::Release);
    // These explicit clauses are mutually exclusive in the MySQL grammar.
    if options.chain == Some(true) && options.release == Some(true) {
        return Err(TransactionSqlError::Unsupported);
    }
    if release {
        return Err(TransactionSqlError::Release);
    }
    Ok(FrontendTransactionCommand::Complete { completion, chain })
}

fn native_spec(
    pair: TransactionCharacteristics,
) -> Result<NativeTransactionSpec, TransactionSqlError> {
    Ok(NativeTransactionSpec {
        isolation: match pair.isolation {
            FrontendIsolation::ReadUncommitted => {
                return Err(TransactionSqlError::UnsupportedIsolation);
            }
            FrontendIsolation::ReadCommitted => NativeIsolation::ReadCommitted,
            FrontendIsolation::RepeatableRead => NativeIsolation::RepeatableRead,
            FrontendIsolation::Serializable => NativeIsolation::Serializable,
        },
        access: match pair.access {
            FrontendTransactionAccess::ReadWrite => NativeTransactionAccess::ReadWrite,
            FrontendTransactionAccess::ReadOnly => NativeTransactionAccess::ReadOnly,
        },
    })
}

pub(crate) async fn apply_transaction(
    stage: &mut SessionCommandStage<'_>,
    backend: &mut NativeBackend,
    plan: TransactionPlan,
) -> Result<(), QueryExecutionError> {
    let mut transaction = stage.stage_transaction(plan.command)?;
    transaction.mark_submitted()?;
    for step in plan.steps.into_iter().flatten() {
        let expected = transaction.next_frontend_boundary();
        if expected != Some(step.boundary) {
            return Err(TransactionSettingsError::UnexpectedBoundary {
                expected,
                received: step.boundary,
            }
            .into());
        }
        match step.control {
            Control::Begin(spec) => {
                let _receipt = backend.begin(spec).await?;
            }
            Control::Commit => {
                let _receipt = backend.commit().await?;
            }
            Control::Rollback => {
                let _receipt = backend.rollback().await?;
            }
            Control::IdleCompletion => {}
        }
        transaction.record_confirmed_frontend_boundary(step.boundary)?;
    }
    transaction.finish_confirmed_effects()?;
    Ok(())
}
