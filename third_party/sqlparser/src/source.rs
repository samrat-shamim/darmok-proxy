// Darmok extension, licensed under Apache-2.0.

//! Immutable MySQL AST and lexical projection provenance from one parse.
//! Source slices are never reconstructed from Display or a second SQL lexer.

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};
use core::{fmt, ops::Range};

use crate::{
    ast::{Query, Select, SetExpr, Statement, ValueWithSpan},
    dialect::ModeAwareMySqlDialect,
    mysql_mode::MySqlModeFlags,
    parser::{Parser, ParserError},
    tokenizer::{Location, Span, Token, Tokenizer, Whitespace},
};

#[derive(Debug)]
struct ProjectionSource {
    select: Span,
    items: Vec<Range<usize>>,
}

/// An original SQL borrow, its immutable statements, and parser-owned ranges.
/// Callers needing a mutable AST use the ordinary AST parsing APIs instead.
#[derive(Debug)]
pub struct ParsedMySqlSource<'sql> {
    sql: &'sql str,
    statements: Vec<Statement>,
    projections: Vec<ProjectionSource>,
    numbers: Vec<(Span, Range<usize>)>,
}

/// A SELECT borrow bound to the same parse that owns its source ranges.
#[derive(Debug, Clone, Copy)]
pub struct SourceSelect<'a> {
    query: &'a Query,
    select: &'a Select,
    sql: &'a str,
    items: &'a [Range<usize>],
    numbers: &'a [(Span, Range<usize>)],
}

/// Missing or ambiguous parser provenance is an internal contract error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionSourceError;

impl fmt::Display for ProjectionSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SELECT projection source does not match its immutable parse")
    }
}
impl core::error::Error for ProjectionSourceError {}

/// Distinguishes unsupported source shapes, SQL syntax errors and internal
/// provenance failures.
#[derive(Debug)]
pub enum MySqlSourceParseError {
    /// Executable comments need a version-aware admission and source contract.
    UnsupportedExecutableComment,
    /// The original SQL could not be parsed.
    Parse(ParserError),
    /// Parser-owned source coordinates could not be resolved.
    Source(ProjectionSourceError),
}
impl From<ParserError> for MySqlSourceParseError {
    fn from(error: ParserError) -> Self {
        Self::Parse(error)
    }
}
impl From<ProjectionSourceError> for MySqlSourceParseError {
    fn from(error: ProjectionSourceError) -> Self {
        Self::Source(error)
    }
}
impl fmt::Display for MySqlSourceParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedExecutableComment => {
                f.write_str("MySQL executable comments are not implemented")
            }
            Self::Parse(error) => error.fmt(f),
            Self::Source(error) => error.fmt(f),
        }
    }
}
impl core::error::Error for MySqlSourceParseError {}

impl ParsedMySqlSource<'_> {
    /// Read the statements without detaching or mutating their provenance.
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }

    /// Bind a single direct SELECT to its captured source. Other statement or
    /// set-expression shapes return None, rather than an approximate SELECT.
    pub fn single_select(&self) -> Result<Option<SourceSelect<'_>>, ProjectionSourceError> {
        let [Statement::Query(query)] = self.statements.as_slice() else {
            return Ok(None);
        };
        let SetExpr::Select(select) = query.body.as_ref() else {
            return Ok(None);
        };
        let mut sources = self
            .projections
            .iter()
            .filter(|source| source.select == select.select_token.0.span);
        let source = sources.next().ok_or(ProjectionSourceError)?;
        if sources.next().is_some() || source.items.len() != select.projection.len() {
            return Err(ProjectionSourceError);
        }
        Ok(Some(SourceSelect {
            query,
            select,
            sql: self.sql,
            items: &source.items,
            numbers: &self.numbers,
        }))
    }
}

impl<'a> SourceSelect<'a> {
    /// Original query clauses, which an execution controller must admit.
    pub fn query(self) -> &'a Query {
        self.query
    }

    /// Original direct SELECT, including the ordered projection expressions.
    pub fn select(self) -> &'a Select {
        self.select
    }

    /// Complete item spelling, including parentheses, operators and aliases.
    pub fn item_source(self, index: usize) -> Result<&'a str, ProjectionSourceError> {
        let range = self.items.get(index).ok_or(ProjectionSourceError)?;
        self.sql.get(range.clone()).ok_or(ProjectionSourceError)
    }

    /// Original numeric token, including zeros or exponent spelling that an
    /// optional BigDecimal AST representation may normalize away.
    pub fn number_source(self, value: &ValueWithSpan) -> Result<&'a str, ProjectionSourceError> {
        let index = self
            .numbers
            .binary_search_by_key(&value.span, |(span, _)| *span)
            .map_err(|_| ProjectionSourceError)?;
        let (_, range) = &self.numbers[index];
        self.sql.get(range.clone()).ok_or(ProjectionSourceError)
    }
}

