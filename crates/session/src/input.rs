//! Borrowed SQL input facts and typed transaction-setting intents. These APIs
//! do not admit SQL, coerce expressions, apply compound assignments, execute
//! native operations, or validate frontend output.

use sqlparser::ast::{
    ContextModifier, Expr, Ident, MySqlSystemVariable, MySqlSystemVariableScope, Set,
    SetAssignment, SetAssignmentTarget, SetTransaction, TransactionAccessMode,
    TransactionIsolationLevel, TransactionMode,
};
use thiserror::Error;

use crate::{
    FrontendIsolation, FrontendTransactionAccess, NamedTransactionCharacteristic, SessionVariable,
    TransactionCharacteristicUpdate, TransactionCharacteristics, TransactionSettingAssignment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SessionInputError {
    #[error("this transaction-setting syntax is outside the MySQL input contract")]
    UnsupportedTransactionSyntax,
    #[error("transaction-setting scope {0:?} is not implemented")]
    UnsupportedTransactionScope(ContextModifier),
    #[error("transaction setting has no characteristics")]
    EmptyTransactionCharacteristics,
    #[error("transaction isolation was specified more than once")]
    RepeatedIsolation,
    #[error("transaction access was specified more than once")]
    RepeatedAccess,
    #[error("transaction isolation {0:?} is outside the MySQL input contract")]
    UnsupportedIsolation(TransactionIsolationLevel),
    #[error("this SET shape is outside the system-variable input contract")]
    UnsupportedVariableSet,
    #[error("variable assignment requires one expression")]
    AssignmentExpressionCount,
    #[error("variable assignment list is empty")]
    EmptyAssignments,
    #[error("this name shape is outside the system-variable input contract")]
    UnsupportedVariableName,
    #[error("system-variable scope {0:?} is outside the session input contract")]
    UnsupportedVariableScope(MySqlSystemVariableScope),
    #[error("system variable is not in the canonical registry")]
    UnknownVariable,
    #[error("expression is not a system-variable reference")]
    NotSystemVariableRead,
}

/// Classify the directly observed SESSION and next-transaction forms. LOCAL
/// is retained by the parser but its controller integration remains pending;
/// GLOBAL is never converted into a session update. A returned intent does not
/// establish a native isolation mapping or a completed setting boundary.
pub fn classify_mysql_transaction_setting(
    input: &SetTransaction,
) -> Result<TransactionSettingAssignment, SessionInputError> {
    let SetTransaction::Direct { scope, modes } = input else {
        return Err(SessionInputError::UnsupportedTransactionSyntax);
    };
    if let Some(scope @ (ContextModifier::Local | ContextModifier::Global)) = scope {
        return Err(SessionInputError::UnsupportedTransactionScope(*scope));
    }
    let mut isolation = None;
    let mut access = None;
    for mode in modes {
        match mode {
            TransactionMode::IsolationLevel(level) => {
                if isolation.is_some() {
                    return Err(SessionInputError::RepeatedIsolation);
                }
                isolation = Some(match level {
                    TransactionIsolationLevel::ReadUncommitted => {
                        FrontendIsolation::ReadUncommitted
                    }
                    TransactionIsolationLevel::ReadCommitted => FrontendIsolation::ReadCommitted,
                    TransactionIsolationLevel::RepeatableRead => FrontendIsolation::RepeatableRead,
                    TransactionIsolationLevel::Serializable => FrontendIsolation::Serializable,
                    TransactionIsolationLevel::Snapshot => {
                        return Err(SessionInputError::UnsupportedIsolation(*level));
                    }
                });
            }
            TransactionMode::AccessMode(mode) => {
                if access.is_some() {
                    return Err(SessionInputError::RepeatedAccess);
                }
                access = Some(match mode {
                    TransactionAccessMode::ReadOnly => FrontendTransactionAccess::ReadOnly,
                    TransactionAccessMode::ReadWrite => FrontendTransactionAccess::ReadWrite,
                });
            }
        }
    }
    let update = match (isolation, access) {
        (Some(isolation), Some(access)) => {
            TransactionCharacteristicUpdate::Both(TransactionCharacteristics { isolation, access })
        }
        (Some(isolation), None) => TransactionCharacteristicUpdate::One(
            NamedTransactionCharacteristic::Isolation(isolation),
        ),
        (None, Some(access)) => {
            TransactionCharacteristicUpdate::One(NamedTransactionCharacteristic::Access(access))
        }
        (None, None) => return Err(SessionInputError::EmptyTransactionCharacteristics),
    };
    Ok(match scope {
        None => TransactionSettingAssignment::NextTransaction(update),
        Some(ContextModifier::Session) => TransactionSettingAssignment::SessionTransaction(update),
        Some(scope) => return Err(SessionInputError::UnsupportedTransactionScope(*scope)),
    })
}

/// Syntax facts, not an effective read/write target. In particular, unqualified
/// @@ transaction assignments differ from bare assignments, and read lookup is
/// not the same operation as assignment lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemVariableForm {
    Bare,
    UnqualifiedAt,
    Qualified(ContextModifier),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemVariableAssignment<'a> {
    pub variable: SessionVariable,
    pub form: SystemVariableForm,
    pub name: &'a SetAssignmentTarget,
    pub value: &'a Expr,
    /// Keyword attached to this assignment, if present.
    pub explicit_keyword_scope: Option<ContextModifier>,
    /// Most recent keyword before this assignment, excluding the current one.
    pub preceding_keyword_scope: Option<ContextModifier>,
}

