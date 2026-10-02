//! Ordinary native namespace changes; no admitted SQL or execution lease.

use darmok_execute::NativeStatementUtc;
use darmok_types::Value;
use tokio_postgres::{Client, NoTls};

async fn client() -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

async fn effective_path(client: &Client) -> Vec<String> {
    client
        .query_one(
            "SELECT pg_catalog.current_schemas(true)::pg_catalog.text[]",
            &[],
        )
        .await
        .unwrap()
        .get(0)
}

async fn renamed_schema(path_contains_source: bool, create_temporary_schema: bool) {
    let (owner, owner_driver) = client().await;
    let (ddl, ddl_driver) = client().await;
    let pid: i32 = owner
        .query_one("SELECT pg_catalog.pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let source = format!("darmok_lookup_{pid}");
    let moved = format!("darmok_lookup_moved_{pid}");
    ddl.batch_execute(&format!(
        "CREATE SCHEMA {source}; \
         CREATE TABLE {source}.items(n pg_catalog.int4); \
         INSERT INTO {source}.items VALUES(1)"
    ))
    .await
    .unwrap();
    let path = if path_contains_source {
        format!("{source}, pg_catalog")
    } else {
        "pg_catalog".to_owned()
    };
    owner
        .batch_execute(&format!(
            "SET search_path = {path}; \
             BEGIN ISOLATION LEVEL READ COMMITTED READ WRITE NOT DEFERRABLE"
        ))
        .await
        .unwrap();
    let path_before = effective_path(&owner).await;
    let statement = owner
        .prepare(&format!(
            "SELECT tableoid::pg_catalog.oid AS actual_relation, n FROM {source}.items"
        ))
        .await
        .unwrap();
    let description = NativeStatementUtc::new(&statement).unwrap();
    let original_oid = statement.columns()[1].table_oid().unwrap();
    let before = description
        .decode_row(&owner.query_one(&statement, &[]).await.unwrap())
        .unwrap();
    ddl.batch_execute(&format!(
        "ALTER SCHEMA {source} RENAME TO {moved}; \
         CREATE SCHEMA {source}; \
         CREATE TABLE {source}.items(n pg_catalog.int4); \
         INSERT INTO {source}.items VALUES(2)"
    ))
    .await
    .unwrap();
    let replacement_oid: u32 = ddl
        .query_one(
            "SELECT pg_catalog.to_regclass($1::pg_catalog.text)::pg_catalog.oid",
            &[&format!("{source}.items")],
        )
        .await
        .unwrap()
        .get(0);
    if create_temporary_schema {
        owner
            .batch_execute("CREATE TEMP TABLE lookup_scratch(n pg_catalog.int4)")
            .await
            .unwrap();
    }
    let path_after = effective_path(&owner).await;
    let row = owner.query_one(&statement, &[]).await.unwrap();
    let cached_row_oid = row.columns()[1].table_oid().unwrap();
    let after = description.decode_row(&row).unwrap();
    owner.batch_execute("ROLLBACK").await.unwrap();
    ddl.batch_execute(&format!(
        "DROP SCHEMA {source} CASCADE; DROP SCHEMA {moved} CASCADE"
    ))
    .await
    .unwrap();
    drop(statement);
    drop(owner);
    drop(ddl);
    owner_driver.await.unwrap().unwrap();
    ddl_driver.await.unwrap().unwrap();

    assert_eq!(
        before,
        [Value::UInt(u64::from(original_oid)), Value::Int(1)]
    );
    assert_ne!(original_oid, replacement_oid);
    assert_eq!(cached_row_oid, original_oid);
    let rebinds = path_contains_source || create_temporary_schema;
    let expected_oid = if rebinds {
        replacement_oid
    } else {
        original_oid
    };
    let expected_value = if rebinds { 2 } else { 1 };
    assert_eq!(
        after,
        [
            Value::UInt(u64::from(expected_oid)),
            Value::Int(expected_value)
        ],
        "path {path_before:?} -> {path_after:?}; cached origin {cached_row_oid}"
    );
    assert_eq!(path_before != path_after, rebinds);
    if !path_contains_source {
        assert_eq!(path_before, ["pg_catalog"]);
        assert_eq!(path_after.last().unwrap(), "pg_catalog");
    }
}

#[tokio::test]
async fn source_schema_recreation_rebinds_same_shape_statement_with_cached_origin() {
    renamed_schema(true, false).await;
}

#[tokio::test]
async fn explicit_native_lookup_path_retains_bound_relation_across_schema_rename() {
    renamed_schema(false, false).await;
}

#[tokio::test]
async fn implicit_temporary_namespace_can_rebind_even_with_fixed_path_text() {
    renamed_schema(false, true).await;
}