/// Parse once with current MySQL modes and retain complete projection spelling.
/// Token coordinates count Unicode characters, not bytes. Convert all captured
/// endpoints together in one character walk after sorting them; never rescan a
/// long source line for each projection. The AST-only path does not allocate
/// this opt-in provenance.
pub fn parse_mysql_source(
    sql: &str,
    flags: MySqlModeFlags,
) -> Result<ParsedMySqlSource<'_>, MySqlSourceParseError> {
    let dialect = ModeAwareMySqlDialect::new(flags);
    // The upstream expansion strips delimiters/version digits and starts inner
    // token coordinates at the opening comment. It also ignores server version
    // gates. Retain executable comments in this single tokenization and reject
    // them before AST parsing; neither approximate coordinates nor unconditional
    // expansion can supply the source/semantic contract required by this API.
    let tokens = Tokenizer::new(&dialect, sql)
        .with_executable_comment_expansion(false)
        .tokenize_with_location()
        .map_err(ParserError::from)?;
    if tokens.iter().any(|token| {
        matches!(&token.token, Token::Whitespace(Whitespace::MultiLineComment(comment))
            if comment.starts_with('!'))
    }) {
        return Err(MySqlSourceParseError::UnsupportedExecutableComment);
    }
    let mut parser = Parser::new(&dialect).with_tokens_with_locations(tokens);
    parser.projection_source = Some(vec![]);
    let statements = parser.parse_statements()?;
    let captured = parser.projection_source.take().expect("capture enabled");
    let numbers = if captured.is_empty() {
        vec![]
    } else {
        parser.numeric_source_spans()
    };
    let spans: Vec<_> = captured
        .iter()
        .flat_map(|(_, items)| items.iter().copied())
        .chain(numbers.iter().copied())
        .collect();
    let mut ranges = byte_ranges(sql, &spans)?.into_iter();
    let projections = captured
        .into_iter()
        .map(|(select, items)| ProjectionSource {
            select,
            items: ranges.by_ref().take(items.len()).collect(),
        })
        .collect();
    let numbers = numbers.into_iter().zip(ranges).collect();
    Ok(ParsedMySqlSource {
        sql,
        statements,
        projections,
        numbers,
    })
}

fn byte_ranges(sql: &str, spans: &[Span]) -> Result<Vec<Range<usize>>, ProjectionSourceError> {
    if spans.is_empty() {
        return Ok(vec![]);
    }
    let mut endpoints: Vec<_> = spans
        .iter()
        .enumerate()
        .flat_map(|(index, span)| [(span.start, index * 2), (span.end, index * 2 + 1)])
        .collect();
    endpoints.sort_unstable_by_key(|(location, _)| *location);
    let mut offsets = vec![0; endpoints.len()];
    let mut next = 0;
    let mut location = Location::new(1, 1);
    // Tokenizer spans end at the position after the consumed token, despite
    // Span's upstream inclusive-end description. EOF completes that position.
    for (offset, ch) in sql
        .char_indices()
        .map(|(i, c)| (i, Some(c)))
        .chain([(sql.len(), None)])
    {
        while next < endpoints.len() && endpoints[next].0 == location {
            offsets[endpoints[next].1] = offset;
            next += 1;
        }
        match ch {
            Some('\n') => {
                location.line += 1;
                location.column = 1;
            }
            Some(_) => location.column += 1,
            None => {}
        }
    }
    if next != endpoints.len() {
        return Err(ProjectionSourceError);
    }
    Ok(offsets
        .chunks_exact(2)
        .map(|offset| offset[0]..offset[1])
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_items_retain_wrappers_spaces_comments_and_unicode() {
        let parsed = parse_mysql_source(
            "SELECT 'hé',\n ( @@SESSION . AUTOCOMMIT ), (- 1), /* note */ TRUE AS `truth`",
            MySqlModeFlags::empty(),
        )
        .unwrap();
        let select = parsed.single_select().unwrap().unwrap();
        for (index, expected) in [
            "'hé'",
            "( @@SESSION . AUTOCOMMIT )",
            "(- 1)",
            "TRUE AS `truth`",
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(select.item_source(index).unwrap(), *expected);
        }
    }

    #[test]
    fn nested_select_provenance_stays_with_its_owner() {
        let parsed = parse_mysql_source(
            "SELECT (SELECT 2 AS inner_value) AS outer_value, @@session.autocommit",
            MySqlModeFlags::empty(),
        )
        .unwrap();
        let select = parsed.single_select().unwrap().unwrap();
        assert_eq!(
            select.item_source(0).unwrap(),
            "(SELECT 2 AS inner_value) AS outer_value"
        );
        assert_eq!(select.item_source(1).unwrap(), "@@session.autocommit");
    }

    #[test]
    fn executable_comments_have_an_explicit_source_admission_boundary() {
        for sql in [
            "SELECT /*! TRUE */",
            "SELECT /*!80411 001 */",
            "/*! SELECT TRUE */",
            "SET /*! autocommit=1 */",
            "SELECT 1 /*!99999 + 2 */",
            "SELECT 1 /*! */",
        ] {
            assert!(matches!(
                parse_mysql_source(sql, MySqlModeFlags::empty()),
                Err(MySqlSourceParseError::UnsupportedExecutableComment)
            ));
        }
        // Matching characters in literal values, quoted identifiers, line
        // comments or an ordinary multiline comment are not executable tokens.
        let parsed = parse_mysql_source(
            "SELECT '/*! TRUE */' AS `/*! alias */`, /* note ! */ (001) -- /*! ignored */\n",
            MySqlModeFlags::empty(),
        )
        .unwrap();
        let select = parsed.single_select().unwrap().unwrap();
        assert_eq!(
            select.item_source(0).unwrap(),
            "'/*! TRUE */' AS `/*! alias */`"
        );
        assert_eq!(select.item_source(1).unwrap(), "(001)");
    }
}