impl SystemVariableAssignment<'_> {
    /// Retains the lexical keyword context independently of a per-name @@
    /// qualifier. This does not resolve their combined semantic target.
    pub fn keyword_context(self) -> Option<ContextModifier> {
        self.explicit_keyword_scope.or(self.preceding_keyword_scope)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemVariableRead<'a> {
    pub variable: SessionVariable,
    pub form: SystemVariableForm,
    pub expression: &'a Expr,
}

#[derive(Debug)]
enum AssignmentSource<'a> {
    Single(Option<(Option<ContextModifier>, &'a SetAssignmentTarget, &'a Expr)>),
    Multiple(std::slice::Iter<'a, SetAssignment>),
}

/// A single pass over borrowed source expressions. Classification stops after
/// its first explicit error. Earlier items are input facts, never permission to
/// apply a partially classified compound statement.
#[derive(Debug)]
pub struct SystemVariableAssignments<'a> {
    source: AssignmentSource<'a>,
    preceding_keyword_scope: Option<ContextModifier>,
    finished: bool,
}

pub fn mysql_system_variable_assignments(
    input: &Set,
) -> Result<SystemVariableAssignments<'_>, SessionInputError> {
    let source = match input {
        Set::SingleAssignment {
            scope,
            hivevar: false,
            variable,
            values,
        } => {
            let [value] = values.as_slice() else {
                return Err(SessionInputError::AssignmentExpressionCount);
            };
            AssignmentSource::Single(Some((*scope, variable, value)))
        }
        Set::MultipleAssignments { assignments } => {
            if assignments.is_empty() {
                return Err(SessionInputError::EmptyAssignments);
            }
            AssignmentSource::Multiple(assignments.iter())
        }
        _ => return Err(SessionInputError::UnsupportedVariableSet),
    };
    Ok(SystemVariableAssignments {
        source,
        preceding_keyword_scope: None,
        finished: false,
    })
}

impl<'a> Iterator for SystemVariableAssignments<'a> {
    type Item = Result<SystemVariableAssignment<'a>, SessionInputError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let next = match &mut self.source {
            AssignmentSource::Single(single) => single.take(),
            AssignmentSource::Multiple(assignments) => assignments
                .next()
                .map(|assignment| (assignment.scope, &assignment.name, &assignment.value)),
        };
        let Some((explicit_keyword_scope, name, value)) = next else {
            self.finished = true;
            return None;
        };
        let preceding_keyword_scope = self.preceding_keyword_scope;
        self.preceding_keyword_scope = explicit_keyword_scope.or(preceding_keyword_scope);
        let result = assignment_name(name).map(|(variable, form)| SystemVariableAssignment {
            variable,
            form,
            name,
            value,
            explicit_keyword_scope,
            preceding_keyword_scope,
        });
        if result.is_err() {
            self.finished = true;
        }
        Some(result)
    }
}

impl std::iter::FusedIterator for SystemVariableAssignments<'_> {}

/// Recognize an ordinary scoped variable reference without selecting its
/// value. Quoted column identifiers are not system-variable reads.
pub fn classify_mysql_system_variable_read(
    expression: &Expr,
) -> Result<SystemVariableRead<'_>, SessionInputError> {
    let Expr::MySqlSystemVariable(input) = expression else {
        return Err(SessionInputError::NotSystemVariableRead);
    };
    if input.scope.is_none() && !matches!(input.name.quote_style, None | Some('`')) {
        return Err(SessionInputError::UnsupportedVariableName);
    }
    let form = system_variable_form(input)?;
    let variable = registry_name(&input.name)?;
    Ok(SystemVariableRead {
        variable,
        form,
        expression,
    })
}

fn assignment_name(
    name: &SetAssignmentTarget,
) -> Result<(SessionVariable, SystemVariableForm), SessionInputError> {
    let (name, form) = match name {
        SetAssignmentTarget::ObjectName(name) => {
            let [part] = name.0.as_slice() else {
                return Err(SessionInputError::UnsupportedVariableName);
            };
            let name = part
                .as_ident()
                .ok_or(SessionInputError::UnsupportedVariableName)?;
            (name, SystemVariableForm::Bare)
        }
        SetAssignmentTarget::MySqlSystemVariable(input) => {
            (&input.name, system_variable_form(input)?)
        }
    };
    if !matches!(name.quote_style, None | Some('`') | Some('"')) {
        return Err(SessionInputError::UnsupportedVariableName);
    }
    Ok((registry_name(name)?, form))
}

fn system_variable_form(
    input: &MySqlSystemVariable,
) -> Result<SystemVariableForm, SessionInputError> {
    if input.prefix.is_some() {
        return Err(SessionInputError::UnsupportedVariableName);
    }
    Ok(match input.scope {
        None => SystemVariableForm::UnqualifiedAt,
        Some(MySqlSystemVariableScope::Session) => {
            SystemVariableForm::Qualified(ContextModifier::Session)
        }
        Some(MySqlSystemVariableScope::Local) => {
            SystemVariableForm::Qualified(ContextModifier::Local)
        }
        Some(MySqlSystemVariableScope::Global) => {
            SystemVariableForm::Qualified(ContextModifier::Global)
        }
        Some(scope) => return Err(SessionInputError::UnsupportedVariableScope(scope)),
    })
}

fn registry_name(name: &Ident) -> Result<SessionVariable, SessionInputError> {
    SessionVariable::ALL
        .into_iter()
        .find(|variable| variable.name().eq_ignore_ascii_case(&name.value))
        .ok_or(SessionInputError::UnknownVariable)
}
