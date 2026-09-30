use darmok_types::PgSchema;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_postgres::NoTls;

/// Run only against a disposable test database. This suite deliberately fails
/// if its database is absent; the PostgreSQL CI matrix always executes it.
#[tokio::test]
async fn schema_selection_preserves_names_creation_target_and_builtin_resolution() {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (mut client, connection) = tokio_postgres::connect(&url, NoTls)
        .await
        .expect("connect to required PostgreSQL test database");
    let connection_task = tokio::spawn(connection);
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let schema = PgSchema::new(format!("Darmok_{suffix}_\"; --")).unwrap();
    let tx = client.transaction().await.unwrap();
    let max_identifier_length: String = tx
        .query_one("SHOW max_identifier_length", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(max_identifier_length, "63");

    tx.batch_execute(&format!("CREATE SCHEMA {}", schema.quoted()))
        .await
        .unwrap();
    tx.batch_execute(&schema.search_path_sql()).await.unwrap();
    tx.batch_execute("CREATE TABLE route_probe (id integer)")
        .await
        .unwrap();
    let row = tx
        .query_one(
            "SELECT current_schema(), n.nspname FROM pg_catalog.pg_class c
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
         WHERE c.oid = pg_catalog.to_regclass('route_probe')",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, &str>(0), schema.as_str());
    assert_eq!(row.get::<_, &str>(1), schema.as_str());

    // An application function with a built-in signature cannot shadow the
    // implicit pg_catalog namespace. Creation still targets the application.
    tx.batch_execute(
        "CREATE FUNCTION lower(text) RETURNS text LANGUAGE sql AS $$ SELECT 'shadowed'::text $$",
    )
    .await
    .unwrap();
    let result: String = tx
        .query_one("SELECT lower('ABC'::text)", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(result, "abc");

    // MySQL-style temporary table shadowing still works within this session.
    tx.batch_execute("CREATE TEMP TABLE route_probe (temporary_marker boolean)")
        .await
        .unwrap();
    let temporary: bool = tx
        .query_one(
            "SELECT relpersistence = 't' FROM pg_catalog.pg_class
         WHERE oid = pg_catalog.to_regclass('route_probe')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(temporary);

    tx.rollback().await.unwrap();
    drop(client);
    connection_task.await.unwrap().unwrap();
}
