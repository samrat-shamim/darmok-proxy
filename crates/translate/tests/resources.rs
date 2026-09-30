use std::process::Command;

use darmok_translate::{InputKind, ParseLimits, ParsedStatement, parse_statement};
use sqlparser::ast::{
    BinaryOperator, DataType, Expr, SelectItem, SetExpr, SetOperator, SetQuantifier, Statement,
    StructField, Value,
};
use sqlparser::mysql_mode::MySqlModeFlags;

fn limits() -> ParseLimits {
    ParseLimits {
        max_sql_bytes: 1_000_000,
        max_parameters: 256,
        max_recursion: 128,
        max_ast_depth: 128,
        max_ast_nodes: 100_000,
    }
}

fn parse(sql: &str) -> darmok_types::Result<ParsedStatement> {
    parse_statement(sql, InputKind::Text, MySqlModeFlags::empty(), limits())
}

fn number() -> Expr {
    Expr::Value(Value::Number("1".parse().unwrap(), false).with_empty_span())
}

fn projection(parsed: &mut ParsedStatement) -> &mut Expr {
    let Statement::Query(query) = parsed.ast_mut() else {
        panic!("query")
    };
    let SetExpr::Select(select) = query.body.as_mut() else {
        panic!("select")
    };
    let SelectItem::UnnamedExpr(expr) = &mut select.projection[0] else {
        panic!("expression")
    };
    expr
}

// Stack overflow aborts a process, so isolate the regression in a subprocess.
// The child creates the same 2 MiB stack on every host, including main-thread
// stacks that would otherwise hide the problem on developer machines.
#[test]
#[ignore = "Resource stress verification is deferred from the current work scope"]
fn recursive_processing_obeys_resource_limits() {
    const CHILD: &str = "DARMOK_AST_RESOURCE_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(exercise_limits)
            .unwrap()
            .join()
            .unwrap();
        return;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "recursive_processing_obeys_resource_limits",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn exercise_limits() {
    // Valid shallow and wide statements still clone, visit, format and drop.
    for sql in [
        format!("SELECT {}", vec!["1"; 32].join(" + ")),
        format!("SELECT {}", vec!["1"; 1000].join(", ")),
        format!("SELECT {}1{}", "ABS(".repeat(8), ")".repeat(8)),
    ] {
        let parsed = parse(&sql).unwrap();
        assert!(!parsed.emit_postgres().unwrap().sql().is_empty());
    }

    for count in [512, 4096, 16_384] {
        for operator in [" + ", " AND ", " OR "] {
            let sql = format!("SELECT {}", vec!["1"; count].join(operator));
            assert!(parse(&sql).is_err());
            // Error after an otherwise complete prefix must also release all
            // partial results without recursive destruction of a deep chain.
            assert!(parse(&format!("{sql} +")).is_err());
        }
        let sql = vec!["SELECT 1"; count].join(" UNION ALL ");
        assert!(parse(&sql).is_err());
        assert!(parse(&format!("{sql} UNION")).is_err());
    }
    for count in [128, 512, 4096] {
        for prefix in ["ABS(", "("] {
            let sql = format!("SELECT {}1{}", prefix.repeat(count), ")".repeat(count));
            assert!(parse(&sql).is_err());
        }
    }

    // Mutations are a second admission boundary. Exercise both Box and Vec
    // recursion, and cleanup with and without attempting emission.
    for tuple in [false, true] {
        for emit in [false, true] {
            let mut parsed = parse("SELECT 1").unwrap();
            let mut expr = number();
            for _ in 0..30_000 {
                expr = if tuple {
                    Expr::Tuple(vec![expr])
                } else {
                    Expr::BinaryOp {
                        left: Box::new(expr),
                        op: BinaryOperator::Plus,
                        right: Box::new(number()),
                    }
                };
            }
            *projection(&mut parsed) = expr;
            if emit {
                assert!(
                    parsed
                        .emit_postgres()
                        .unwrap_err()
                        .to_string()
                        .contains("ast_limit")
                );
            }
            drop(parsed);
        }
    }
    let mut parsed = parse("SELECT 1").unwrap();
    let Statement::Query(query) = parsed.ast_mut() else {
        panic!("query")
    };
    let leaf = query.body.clone();
    let mut tree = query.body.clone();
    for _ in 0..10_000 {
        tree = Box::new(SetExpr::SetOperation {
            left: tree,
            op: SetOperator::Union,
            set_quantifier: SetQuantifier::All,
            right: leaf.clone(),
        });
    }
    query.body = tree;
    assert!(parsed.emit_postgres().is_err());
    drop(parsed);

    let mut parsed = parse("CREATE TABLE t (v INT)").unwrap();
    let mut data_type = DataType::Int(None);
    for _ in 0..30_000 {
        data_type = DataType::Tuple(vec![StructField {
            field_name: None,
            field_type: data_type,
            options: None,
        }]);
    }
    let Statement::CreateTable(table) = parsed.ast_mut() else {
        panic!("create")
    };
    table.columns[0].data_type = data_type;
    assert!(parsed.emit_postgres().is_err());
    drop(parsed);

    for invalid in [
        ParseLimits {
            max_ast_depth: 0,
            ..limits()
        },
        ParseLimits {
            max_ast_depth: 129,
            ..limits()
        },
        ParseLimits {
            max_ast_nodes: 0,
            ..limits()
        },
        ParseLimits {
            max_ast_nodes: 2,
            ..limits()
        },
        ParseLimits {
            max_recursion: 129,
            ..limits()
        },
    ] {
        assert!(
            parse_statement(
                "SELECT 1",
                InputKind::Text,
                MySqlModeFlags::empty(),
                invalid
            )
            .is_err()
        );
    }
}
