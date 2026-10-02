//! Typed frontend setting state. This component does not classify SQL, execute
//! native controls, or certify frontend boundaries. Its caller must establish
//! those boundaries through the admitted controller and output validation.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontendIsolation {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

impl FrontendIsolation {
    pub fn variable_label(self) -> &'static str {
        match self {
            Self::ReadUncommitted => "READ-UNCOMMITTED",
            Self::ReadCommitted => "READ-COMMITTED",
            Self::RepeatableRead => "REPEATABLE-READ",
            Self::Serializable => "SERIALIZABLE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontendTransactionAccess {
    ReadWrite,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionCharacteristics {
    pub isolation: FrontendIsolation,
    pub access: FrontendTransactionAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedTransactionCharacteristic {
    Isolation(FrontendIsolation),
    Access(FrontendTransactionAccess),
}

/// A nonempty change; compound variable assignments are not represented by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransactionCharacteristicUpdate {
    One(NamedTransactionCharacteristic),
    Both(TransactionCharacteristics),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransactionVariableAssignmentForm {
    SessionKeyword,
    QualifiedSession,
    Bare,
    UnqualifiedAt,
}

/// Original form is retained through staging. No prefix normalizer supplies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransactionSettingAssignment {
    SessionTransaction(TransactionCharacteristicUpdate),
    NextTransaction(TransactionCharacteristicUpdate),
    Variable {
        form: TransactionVariableAssignmentForm,
        characteristic: NamedTransactionCharacteristic,
    },
}

impl TransactionSettingAssignment {
    fn target(self) -> (bool, TransactionCharacteristicUpdate) {
        match self {
            Self::SessionTransaction(update) => (false, update),
            Self::NextTransaction(update) => (true, update),
            Self::Variable {
                form,
                characteristic,
            } => (
                form == TransactionVariableAssignmentForm::UnqualifiedAt,
                TransactionCharacteristicUpdate::One(characteristic),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutocommitSetting {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransactionCompletion {
    Commit,
    Rollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontendTransactionCommand {
    Assign(TransactionSettingAssignment),
    BeginExplicit {
        access: Option<FrontendTransactionAccess>,
    },
    BeginImplicit,
    AutocommitStatement,
    Complete {
        completion: TransactionCompletion,
        chain: bool,
    },
    SetAutocommit(AutocommitSetting),
}

/// Semantic frontend actions, never a PostgreSQL readiness state. A native
/// internal BEGIN is not one of these actions and cannot consume next choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontendTransactionBoundary {
    SettingsAssigned(TransactionSettingAssignment),
    Started(TransactionCharacteristics),
    Ended(TransactionCompletion),
    AutocommitAssigned(AutocommitSetting),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NextTransactionCharacteristics {
    pub isolation: Option<FrontendIsolation>,
    pub access: Option<FrontendTransactionAccess>,
}

/// Immutable copy of the authoritative confirmed state, not a mutable store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionSettingsSnapshot {
    pub defaults: TransactionCharacteristics,
    pub next: NextTransactionCharacteristics,
    pub active: Option<TransactionCharacteristics>,
    pub autocommit: AutocommitSetting,
}

/// A contradictory command-phase report is retained, even if later calls
/// supply the originally expected boundary. It cannot settle as success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionCommandPhaseError {
    RepeatedSubmission,
    BoundaryBeforeSubmission(FrontendTransactionBoundary),
    SuccessBeforeSubmission,
    UnexpectedBoundary {
        expected: Option<FrontendTransactionBoundary>,
        received: FrontendTransactionBoundary,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnconfirmedTransactionCommand {
    pub command: FrontendTransactionCommand,
    pub before: TransactionSettingsSnapshot,
    pub last_confirmed: TransactionSettingsSnapshot,
    pub next_boundary: Option<FrontendTransactionBoundary>,
    pub confirmed_boundaries: u8,
    pub phase_error: Option<TransactionCommandPhaseError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TransactionSettingsError {
    #[error("transaction command outcome is not settled")]
    UnsettledOutcome,
    #[error("next-transaction choices cannot change inside an active transaction")]
    NextChoicesDuringActive,
    #[error("this command requires an active frontend transaction")]
    NoActiveTransaction,
    #[error("this statement start requires an idle frontend transaction")]
    AlreadyActive,
    #[error("implicit start requires disabled autocommit")]
    ImplicitStartWithAutocommit,
    #[error("autocommit statement requires enabled autocommit")]
    AutocommitStatementWithAutocommitDisabled,
    #[error("this active autocommit-setting boundary is not verified")]
    UnverifiedAutocommitBoundary,
    #[error("transaction command has already been submitted")]
    AlreadySubmitted,
    #[error("transaction command has not been submitted")]
    NotSubmitted,
    #[error("confirmed frontend boundary differs from the staged action")]
    UnexpectedBoundary {
        expected: Option<FrontendTransactionBoundary>,
        received: FrontendTransactionBoundary,
    },
    #[error("frontend boundaries are incomplete")]
    IncompleteBoundaries,
}

#[derive(Debug, PartialEq, Eq)]
struct StagedCommand {
    command: FrontendTransactionCommand,
    before: TransactionSettingsSnapshot,
    boundaries: [Option<FrontendTransactionBoundary>; 2],
    next: u8,
    submitted: bool,
    phase_error: Option<TransactionCommandPhaseError>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TransactionSettings {
    confirmed: TransactionSettingsSnapshot,
    staged: Option<StagedCommand>,
}

impl TransactionSettings {
    pub(crate) fn new(defaults: TransactionCharacteristics) -> Self {
        Self {
            confirmed: TransactionSettingsSnapshot {
                defaults,
                next: NextTransactionCharacteristics::default(),
                active: None,
                autocommit: AutocommitSetting::Enabled,
            },
            staged: None,
        }
    }

    pub(crate) fn snapshot(&self) -> Result<TransactionSettingsSnapshot, TransactionSettingsError> {
        if self.staged.is_some() {
            return Err(TransactionSettingsError::UnsettledOutcome);
        }
        Ok(self.confirmed)
    }

    pub(crate) fn last_confirmed(&self) -> TransactionSettingsSnapshot {
        self.confirmed
    }

    pub(crate) fn unconfirmed(&self) -> Option<UnconfirmedTransactionCommand> {
        self.staged
            .as_ref()
            .map(|stage| UnconfirmedTransactionCommand {
                command: stage.command,
                before: stage.before,
                last_confirmed: self.confirmed,
                next_boundary: stage
                    .boundaries
                    .get(usize::from(stage.next))
                    .copied()
                    .flatten(),
                confirmed_boundaries: stage.next,
                phase_error: stage.phase_error,
            })
    }

    pub(crate) fn stage(
        &mut self,
        command: FrontendTransactionCommand,
    ) -> Result<TransactionCommandStage<'_>, TransactionSettingsError> {
        let current = self.snapshot()?;
        use FrontendTransactionBoundary as Boundary;
        let start = |access: Option<FrontendTransactionAccess>| {
            Boundary::Started(TransactionCharacteristics {
                isolation: current.next.isolation.unwrap_or(current.defaults.isolation),
                access: access
                    .or(current.next.access)
                    .unwrap_or(current.defaults.access),
            })
        };
        let boundaries = match command {
            FrontendTransactionCommand::Assign(assignment) => {
                if assignment.target().0 && current.active.is_some() {
                    return Err(TransactionSettingsError::NextChoicesDuringActive);
                }
                [Some(Boundary::SettingsAssigned(assignment)), None]
            }
            FrontendTransactionCommand::BeginExplicit { access } => {
                if current.active.is_some() {
                    [
                        Some(Boundary::Ended(TransactionCompletion::Commit)),
                        Some(start(access)),
                    ]
                } else {
                    [Some(start(access)), None]
                }
            }
            FrontendTransactionCommand::BeginImplicit => {
                if current.active.is_some() {
                    return Err(TransactionSettingsError::AlreadyActive);
                }
                if current.autocommit != AutocommitSetting::Disabled {
                    return Err(TransactionSettingsError::ImplicitStartWithAutocommit);
                }
                [Some(start(None)), None]
            }
            FrontendTransactionCommand::AutocommitStatement => {
                if current.active.is_some() {
                    return Err(TransactionSettingsError::AlreadyActive);
                }
                if current.autocommit != AutocommitSetting::Enabled {
                    return Err(
                        TransactionSettingsError::AutocommitStatementWithAutocommitDisabled,
                    );
                }
                [
                    Some(start(None)),
                    Some(Boundary::Ended(TransactionCompletion::Commit)),
                ]
            }
            FrontendTransactionCommand::Complete { completion, chain } => {
                let active = current
                    .active
                    .ok_or(TransactionSettingsError::NoActiveTransaction)?;
                [
                    Some(Boundary::Ended(completion)),
                    chain.then_some(Boundary::Started(active)),
                ]
            }
            FrontendTransactionCommand::SetAutocommit(setting) => {
                if current.active.is_some() && current.autocommit == AutocommitSetting::Enabled {
                    return Err(TransactionSettingsError::UnverifiedAutocommitBoundary);
                }
                if current.active.is_some() && setting == AutocommitSetting::Enabled {
                    [
                        Some(Boundary::Ended(TransactionCompletion::Commit)),
                        Some(Boundary::AutocommitAssigned(setting)),
                    ]
                } else {
                    [Some(Boundary::AutocommitAssigned(setting)), None]
                }
            }
        };
        self.staged = Some(StagedCommand {
            command,
            before: current,
            boundaries,
            next: 0,
            submitted: false,
            phase_error: None,
        });
        Ok(TransactionCommandStage { settings: self })
    }

    fn apply_boundary(&mut self, boundary: FrontendTransactionBoundary) {
        match boundary {
            FrontendTransactionBoundary::SettingsAssigned(assignment) => {
                let (next_only, update) = assignment.target();
                let (isolation, access) = match update {
                    TransactionCharacteristicUpdate::One(
                        NamedTransactionCharacteristic::Isolation(value),
                    ) => (Some(value), None),
                    TransactionCharacteristicUpdate::One(
                        NamedTransactionCharacteristic::Access(value),
                    ) => (None, Some(value)),
                    TransactionCharacteristicUpdate::Both(value) => {
                        (Some(value.isolation), Some(value.access))
                    }
                };
                if let Some(isolation) = isolation {
                    if next_only {
                        self.confirmed.next.isolation = Some(isolation);
                    } else {
                        self.confirmed.defaults.isolation = isolation;
                        self.confirmed.next.isolation = None;
                    }
                }
                if let Some(access) = access {
                    if next_only {
                        self.confirmed.next.access = Some(access);
                    } else {
                        self.confirmed.defaults.access = access;
                        self.confirmed.next.access = None;
                    }
                }
            }
            FrontendTransactionBoundary::Started(choices) => {
                self.confirmed.active = Some(choices);
                self.confirmed.next = NextTransactionCharacteristics::default();
            }
            FrontendTransactionBoundary::Ended(_) => self.confirmed.active = None,
            FrontendTransactionBoundary::AutocommitAssigned(setting) => {
                self.confirmed.autocommit = setting
            }
        }
    }
}

/// Borrowed setting command. Preparing can be abandoned without changes.
/// After submission, dropping it retains an unconfirmed outcome and prohibits
/// further settings reads, fingerprints or commands. Boundary methods assert
/// the caller's semantic receipt; they do not verify a native operation.
/// Contradictory boundary or success reports are retained before submission too.
#[derive(Debug)]
pub struct TransactionCommandStage<'a> {
    settings: &'a mut TransactionSettings,
}

impl TransactionCommandStage<'_> {
    pub fn next_frontend_boundary(&self) -> Option<FrontendTransactionBoundary> {
        let stage = self
            .settings
            .staged
            .as_ref()
            .expect("stage is owned by this borrow");
        stage
            .boundaries
            .get(usize::from(stage.next))
            .copied()
            .flatten()
    }

    /// Call once before any operation can produce the command's effects.
    /// A repeated report makes the command outcome unconfirmed; this method
    /// does not submit any native operation itself.
    pub fn mark_submitted(&mut self) -> Result<(), TransactionSettingsError> {
        let stage = self
            .settings
            .staged
            .as_mut()
            .expect("stage is owned by this borrow");
        if stage.phase_error.is_some() {
            return Err(TransactionSettingsError::UnsettledOutcome);
        }
        if stage.submitted {
            stage.phase_error = Some(TransactionCommandPhaseError::RepeatedSubmission);
            return Err(TransactionSettingsError::AlreadySubmitted);
        }
        stage.submitted = true;
        Ok(())
    }

    pub fn record_confirmed_frontend_boundary(
        &mut self,
        boundary: FrontendTransactionBoundary,
    ) -> Result<(), TransactionSettingsError> {
        let stage = self
            .settings
            .staged
            .as_mut()
            .expect("stage is owned by this borrow");
        if stage.phase_error.is_some() {
            return Err(TransactionSettingsError::UnsettledOutcome);
        }
        if !stage.submitted {
            stage.phase_error = Some(TransactionCommandPhaseError::BoundaryBeforeSubmission(
                boundary,
            ));
            return Err(TransactionSettingsError::NotSubmitted);
        }
        let expected = self.next_frontend_boundary();
        if expected != Some(boundary) {
            self.settings
                .staged
                .as_mut()
                .expect("stage is owned by this borrow")
                .phase_error = Some(TransactionCommandPhaseError::UnexpectedBoundary {
                expected,
                received: boundary,
            });
            return Err(TransactionSettingsError::UnexpectedBoundary {
                expected,
                received: boundary,
            });
        }
        self.settings.apply_boundary(boundary);
        self.settings
            .staged
            .as_mut()
            .expect("stage is owned by this borrow")
            .next += 1;
        Ok(())
    }

    /// Call only for a successful command whose frontend output is validated.
    /// This is a caller contract, not an encoder or a wire acknowledgement.
    /// Statement/error recovery outcomes need their own verified controller;
    /// an error packet is not a successful completion of this setting command.
    pub fn finish_success_with_validated_output(
        self,
    ) -> Result<TransactionSettingsSnapshot, TransactionSettingsError> {
        self.finish_known_effects()
    }

    /// Only a session command guard may settle intermediate model effects;
    /// that guard keeps public reads unavailable until the statement output.
    pub(crate) fn finish_known_effects(
        self,
    ) -> Result<TransactionSettingsSnapshot, TransactionSettingsError> {
        let stage = self
            .settings
            .staged
            .as_mut()
            .expect("stage is owned by this borrow");
        if stage.phase_error.is_some() {
            return Err(TransactionSettingsError::UnsettledOutcome);
        }
        if !stage.submitted {
            stage.phase_error = Some(TransactionCommandPhaseError::SuccessBeforeSubmission);
            return Err(TransactionSettingsError::NotSubmitted);
        }
        if self.next_frontend_boundary().is_some() {
            return Err(TransactionSettingsError::IncompleteBoundaries);
        }
        self.settings.staged = None;
        Ok(self.settings.confirmed)
    }
}

impl Drop for TransactionCommandStage<'_> {
    fn drop(&mut self) {
        if self
            .settings
            .staged
            .as_ref()
            .is_some_and(|stage| !stage.submitted && stage.phase_error.is_none())
        {
            self.settings.staged = None;
        }
    }
}
