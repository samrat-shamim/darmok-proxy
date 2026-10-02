// Copyright 2026 Darmok contributors. Licensed under Apache-2.0.

//! MySQL system-variable syntax, distinct from ordinary column identifiers.

use core::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "visitor")]
use sqlparser_derive::{Visit, VisitMut};

use super::{Ident, ObjectName};
use crate::tokenizer::Span;

/// Scope spelled after `@@`. Persistence scopes are assignment syntax only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum MySqlSystemVariableScope {
    /// The SESSION spelling.
    Session,
    /// The LOCAL spelling, retained independently of SESSION.
    Local,
    /// The GLOBAL spelling.
    Global,
    /// The assignment-only PERSIST spelling.
    Persist,
    /// The assignment-only PERSIST_ONLY spelling.
    PersistOnly,
}

impl fmt::Display for MySqlSystemVariableScope {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Session => "SESSION",
            Self::Local => "LOCAL",
            Self::Global => "GLOBAL",
            Self::Persist => "PERSIST",
            Self::PersistOnly => "PERSIST_ONLY",
        })
    }
}

/// A name following `@@`, with its scope and optional named-variable prefix.
/// Quotes belong to the name, never to the sigil. A quoted-text read retains
/// its delimiter in `Ident::quote_style`; assignment names require identifiers.
#[derive(Debug, Clone, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct MySqlSystemVariable {
    /// Qualifier after the sigil, if explicitly present.
    pub scope: Option<MySqlSystemVariableScope>,
    /// Optional named-instance prefix before the variable name.
    pub prefix: Option<Ident>,
    /// Name and its original delimiter, excluding the sigil and qualifier.
    pub name: Ident,
    /// Location of the `@@` token; excluded from semantic equality and hashing.
    pub sigil_span: Span,
}

// Source locations do not participate in semantic identity, as for Ident.
impl PartialEq for MySqlSystemVariable {
    fn eq(&self, other: &Self) -> bool {
        (self.scope, &self.prefix, &self.name) == (other.scope, &other.prefix, &other.name)
    }
}

impl Ord for MySqlSystemVariable {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.scope, &self.prefix, &self.name).cmp(&(other.scope, &other.prefix, &other.name))
    }
}

impl PartialOrd for MySqlSystemVariable {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for MySqlSystemVariable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.scope.hash(state);
        self.prefix.hash(state);
        self.name.hash(state);
    }
}

impl fmt::Display for MySqlSystemVariable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("@@")?;
        if let Some(scope) = self.scope {
            write!(f, "{scope}.")?;
        }
        if let Some(prefix) = &self.prefix {
            write!(f, "{prefix}.")?;
        }
        self.name.fmt(f)
    }
}

/// A SET target is either an ordinary name or explicit MySQL `@@` syntax.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum SetAssignmentTarget {
    /// An ordinary assignment name, including a bare MySQL variable.
    ObjectName(ObjectName),
    /// An explicit MySQL system-variable target beginning with `@@`.
    MySqlSystemVariable(MySqlSystemVariable),
}

impl From<ObjectName> for SetAssignmentTarget {
    fn from(name: ObjectName) -> Self {
        Self::ObjectName(name)
    }
}

impl fmt::Display for SetAssignmentTarget {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ObjectName(name) => name.fmt(f),
            Self::MySqlSystemVariable(variable) => variable.fmt(f),
        }
    }
}
