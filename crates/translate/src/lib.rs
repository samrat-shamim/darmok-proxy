//! Compiler foundations. Parsing and emission do not certify SQL semantics.
//! Execution must additionally pass the semantic and catalog-binding gates.

pub mod bindings;
pub mod frontend;

pub use bindings::EmittedStatement;
pub use frontend::{InputKind, ParseLimits, ParsedStatement, parse_statement};
