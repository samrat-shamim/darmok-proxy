//! One command outcome boundary for setting effects and SQL diagnostics.
//! Methods recording effects assert the execution controller's receipt contract.
//! They do not submit native SQL or send a response.

use crate::{
    AutocommitSetting, FrontendCompletionType, FrontendTransactionBoundary,
    FrontendTransactionCommand, SessionState, SqlModes, TransactionCommandStage,
    TransactionCompletion, TransactionSettingAssignment, TransactionSettingsError,
    TransactionSettingsSnapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSettingsSnapshot {
    pub sql_modes: SqlModes,
    pub transactions: TransactionSettingsSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnconfirmedCommand {
    pub before: CommandSettingsSnapshot,
    pub last_confirmed: CommandSettingsSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingCommand {
    pub before: CommandSettingsSnapshot,
}

/// Exclusively holds command settings until its complete response is sent.
/// Dropping the stage retains known effects and prevents session reuse.
#[derive(Debug)]
pub struct SessionCommandStage<'a> {
    pub(crate) state: &'a mut SessionState,
}

impl SessionCommandStage<'_> {
    pub fn settings(&self) -> Result<CommandSettingsSnapshot, TransactionSettingsError> {
        Ok(CommandSettingsSnapshot {
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

    pub fn apply_completion_type(
        &mut self,
        setting: FrontendCompletionType,
    ) -> Result<(), TransactionSettingsError> {
        self.apply_local_command(FrontendTransactionCommand::SetCompletionType(setting))
    }

    /// Holds ordered model boundaries inside this command's output guard.
    /// Settling effects through this borrow cannot settle the SQL response.
    pub fn stage_transaction(
        &mut self,
        command: FrontendTransactionCommand,
    ) -> Result<SessionTransactionStage<'_>, TransactionSettingsError> {
        Ok(SessionTransactionStage {
            stage: self.state.transactions.stage(command)?,
        })
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
        // The encompassing command guard still prevents all public session reads.
        // No intermediate assignment is certified as a completed SQL command.
        stage.finish_known_effects()?;
        Ok(())
    }

    pub fn clear_diagnostics(&mut self) {
        self.state.clear_diagnostics();
    }

    pub fn statement_condition_count_u16(&self) -> u16 {
        self.state.statement_condition_count_u16()
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

    /// Record a completed ordinary SELECT. MySQL ROW_COUNT() is -1 for a
    /// result set; FOUND_ROWS() is its row count without SQL_CALC_FOUND_ROWS.
    pub fn record_select_success(&mut self, rows: u64) {
        self.state.affected_rows = 0;
        self.state.row_count = -1;
        self.state.found_rows = rows;
    }

    /// Call after the controller has encoded and sent either the success
    /// response or the known SQL error response for this entire command.
    pub fn finish_with_sent_output(
        self,
    ) -> Result<CommandSettingsSnapshot, TransactionSettingsError> {
        let snapshot = self.settings()?;
        self.state.pending_command = None;
        Ok(snapshot)
    }
}

/// A nested transaction stage can confirm native controls while the enclosing
/// command continues to prohibit public session reads and reuse through output.
#[derive(Debug)]
pub struct SessionTransactionStage<'a> {
    stage: TransactionCommandStage<'a>,
}

impl SessionTransactionStage<'_> {
    pub fn next_frontend_boundary(&self) -> Option<FrontendTransactionBoundary> {
        self.stage.next_frontend_boundary()
    }
    pub fn mark_submitted(&mut self) -> Result<(), TransactionSettingsError> {
        self.stage.mark_submitted()
    }
    pub fn record_confirmed_frontend_boundary(
        &mut self,
        boundary: FrontendTransactionBoundary,
    ) -> Result<(), TransactionSettingsError> {
        self.stage.record_confirmed_frontend_boundary(boundary)
    }
    /// Requires every staged boundary's semantic receipt. The encompassing
    /// guard remains pending until it confirms the whole response was sent.
    pub fn finish_confirmed_effects(
        self,
    ) -> Result<TransactionSettingsSnapshot, TransactionSettingsError> {
        self.stage.finish_known_effects()
    }
}

impl SessionState {
    pub fn stage_command(&mut self) -> Result<SessionCommandStage<'_>, TransactionSettingsError> {
        self.ensure_settled()?;
        let before = CommandSettingsSnapshot {
            sql_modes: self.sql_modes,
            transactions: self.transactions.snapshot()?,
        };
        self.pending_command = Some(PendingCommand { before });
        Ok(SessionCommandStage { state: self })
    }

    pub fn unconfirmed_command(&self) -> Option<UnconfirmedCommand> {
        self.pending_command.map(|stage| UnconfirmedCommand {
            before: stage.before,
            last_confirmed: CommandSettingsSnapshot {
                sql_modes: self.sql_modes,
                transactions: self.transactions.last_confirmed(),
            },
        })
    }
}
