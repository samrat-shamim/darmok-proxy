use darmok_translate::{EmittedStatement, InputKind, ParseLimits, parse_statement};
use darmok_types::Value;
use sqlparser::ast::{Expr, SetExpr, Statement};
use sqlparser::mysql_mode::MySqlModeFlags;
use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, NoTls};

#[tokio::test]
async fn native_postgres_executes_source_bindings_after_limit_and_predicate_rewrites() {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls)
        .await
        .expect("connect to required PostgreSQL test database");
    let connection_task = tokio::spawn(connection);
    client
        .batch_execute(
            "CREATE TEMPORARY TABLE bind_probe (id bigint PRIMARY KEY);
        INSERT INTO bind_probe VALUES (10), (20), (30), (40), (50), (60)",
        )
        .await
        .unwrap();

    let limits = ParseLimits {
        max_sql_bytes: 65536,
        max_parameters: 256,
        max_recursion: 64,
        max_ast_depth: 128,
        max_ast_nodes: 65536,
    };
    let parsed = parse_statement(
        "WITH chosen AS (SELECT id FROM bind_probe WHERE id >= ?)
        SELECT id FROM chosen ORDER BY id LIMIT ?, ?",
        InputKind::Prepared,
        MySqlModeFlags::empty(),
        limits,
    )
    .unwrap();
    let emitted = parsed.emit_postgres().unwrap();
    let ids = query_ids(
        &client,
        &emitted,
        &[Value::Int(20), Value::Int(1), Value::Int(2)],
    )
    .await;
    assert_eq!(ids, vec![30, 40]);

    let mut parsed = parse_statement(
        "SELECT id FROM bind_probe WHERE id = ? OR id = ?",
        InputKind::Prepared,
        MySqlModeFlags::empty(),
        limits,
    )
    .unwrap();
    let Statement::Query(query) = parsed.ast_mut() else {
        panic!("query expected");
    };
    let SetExpr::Select(select) = query.body.as_mut() else {
        panic!("select expected");
    };
    let Expr::BinaryOp { right, .. } = select.selection.take().unwrap() else {
        panic!("OR expected");
    };
    select.selection = Some(*right);
    let emitted = parsed.emit_postgres().unwrap();
    assert_eq!(emitted.parameter_order(), &[1]);
    let ids = query_ids(&client, &emitted, &[Value::Int(10), Value::Int(50)]).await;
    assert_eq!(ids, vec![50]);

    client
        .batch_execute(
            "CREATE TEMPORARY TABLE dollar_probe (\"$1\" bigint);
        INSERT INTO dollar_probe VALUES (71)",
        )
        .await
        .unwrap();
    let parsed = parse_statement(
        "SELECT $1 FROM dollar_probe WHERE $1 > ?",
        InputKind::Prepared,
        MySqlModeFlags::empty(),
        limits,
    )
    .unwrap();
    let emitted = parsed.emit_postgres().unwrap();
    assert_eq!(
        query_ids(&client, &emitted, &[Value::Int(20)]).await,
        vec![71]
    );
    client
        .batch_execute(
            r#"CREATE TEMPORARY TABLE keyword_probe ("user" bigint);
        INSERT INTO keyword_probe VALUES (71)"#,
        )
        .await
        .unwrap();
    let parsed = parse_statement(
        "SELECT user FROM keyword_probe WHERE CURRENT_DATE = CURRENT_DATE",
        InputKind::Text,
        MySqlModeFlags::empty(),
        limits,
    )
    .unwrap();
    let emitted = parsed.emit_postgres().unwrap();
    assert_eq!(query_ids(&client, &emitted, &[]).await, vec![71]);
    drop(client);
    connection_task.await.unwrap().unwrap();
}

async fn query_ids(client: &Client, emitted: &EmittedStatement, values: &[Value]) -> Vec<i64> {
    let statement = client.prepare(emitted.sql()).await.unwrap();
    assert_eq!(
        statement.params().len(),
        usize::from(emitted.parameter_count())
    );
    let values: Vec<i64> = emitted
        .bind(values)
        .unwrap()
        .into_iter()
        .map(|value| match value {
            Value::Int(value) => *value,
            _ => panic!("fixture uses only bigint values"),
        })
        .collect();
    let parameters: Vec<&(dyn ToSql + Sync)> = values
        .iter()
        .map(|value| value as &(dyn ToSql + Sync))
        .collect();
    client
        .query(&statement, &parameters)
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}
