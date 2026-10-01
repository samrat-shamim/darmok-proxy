//! One outcome boundary for an entire SET, including known partial effects.
//! Methods recording effects assert the execution controller's receipt contract.
//! They do not submit native SQL or send a response.

use crate::{
    AutocommitSetting, FrontendTransactionCommand, SessionState, SqlModes, TransactionCompletion,
    TransactionSettingAssignment, TransactionSettingsError, TransactionSettingsSnapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetSettingsSnapshot {
    pub sql_modes: SqlModes,
    pub transactions: TransactionSettingsSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnconfirmedSetCommand {
    pub before: SetSettingsSnapshot,
    pub last_confirmed: SetSettingsSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingSetCommand {
    pub before: SetSettingsSnapshot,
}

/// Exclusively holds session state until the complete SET response is sent.
/// Dropping the stage retains known effects and prevents session reuse.
#[derive(Debug)]
pub struct SessionSetStage<'a> {
    pub(crate) state: &'a mut SessionState,
}

impl SessionSetStage<'_> {
    pub fn settings(&self) -> Result<SetSettingsSnapshot, TransactionSettingsError> {
        Ok(SetSettingsSnapshot {
            sql_modes: self.state.sql_modes,
            transactions: self.state.transactions.snapshot()?,
        })
    }

    pub fn apply_sql_modes(&mut self, modes: SqlModes) {
        self.state.sql_modes = modes;
    }

    pub fn apply_transaction_assignment(
        &mut self,
        assignment: TransactionSettingAssignment,
    ) -> Result<(), TransactionSettingsError> {
        self.apply_local_command(FrontendTransactionCommand::Assign(assignment))
    }

    /// Record only after the native owner confirms COMMIT, including its tag
    /// and idle ReadyForQuery. This cannot be inferred from native idle alone.
    pub fn record_autocommit_commit(&mut self) -> Result<(), TransactionSettingsError> {
        let state = self.state.transactions.snapshot()?;
        if state.active.is_none() {
            return Err(TransactionSettingsError::NoActiveTransaction);
        }
        if state.autocommit != AutocommitSetting::Disabled {
            return Err(TransactionSettingsError::UnverifiedAutocommitBoundary);
        }
        self.apply_local_command(FrontendTransactionCommand::Complete {
            completion: TransactionCompletion::Commit,
            chain: false,
        })
    }

    pub fn apply_autocommit(
        &mut self,
        setting: AutocommitSetting,
    ) -> Result<(), TransactionSettingsError> {
        // The commit must already have its separate native receipt.
        let current = self.state.transactions.snapshot()?;
        if current.active.is_some() && setting == AutocommitSetting::Enabled {
            return Err(TransactionSettingsError::UnverifiedAutocommitBoundary);
        }
        self.apply_local_command(FrontendTransactionCommand::SetAutocommit(setting))
    }

    fn apply_local_command(
        &mut self,
        command: FrontendTransactionCommand,
    ) -> Result<(), TransactionSettingsError> {
        let mut stage = self.state.transactions.stage(command)?;
        stage.mark_submitted()?;
        let boundary = stage
            .next_frontend_boundary()
            .expect("setting has a boundary");
        stage.record_confirmed_frontend_boundary(boundary)?;
        // The encompassing SET guard still prevents all public session reads.
        // No intermediate assignment is certified as a completed SQL command.
        stage.finish_known_effects()?;
        Ok(())
    }

    pub fn clear_diagnostics(&mut self) {
        self.state.clear_warning_stack();
    }

    pub fn record_sql_error(&mut self, code: u16, message: &str) {
        self.state
            .push_warning(crate::WarningLevel::Error, code, message);
        self.state.affected_rows = 0;
        self.state.row_count = -1;
    }

    pub fn record_sql_success(&mut self) {
        self.state.affected_rows = 0;
        self.state.row_count = 0;
    }

    /// Call after the controller has encoded and sent either the success
    /// response or the known SQL error response for this entire statement.
    pub fn finish_with_sent_output(self) -> Result<SetSettingsSnapshot, TransactionSettingsError> {
        let snapshot = self.settings()?;
        self.state.pending_set = None;
        Ok(snapshot)
    }
}

impl SessionState {
    pub fn stage_set_command(&mut self) -> Result<SessionSetStage<'_>, TransactionSettingsError> {
        self.ensure_settled()?;
        let before = SetSettingsSnapshot {
            sql_modes: self.sql_modes,
            transactions: self.transactions.snapshot()?,
        };
        self.pending_set = Some(PendingSetCommand { before });
        Ok(SessionSetStage { state: self })
    }

    pub fn unconfirmed_set_command(&self) -> Option<UnconfirmedSetCommand> {
        self.pending_set.map(|stage| UnconfirmedSetCommand {
            before: stage.before,
            last_confirmed: SetSettingsSnapshot {
                sql_modes: self.sql_modes,
                transactions: self.transactions.last_confirmed(),
            },
        })
    }
}
