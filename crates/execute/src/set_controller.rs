//! Executes the admitted, catalog-independent SET subset. Transaction starts,
//! row execution and native/frontend isolation equivalence are separate gates.

use crate::{NativeBackend, QueryExecutionError};
use darmok_session::{
    AutocommitSetting, FrontendIsolation, FrontendTransactionAccess,
    NamedTransactionCharacteristic, SessionCommandStage, SessionState, SessionVariable,
    SessionVariableReader, SqlMode, SqlModes, SystemVariableAssignment, SystemVariableForm,
    TransactionCharacteristics, TransactionSettingAssignment, TransactionVariableAssignmentForm,
    classify_mysql_system_variable_read, classify_mysql_transaction_setting,
    mysql_system_variable_assignments,
};
use darmok_types::value::Value;
use sqlparser::ast::{ContextModifier, Expr, Set, Value as Literal};
use std::borrow::Cow;

/// Explicit server authority for global reads and SESSION DEFAULT. There is
/// no ambient PostgreSQL lookup, invented DEFAULT, or global setting writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSetValues {
    pub sql_modes: SqlModes,
    pub transactions: TransactionCharacteristics,
    pub autocommit: AutocommitSetting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetSqlError {
    Unsupported,
    WrongValue,
    NextChoicesDuringActive,
}

