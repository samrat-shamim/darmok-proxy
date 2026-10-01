//! SQL-mode values, independent of SQL assignment coercion and execution.
//!
//! Recognizing a mode is not evidence that its query/write semantics are
//! implemented. The complete set is retained even when only a subset affects
//! the parser, so translation identity cannot discard validation modes.

use sqlparser::mysql_mode::MySqlModeFlags;
use thiserror::Error;

/// Public MySQL 8.4 mode names in canonical readback order. Numeric MySQL mode
/// masks and internal/obsolete names are not part of this names-only interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SqlMode {
    RealAsFloat,
    PipesAsConcat,
    AnsiQuotes,
    IgnoreSpace,
    OnlyFullGroupBy,
    NoUnsignedSubtraction,
    NoDirInCreate,
    Ansi,
    NoAutoValueOnZero,
    NoBackslashEscapes,
    StrictTransTables,
    StrictAllTables,
    NoZeroInDate,
    NoZeroDate,
    AllowInvalidDates,
    ErrorForDivisionByZero,
    Traditional,
    HighNotPrecedence,
    NoEngineSubstitution,
    PadCharToFullLength,
    TimeTruncateFractional,
}

impl SqlMode {
    pub const ALL: [Self; 21] = [
        Self::RealAsFloat,
        Self::PipesAsConcat,
        Self::AnsiQuotes,
        Self::IgnoreSpace,
        Self::OnlyFullGroupBy,
        Self::NoUnsignedSubtraction,
        Self::NoDirInCreate,
        Self::Ansi,
        Self::NoAutoValueOnZero,
        Self::NoBackslashEscapes,
        Self::StrictTransTables,
        Self::StrictAllTables,
        Self::NoZeroInDate,
        Self::NoZeroDate,
        Self::AllowInvalidDates,
        Self::ErrorForDivisionByZero,
        Self::Traditional,
        Self::HighNotPrecedence,
        Self::NoEngineSubstitution,
        Self::PadCharToFullLength,
        Self::TimeTruncateFractional,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::RealAsFloat => "REAL_AS_FLOAT",
            Self::PipesAsConcat => "PIPES_AS_CONCAT",
            Self::AnsiQuotes => "ANSI_QUOTES",
            Self::IgnoreSpace => "IGNORE_SPACE",
            Self::OnlyFullGroupBy => "ONLY_FULL_GROUP_BY",
            Self::NoUnsignedSubtraction => "NO_UNSIGNED_SUBTRACTION",
            Self::NoDirInCreate => "NO_DIR_IN_CREATE",
            Self::Ansi => "ANSI",
            Self::NoAutoValueOnZero => "NO_AUTO_VALUE_ON_ZERO",
            Self::NoBackslashEscapes => "NO_BACKSLASH_ESCAPES",
            Self::StrictTransTables => "STRICT_TRANS_TABLES",
            Self::StrictAllTables => "STRICT_ALL_TABLES",
            Self::NoZeroInDate => "NO_ZERO_IN_DATE",
            Self::NoZeroDate => "NO_ZERO_DATE",
            Self::AllowInvalidDates => "ALLOW_INVALID_DATES",
            Self::ErrorForDivisionByZero => "ERROR_FOR_DIVISION_BY_ZERO",
            Self::Traditional => "TRADITIONAL",
            Self::HighNotPrecedence => "HIGH_NOT_PRECEDENCE",
            Self::NoEngineSubstitution => "NO_ENGINE_SUBSTITUTION",
            Self::PadCharToFullLength => "PAD_CHAR_TO_FULL_LENGTH",
            Self::TimeTruncateFractional => "TIME_TRUNCATE_FRACTIONAL",
        }
    }

    const fn bit(self) -> u32 {
        1 << self as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SqlModeNamesError {
    #[error("unrecognized or unsupported SQL mode name")]
    UnknownName,
}

/// Closed, allocation-free mode identity. Its bits are private implementation
/// details, not MySQL's numeric SQL-mode assignment format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SqlModes(u32);

impl SqlModes {
    pub const MYSQL84_DEFAULT: Self = Self(
        SqlMode::OnlyFullGroupBy.bit()
            | SqlMode::StrictTransTables.bit()
            | SqlMode::NoZeroInDate.bit()
            | SqlMode::NoZeroDate.bit()
            | SqlMode::ErrorForDivisionByZero.bit()
            | SqlMode::NoEngineSubstitution.bit(),
    );

    pub const fn empty() -> Self {
        Self(0)
    }

    /// Validate a comma-separated list of public mode names. This is not SQL
    /// expression evaluation, numeric coercion, SET DEFAULT, or warning policy.
    pub fn parse_names(names: &str) -> Result<Self, SqlModeNamesError> {
        let names = names.trim_end_matches(' ');
        if names.is_empty() {
            return Ok(Self::empty());
        }
        let mut modes = Self::empty();
        for token in names.split(',') {
            if token.is_empty() {
                continue;
            }
            let mode = SqlMode::ALL
                .into_iter()
                .find(|mode| mode.name().eq_ignore_ascii_case(token))
                .ok_or(SqlModeNamesError::UnknownName)?;
            modes = modes.with(mode);
        }
        Ok(modes)
    }

    pub const fn contains(self, mode: SqlMode) -> bool {
        self.0 & mode.bit() != 0
    }

    /// Add a named mode, retaining composite names and all their constituent
    /// modes. Composite expansion changes no transaction characteristics.
    pub const fn with(mut self, mode: SqlMode) -> Self {
        self.0 |= mode.bit();
        match mode {
            SqlMode::Ansi => {
                self.0 |= SqlMode::RealAsFloat.bit()
                    | SqlMode::PipesAsConcat.bit()
                    | SqlMode::AnsiQuotes.bit()
                    | SqlMode::IgnoreSpace.bit()
                    | SqlMode::OnlyFullGroupBy.bit();
            }
            SqlMode::Traditional => {
                self.0 |= SqlMode::StrictTransTables.bit()
                    | SqlMode::StrictAllTables.bit()
                    | SqlMode::NoZeroInDate.bit()
                    | SqlMode::NoZeroDate.bit()
                    | SqlMode::ErrorForDivisionByZero.bit()
                    | SqlMode::NoEngineSubstitution.bit();
            }
            _ => {}
        }
        self
    }

    pub fn canonical_names(self) -> String {
        let capacity = SqlMode::ALL
            .into_iter()
            .filter(|mode| self.contains(*mode))
            .map(|mode| mode.name().len() + 1)
            .sum();
        let mut result = String::with_capacity(capacity);
        for mode in SqlMode::ALL {
            if self.contains(mode) {
                if !result.is_empty() {
                    result.push(',');
                }
                result.push_str(mode.name());
            }
        }
        result
    }

    /// Parser projection only. Other modes remain in this value and must be
    /// checked by admission/execution; this does not approve any SQL construct.
    pub fn parser_flags(self) -> MySqlModeFlags {
        let mut flags = MySqlModeFlags::empty();
        for (mode, flag) in [
            (SqlMode::RealAsFloat, MySqlModeFlags::REAL_AS_FLOAT),
            (SqlMode::PipesAsConcat, MySqlModeFlags::PIPES_AS_CONCAT),
            (SqlMode::AnsiQuotes, MySqlModeFlags::ANSI_QUOTES),
            (SqlMode::IgnoreSpace, MySqlModeFlags::IGNORE_SPACE),
            (
                SqlMode::NoBackslashEscapes,
                MySqlModeFlags::NO_BACKSLASH_ESCAPES,
            ),
            (
                SqlMode::HighNotPrecedence,
                MySqlModeFlags::HIGH_NOT_PRECEDENCE,
            ),
        ] {
            if self.contains(mode) {
                flags.insert(flag);
            }
        }
        flags
    }
}
