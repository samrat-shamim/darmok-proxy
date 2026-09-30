use std::fmt;

use darmok_types::{ProxyError, Result, TranslationError};
use sqlparser::ast::{AstLimits, Statement, check_ast, drop_ast};
use sqlparser::dialect::ModeAwareMySqlDialect;
use sqlparser::mysql_mode::MySqlModeFlags;
use sqlparser::parser::{Parser, ParserError};
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

/// Experimental cap for recursive processing. Resource verification remains
/// a release gate; this constant is not a certification of every parser path.
pub const MAX_AST_DEPTH: usize = 128;
pub const MAX_PARSER_RECURSION: usize = 128;

/// Separate lexical, parser and constructed-tree resource limits.
#[derive(Debug, Clone, Copy)]
pub struct ParseLimits {
    pub max_sql_bytes: usize,
    pub max_parameters: u16,
    pub max_recursion: usize,
    pub max_ast_depth: usize,
    pub max_ast_nodes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Text,
    Prepared,
}

/// One parsed statement with stable source-order client parameter identities.
/// It is not an executable plan: catalog binding and semantic validation follow.
pub struct ParsedStatement {
    ast: Option<Statement>,
    pub(crate) client_parameter_count: u16,
    limits: AstLimits,
    checked: bool,
}

impl fmt::Debug for ParsedStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedStatement")
            .field("client_parameter_count", &self.client_parameter_count)
            .finish_non_exhaustive()
    }
}

impl ParsedStatement {
    pub fn ast(&self) -> &Statement {
        self.ast.as_ref().expect("AST exists until drop")
    }

    /// Rewriters may move, duplicate or eliminate client placeholders, but may
    /// not create new identities. Emission checks that invariant again.
    pub fn ast_mut(&mut self) -> &mut Statement {
        self.checked = false;
        self.ast.as_mut().expect("AST exists until drop")
    }

    pub fn client_parameter_count(&self) -> u16 {
        self.client_parameter_count
    }

    pub(crate) fn check_limits(&self) -> Result<()> {
        check_ast(self.ast(), self.limits)
            .map_err(|_| rejected("ast_limit", "AST depth or node limit exceeded"))
    }
}

impl Drop for ParsedStatement {
    fn drop(&mut self) {
        if !self.checked {
            // A rewriter may have built a tree exceeding the admission limit.
            // Dropping that tree recursively could abort before returning its
            // validation error. Accepted unmodified trees use ordinary drop.
            if let Some(ast) = self.ast.take() {
                drop_ast(ast);
            }
        }
    }
}

/// Parse exactly one statement. Number `?` tokens before parsing so AST visitor
/// order and later rewrites cannot change the meaning of client parameter slots.
/// Tokens inside strings, identifiers and ordinary comments are never bindings.
pub fn parse_statement(
    sql: &str,
    input: InputKind,
    mode: MySqlModeFlags,
    limits: ParseLimits,
) -> Result<ParsedStatement> {
    if sql.len() > limits.max_sql_bytes {
        return Err(rejected(
            "input_limit",
            "SQL input exceeds the configured byte limit",
        ));
    }
    if limits.max_recursion == 0 || limits.max_recursion > MAX_PARSER_RECURSION {
        return Err(rejected(
            "recursion_limit",
            "parser recursion limit must be between 1 and 128",
        ));
    }
    if limits.max_ast_depth == 0
        || limits.max_ast_depth > MAX_AST_DEPTH
        || limits.max_ast_nodes == 0
    {
        return Err(rejected(
            "ast_limit",
            "AST depth must be between 1 and 128 and node limit must be positive",
        ));
    }
    let known_modes = MySqlModeFlags::ANSI_QUOTES
        | MySqlModeFlags::NO_BACKSLASH_ESCAPES
        | MySqlModeFlags::PIPES_AS_CONCAT
        | MySqlModeFlags::IGNORE_SPACE
        | MySqlModeFlags::HIGH_NOT_PRECEDENCE
        | MySqlModeFlags::REAL_AS_FLOAT;
    if mode.bits() & !known_modes != 0 {
        return Err(rejected("sql_mode", "unknown parser mode bits"));
    }
    let dialect = ModeAwareMySqlDialect::new(mode);
    let mut tokens = Tokenizer::new(&dialect, sql)
        .with_comment_hint_expansion(false)
        .tokenize_with_location()
        .map_err(|error| {
            ProxyError::translation(TranslationError::Parse(format!(
                "invalid MySQL token at line {}, column {}",
                error.location.line, error.location.column
            )))
        })?;
    let mut parameter_count = 0u16;
    for token in &mut tokens {
        match &mut token.token {
            Token::Placeholder(placeholder) => {
                if input != InputKind::Prepared || placeholder != "?" {
                    return Err(rejected(
                        "placeholder",
                        "only prepared input may contain ? parameters",
                    ));
                }
                if parameter_count == limits.max_parameters {
                    return Err(rejected(
                        "parameter_limit",
                        "prepared parameter limit exceeded",
                    ));
                }
                parameter_count += 1;
                *placeholder = format!("${parameter_count}");
            }
            Token::Whitespace(Whitespace::MultiLineComment(comment))
                if comment.starts_with('!') =>
            {
                // MySQL executes these comments; treating them as whitespace
                // would silently discard statements or clauses.
                return Err(rejected(
                    "executable_comment",
                    "MySQL executable comments are unsupported",
                ));
            }
            _ => {}
        }
    }
    let mut parser = Parser::new(&dialect)
        .with_recursion_limit(limits.max_recursion)
        .with_ast_limits(AstLimits {
            max_depth: limits.max_ast_depth,
            max_nodes: limits.max_ast_nodes,
        })
        .with_tokens_with_locations(tokens);
    let mut statements = parser.parse_statements().map_err(|error| {
        if matches!(error, ParserError::AstLimitExceeded) {
            return rejected("ast_limit", "AST depth or node limit exceeded");
        }
        if matches!(error, ParserError::RecursionLimitExceeded) {
            return rejected("recursion_limit", "parser recursion limit exceeded");
        }
        let location = parser.peek_token().span.start;
        ProxyError::translation(TranslationError::Parse(format!(
            "invalid or unsupported MySQL syntax near line {}, column {}",
            location.line, location.column
        )))
    })?;
    if statements.len() != 1 {
        drop_ast(statements);
        return Err(rejected(
            "statement_count",
            "exactly one SQL statement is required",
        ));
    }
    let mut parsed = ParsedStatement {
        ast: Some(statements.remove(0)),
        client_parameter_count: parameter_count,
        limits: AstLimits {
            max_depth: limits.max_ast_depth,
            max_nodes: limits.max_ast_nodes,
        },
        checked: false,
    };
    parsed.check_limits()?;
    parsed.checked = true;
    Ok(parsed)
}

pub(crate) fn rejected(id: &'static str, message: &str) -> ProxyError {
    ProxyError::translation(TranslationError::unsupported_syntax(id, message))
}