impl SetSqlError {
    pub fn code(self) -> u16 {
        match self {
            Self::Unsupported => 1235,
            Self::WrongValue => 1231,
            Self::NextChoicesDuringActive => 1568,
        }
    }
    pub fn sql_state(self) -> [u8; 5] {
        match self {
            Self::Unsupported | Self::WrongValue => *b"42000",
            Self::NextChoicesDuringActive => *b"25001",
        }
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::Unsupported => "This SET setting, expression or outcome is not implemented",
            Self::WrongValue => "Variable cannot be set to the supplied value",
            Self::NextChoicesDuringActive => {
                "Transaction characteristics can't be changed while a transaction is in progress"
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TypedValue {
    Modes(SqlModes),
    Isolation(FrontendIsolation),
    Access(FrontendTransactionAccess),
    Autocommit(AutocommitSetting),
}
#[derive(Debug, Clone, Copy)]
enum PlannedValue {
    Default(TypedValue),
    Checked(TypedValue),
}
#[derive(Debug, Clone, Copy)]
enum SetAction {
    Variable {
        variable: SessionVariable,
        form: TransactionVariableAssignmentForm,
        value: PlannedValue,
    },
    Transaction(TransactionSettingAssignment),
}
enum EvaluatedValue<'a> {
    String(Cow<'a, str>),
    Unsigned(u64),
    Boolean(bool),
    Null,
}

/// A pure admission result. It cannot be constructed outside this module.
pub(crate) struct SetPlan(Vec<SetAction>);

pub(crate) async fn apply_set(
    stage: &mut SessionCommandStage<'_>,
    backend: &mut NativeBackend,
    plan: SetPlan,
) -> Result<Result<(), SetSqlError>, QueryExecutionError> {
    for action in plan.0 {
        if let Err(error) = apply_action(stage, backend, action).await? {
            return Ok(Err(error));
        }
    }
    Ok(Ok(()))
}

pub(crate) fn admit_set(
    state: &SessionState,
    globals: &ServerSetValues,
    input: &Set,
) -> Result<SetPlan, SetSqlError> {
    let before = state
        .transaction_settings()
        .map_err(|_| SetSqlError::Unsupported)?;
    if let Set::SetTransaction(input) = input {
        let assignment =
            classify_mysql_transaction_setting(input).map_err(|_| SetSqlError::Unsupported)?;
        if matches!(assignment, TransactionSettingAssignment::NextTransaction(_))
            && before.active.is_some()
        {
            return Err(SetSqlError::NextChoicesDuringActive);
        }
        return Ok(SetPlan(vec![SetAction::Transaction(assignment)]));
    }
    let inputs = mysql_system_variable_assignments(input).map_err(|_| SetSqlError::Unsupported)?;
    let mut actions = Vec::new();
    for input in inputs {
        let input = input.map_err(|_| SetSqlError::Unsupported)?;
        let form = assignment_form(input)?;
        if !matches!(
            input.variable,
            SessionVariable::SqlMode
                | SessionVariable::Autocommit
                | SessionVariable::TransactionIsolation
                | SessionVariable::TransactionReadOnly
        ) {
            return Err(SetSqlError::Unsupported);
        }
        let value = if matches!(input.value, Expr::Identifier(name) if name.quote_style.is_none() && name.value.eq_ignore_ascii_case("DEFAULT"))
        {
            // Implementation support is an admission gate even for DEFAULT.
            // The active-next check still belongs to its update position.
            PlannedValue::Default(default_value(globals, input.variable)?)
        } else {
            let value = coerce(input.variable, evaluate(state, globals, input.value)?)?;
            check_phase(input.variable, form, value, before)?;
            PlannedValue::Checked(value)
        };
        actions.push(SetAction::Variable {
            variable: input.variable,
            form,
            value,
        });
    }
    Ok(SetPlan(actions))
}

fn assignment_form(
    input: SystemVariableAssignment<'_>,
) -> Result<TransactionVariableAssignmentForm, SetSqlError> {
    use ContextModifier as Scope;
    use TransactionVariableAssignmentForm as Form;
    if input.explicit_keyword_scope.is_some() && input.form != SystemVariableForm::Bare {
        return Err(SetSqlError::Unsupported);
    }
    Ok(match input.form {
        SystemVariableForm::Bare => match input.keyword_context() {
            None => Form::Bare,
            Some(Scope::Session | Scope::Local) => Form::SessionKeyword,
            _ => return Err(SetSqlError::Unsupported),
        },
        SystemVariableForm::UnqualifiedAt => Form::UnqualifiedAt,
        SystemVariableForm::Qualified(Scope::Session | Scope::Local) => Form::QualifiedSession,
        _ => return Err(SetSqlError::Unsupported),
    })
}

fn check_phase(
    variable: SessionVariable,
    form: TransactionVariableAssignmentForm,
    value: TypedValue,
    settings: darmok_session::TransactionSettingsSnapshot,
) -> Result<(), SetSqlError> {
    if matches!(
        variable,
        SessionVariable::TransactionIsolation | SessionVariable::TransactionReadOnly
    ) && form == TransactionVariableAssignmentForm::UnqualifiedAt
        && settings.active.is_some()
    {
        return Err(SetSqlError::NextChoicesDuringActive);
    }
    if matches!(value, TypedValue::Autocommit(_))
        && settings.active.is_some()
        && settings.autocommit == AutocommitSetting::Enabled
    {
        return Err(SetSqlError::Unsupported);
    }
    Ok(())
}

fn evaluate<'a>(
    state: &SessionState,
    globals: &ServerSetValues,
    expression: &'a Expr,
) -> Result<EvaluatedValue<'a>, SetSqlError> {
    Ok(match expression {
        Expr::Value(value) => match &value.value {
            Literal::SingleQuotedString(value) | Literal::DoubleQuotedString(value) => {
                EvaluatedValue::String(Cow::Borrowed(value))
            }
            Literal::Boolean(value) => EvaluatedValue::Boolean(*value),
            Literal::Null => EvaluatedValue::Null,
            Literal::Number(value, false) => EvaluatedValue::Unsigned(
                value
                    .to_string()
                    .parse()
                    .map_err(|_| SetSqlError::Unsupported)?,
            ),
            _ => return Err(SetSqlError::Unsupported),
        },
        Expr::Identifier(name)
            if name.quote_style.is_none()
                && (name.value.eq_ignore_ascii_case("ON")
                    || name.value.eq_ignore_ascii_case("OFF")) =>
        {
            EvaluatedValue::String(Cow::Borrowed(&name.value))
        }
        Expr::MySqlSystemVariable(_) => {
            let read = classify_mysql_system_variable_read(expression)
                .map_err(|_| SetSqlError::Unsupported)?;
            if !matches!(
                read.variable,
                SessionVariable::SqlMode
                    | SessionVariable::Autocommit
                    | SessionVariable::TransactionIsolation
                    | SessionVariable::TransactionReadOnly
            ) {
                return Err(SetSqlError::Unsupported);
            }
            let value = if read.form == SystemVariableForm::Qualified(ContextModifier::Global) {
                global_value(globals, read.variable)?
            } else {
                state
                    .read_variable(read.variable)
                    .map_err(|_| SetSqlError::Unsupported)?
            };
            match value {
                Value::String(value) => EvaluatedValue::String(Cow::Owned(value.into())),
                Value::UInt(value) => EvaluatedValue::Unsigned(value),
                _ => return Err(SetSqlError::Unsupported),
            }
        }
        _ => return Err(SetSqlError::Unsupported),
    })
}

pub(crate) fn global_value(
    globals: &ServerSetValues,
    variable: SessionVariable,
) -> Result<Value, SetSqlError> {
    Ok(match variable {
        SessionVariable::SqlMode => Value::String(globals.sql_modes.canonical_names().into()),
        SessionVariable::TransactionIsolation => {
            Value::String(globals.transactions.isolation.variable_label().into())
        }
        SessionVariable::TransactionReadOnly => Value::UInt(u64::from(
            globals.transactions.access == FrontendTransactionAccess::ReadOnly,
        )),
        SessionVariable::Autocommit => {
            Value::UInt(u64::from(globals.autocommit == AutocommitSetting::Enabled))
        }
        _ => return Err(SetSqlError::Unsupported),
    })
}

fn mode_warning_needed(modes: SqlModes) -> bool {
    let strict =
        modes.contains(SqlMode::StrictTransTables) || modes.contains(SqlMode::StrictAllTables);
    let submodes = [
        SqlMode::NoZeroInDate,
        SqlMode::NoZeroDate,
        SqlMode::ErrorForDivisionByZero,
    ];
    let any = submodes.into_iter().any(|mode| modes.contains(mode));
    let all = submodes.into_iter().all(|mode| modes.contains(mode));
    modes.contains(SqlMode::PadCharToFullLength) || ((strict || any) && (!strict || !all))
}

fn boolean(value: EvaluatedValue<'_>) -> Result<bool, SetSqlError> {
    match value {
        EvaluatedValue::Boolean(value) => Ok(value),
        EvaluatedValue::Unsigned(0) => Ok(false),
        EvaluatedValue::Unsigned(1) => Ok(true),
        EvaluatedValue::String(value) if value.eq_ignore_ascii_case("OFF") => Ok(false),
        EvaluatedValue::String(value) if value.eq_ignore_ascii_case("ON") => Ok(true),
        _ => Err(SetSqlError::WrongValue),
    }
}

fn coerce(variable: SessionVariable, value: EvaluatedValue<'_>) -> Result<TypedValue, SetSqlError> {
    Ok(match variable {
        SessionVariable::Autocommit => TypedValue::Autocommit(if boolean(value)? {
            AutocommitSetting::Enabled
        } else {
            AutocommitSetting::Disabled
        }),
        SessionVariable::TransactionReadOnly => TypedValue::Access(if boolean(value)? {
            FrontendTransactionAccess::ReadOnly
        } else {
            FrontendTransactionAccess::ReadWrite
        }),
        SessionVariable::TransactionIsolation => TypedValue::Isolation(match value {
            EvaluatedValue::String(value) => [
                FrontendIsolation::ReadUncommitted,
                FrontendIsolation::ReadCommitted,
                FrontendIsolation::RepeatableRead,
                FrontendIsolation::Serializable,
            ]
            .into_iter()
            .find(|isolation| isolation.variable_label().eq_ignore_ascii_case(&value))
            .ok_or(SetSqlError::WrongValue)?,
            EvaluatedValue::Unsigned(0) => FrontendIsolation::ReadUncommitted,
            EvaluatedValue::Unsigned(1) => FrontendIsolation::ReadCommitted,
            EvaluatedValue::Unsigned(2) => FrontendIsolation::RepeatableRead,
            EvaluatedValue::Unsigned(3) => FrontendIsolation::Serializable,
            _ => return Err(SetSqlError::WrongValue),
        }),
        SessionVariable::SqlMode => {
            let modes = match value {
                EvaluatedValue::String(value) => {
                    SqlModes::parse_names(&value).map_err(|_| SetSqlError::WrongValue)?
                }
                EvaluatedValue::Unsigned(0) => SqlModes::empty(),
                EvaluatedValue::Unsigned(4) => SqlModes::empty().with(SqlMode::AnsiQuotes),
                EvaluatedValue::Unsigned(_) => return Err(SetSqlError::Unsupported),
                _ => return Err(SetSqlError::WrongValue),
            };
            if mode_warning_needed(modes) {
                return Err(SetSqlError::Unsupported);
            }
            TypedValue::Modes(modes)
        }
        _ => return Err(SetSqlError::Unsupported),
    })
}

fn default_value(
    globals: &ServerSetValues,
    variable: SessionVariable,
) -> Result<TypedValue, SetSqlError> {
    Ok(match variable {
        SessionVariable::SqlMode => {
            if mode_warning_needed(globals.sql_modes) {
                return Err(SetSqlError::Unsupported);
            }
            TypedValue::Modes(globals.sql_modes)
        }
        SessionVariable::TransactionIsolation => {
            TypedValue::Isolation(globals.transactions.isolation)
        }
        SessionVariable::TransactionReadOnly => TypedValue::Access(globals.transactions.access),
        SessionVariable::Autocommit => TypedValue::Autocommit(globals.autocommit),
        _ => return Err(SetSqlError::Unsupported),
    })
}

async fn apply_action(
    stage: &mut SessionCommandStage<'_>,
    backend: &mut NativeBackend,
    action: SetAction,
) -> Result<Result<(), SetSqlError>, QueryExecutionError> {
    let SetAction::Variable {
        variable,
        form,
        value,
    } = action
    else {
        if let SetAction::Transaction(assignment) = action {
            stage.apply_transaction_assignment(assignment)?;
        }
        return Ok(Ok(()));
    };
    let value = match value {
        PlannedValue::Checked(value) => value,
        PlannedValue::Default(value) => match (|| {
            check_phase(
                variable,
                form,
                value,
                stage
                    .settings()
                    .map_err(|_| SetSqlError::Unsupported)?
                    .transactions,
            )?;
            Ok(value)
        })() {
            Ok(value) => value,
            Err(error) => return Ok(Err(error)),
        },
    };
    match value {
        TypedValue::Modes(value) => stage.apply_sql_modes(value),
        TypedValue::Isolation(value) => {
            stage.apply_transaction_assignment(TransactionSettingAssignment::Variable {
                form,
                characteristic: NamedTransactionCharacteristic::Isolation(value),
            })?
        }
        TypedValue::Access(value) => {
            stage.apply_transaction_assignment(TransactionSettingAssignment::Variable {
                form,
                characteristic: NamedTransactionCharacteristic::Access(value),
            })?
        }
        TypedValue::Autocommit(value) => {
            if value == AutocommitSetting::Enabled
                && stage.settings()?.transactions.active.is_some()
            {
                let _receipt = backend.commit().await?;
                stage.record_autocommit_commit()?;
            }
            stage.apply_autocommit(value)?;
        }
    }
    Ok(Ok(()))
}
