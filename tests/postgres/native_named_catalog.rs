use darmok_catalog::{
    CatalogError, NamedNativeCatalog, NativeRelationName, RelationPersistence,
    read_native_named_relations,
};
use tokio_postgres::{Client, GenericClient, NoTls, error::SqlState, types::Type};

async fn database_client() -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL test database");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

fn name<'a>(schema_name: &'a str, relation_name: &'a str) -> NativeRelationName<'a> {
    NativeRelationName {
        schema_name,
        relation_name,
    }
}

async fn assert_missing<C: GenericClient + Sync>(
    client: &C,
    names: &[NativeRelationName<'_>],
    missing_index: usize,
) {
    match read_native_named_relations(client, names).await {
        Err(CatalogError::MissingNamedRelation {
            index,
            schema_name,
            relation_name,
        }) => {
            assert_eq!(index, missing_index);
            assert_eq!(schema_name, names[index].schema_name);
            assert_eq!(relation_name, names[index].relation_name);
        }
        other => panic!("expected complete lookup failure, received {other:?}"),
    }
}

fn first_column_type(named: &NamedNativeCatalog, index: usize) -> u32 {
    named.catalog().relations[&named.relation_oids()[index]].columns[0].declared_type_oid
}

#[tokio::test]
async fn literal_names_preserve_case_unicode_and_request_order() {
    let (mut client, connection) = database_client().await;
    let tx = client.transaction().await.unwrap();
    tx.batch_execute(
        r#"CREATE SCHEMA "Named.Catalog";
        CREATE SCHEMA "Named""Catalog";
        CREATE TABLE "Named.Catalog"."Item.Set" (id integer);
        CREATE TABLE "Named.Catalog"."item.set" (id bigint);
        CREATE TABLE "Named""Catalog"."Item""Set" (id boolean);
        CREATE TABLE "Named.Catalog"." Item.Set " (id text);
        CREATE TABLE "Named.Catalog"."分析" (id numeric(8,4));
        CREATE TABLE "Named.Catalog"."é" (id integer);
        CREATE TABLE "Named.Catalog"."é" (id bigint);
        SET LOCAL search_path TO "Named""Catalog";"#,
    )
    .await
    .unwrap();
    let requests = [
        name("Named.Catalog", "item.set"),
        name("Named\"Catalog", "Item\"Set"),
        name("Named.Catalog", "Item.Set"),
        name("Named.Catalog", "item.set"),
        name("Named.Catalog", " Item.Set "),
        name("Named.Catalog", "分析"),
        name("Named.Catalog", "é"),
        name("Named.Catalog", "e\u{301}"),
    ];
    let named = read_native_named_relations(&tx, &requests).await.unwrap();
    assert_eq!(named.relation_oids().len(), requests.len());
    assert_eq!(named.catalog().relations.len(), requests.len() - 1);
    assert_eq!(named.relation_oids()[0], named.relation_oids()[3]);
    for (index, requested) in requests.iter().enumerate() {
        let relation = &named.catalog().relations[&named.relation_oids()[index]];
        assert_eq!(relation.schema_name, requested.schema_name);
        assert_eq!(relation.name, requested.relation_name);
    }
    for (index, expected) in [
        Type::INT8,
        Type::BOOL,
        Type::INT4,
        Type::INT8,
        Type::TEXT,
        Type::NUMERIC,
        Type::INT4,
        Type::INT8,
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(first_column_type(&named, index), expected.oid());
    }
    assert_ne!(named.relation_oids()[6], named.relation_oids()[7]);
    tx.rollback().await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn missing_names_fail_the_whole_batch_without_search_path_fallback() {
    let (mut client, connection) = database_client().await;
    let tx = client.transaction().await.unwrap();
    tx.batch_execute(
        r#"CREATE SCHEMA "Named Missing";
        CREATE TABLE "Named Missing"."Present" (id integer);
        SET LOCAL search_path TO "Named Missing";"#,
    )
    .await
    .unwrap();
    let present = name("Named Missing", "Present");
    for missing in [
        name("Named Missing", "Absent"),
        name("named missing", "Present"),
        name("Named Missing", "present"),
        name("Named Missing ", "Present"),
        name("Named Missing", "\"Present\""),
        name("", "Present"),
        name("Named Missing", ""),
        name("missing_schema", "Present"),
        name("$user", "Present"),
    ] {
        assert_missing(&tx, &[present, missing, present], 1).await;
        assert_missing(&tx, &[missing, present], 0).await;
        assert_missing(&tx, &[present, present, missing], 2).await;
    }
    // Errors identify the first missing request, even when other objects exist.
    assert_missing(
        &tx,
        &[present, name("missing_schema", "first"), name("", "second")],
        1,
    )
    .await;
    assert_eq!(
        read_native_named_relations(&tx, &[present])
            .await
            .unwrap()
            .catalog()
            .relations
            .len(),
        1
    );
    tx.rollback().await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn overlong_names_cannot_resolve_to_truncated_catalog_names() {
    let (mut client, connection) = database_client().await;
    let tx = client.transaction().await.unwrap();
    let ascii = "a".repeat(63);
    let unicode = "界".repeat(21);
    assert_eq!(ascii.len(), 63);
    assert_eq!(unicode.len(), 63);
    tx.batch_execute(&format!(
        "CREATE SCHEMA \"{ascii}\";
         CREATE SCHEMA \"{unicode}\";
         CREATE TABLE \"{ascii}\".\"{ascii}\" (id integer);
         CREATE TABLE \"{unicode}\".\"{unicode}\" (id bigint);"
    ))
    .await
    .unwrap();
    let requests = [name(&ascii, &ascii), name(&unicode, &unicode)];
    let named = read_native_named_relations(&tx, &requests).await.unwrap();
    assert_eq!(first_column_type(&named, 0), Type::INT4.oid());
    assert_eq!(first_column_type(&named, 1), Type::INT8.oid());
    for (schema, relation) in [(&ascii, &ascii), (&unicode, &unicode)] {
        let long_schema = format!("{schema}x");
        let long_relation = format!("{relation}界");
        assert_missing(&tx, &[name(&long_schema, relation)], 0).await;
        assert_missing(&tx, &[name(schema, &long_relation)], 0).await;
        assert_missing(&tx, &[name(&long_schema, &long_relation)], 0).await;
    }
    tx.rollback().await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn temporary_lookup_uses_the_actual_namespace_and_preserves_empty_relations() {
    let (mut client, connection) = database_client().await;
    let tx = client.transaction().await.unwrap();
    tx.batch_execute(
        r#"CREATE SCHEMA "Named Permanent";
        CREATE TABLE "Named Permanent".shared (id bigint);
        CREATE TEMP TABLE shared (id integer);
        CREATE TEMP TABLE empty ();
        SET LOCAL search_path TO "Named Permanent";"#,
    )
    .await
    .unwrap();
    let temporary_schema: String = tx
        .query_one(
            "SELECT nspname::text FROM pg_catalog.pg_namespace
             WHERE oid = pg_catalog.pg_my_temp_schema()",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let requests = [
        name("Named Permanent", "shared"),
        name(&temporary_schema, "shared"),
        name(&temporary_schema, "empty"),
    ];
    let named = read_native_named_relations(&tx, &requests).await.unwrap();
    assert_eq!(first_column_type(&named, 0), Type::INT8.oid());
    assert_eq!(first_column_type(&named, 1), Type::INT4.oid());
    assert_ne!(named.relation_oids()[0], named.relation_oids()[1]);
    assert_eq!(
        named.catalog().relations[&named.relation_oids()[0]].persistence,
        RelationPersistence::Permanent
    );
    assert_eq!(
        named.catalog().relations[&named.relation_oids()[1]].persistence,
        RelationPersistence::Temporary
    );
    assert!(
        named.catalog().relations[&named.relation_oids()[2]]
            .columns
            .is_empty()
    );
    assert_missing(&tx, &[name("pg_temp", "shared")], 0).await;
    assert_missing(&tx, &[name("Named Permanent", "empty")], 0).await;
    let described = tx.prepare("SELECT id FROM shared").await.unwrap();
    assert_eq!(
        described.columns()[0].table_oid(),
        Some(named.relation_oids()[1])
    );
    tx.rollback().await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn empty_input_does_no_io_and_backend_failures_are_not_missing_objects() {
    let (mut client, connection) = database_client().await;
    let tx = client.transaction().await.unwrap();
    assert!(tx.batch_execute("SELECT 1 / 0").await.is_err());
    assert_eq!(
        read_native_named_relations(&tx, &[]).await.unwrap(),
        NamedNativeCatalog::default()
    );
    assert!(matches!(
        read_native_named_relations(&tx, &[name("pg_catalog", "pg_class")]).await,
        Err(CatalogError::Postgres(error))
            if error.code() == Some(&SqlState::IN_FAILED_SQL_TRANSACTION)
    ));
    tx.rollback().await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn fresh_named_reads_observe_external_and_rolled_back_local_ddl() {
    let (mut client, connection) = database_client().await;
    let (external, external_connection) = database_client().await;
    let pid: i32 = client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let schema = format!("darmok_named_catalog_{pid}");
    external
        .batch_execute(&format!(
            "CREATE SCHEMA {schema}; CREATE TABLE {schema}.current (original integer);"
        ))
        .await
        .unwrap();
    let current = name(&schema, "current");
    let renamed = name(&schema, "Renamed");
    let initial = read_native_named_relations(&client, &[current])
        .await
        .unwrap();
    let original_oid = initial.relation_oids()[0];
    external
        .batch_execute(&format!(
            "ALTER TABLE {schema}.current RENAME TO \"Renamed\";
             ALTER TABLE {schema}.\"Renamed\" ALTER COLUMN original TYPE bigint;
             ALTER TABLE {schema}.\"Renamed\" RENAME COLUMN original TO changed;"
        ))
        .await
        .unwrap();
    assert_missing(&client, &[current], 0).await;
    let changed = read_native_named_relations(&client, &[renamed])
        .await
        .unwrap();
    assert_eq!(changed.relation_oids(), &[original_oid]);
    assert_eq!(first_column_type(&changed, 0), Type::INT8.oid());
    assert_eq!(
        changed.catalog().relations[&original_oid].columns[0].name,
        "changed"
    );
    let tx = client.transaction().await.unwrap();
    tx.batch_execute(&format!(
        "ALTER TABLE {schema}.\"Renamed\" RENAME TO local;
         ALTER TABLE {schema}.local ADD COLUMN flag boolean;"
    ))
    .await
    .unwrap();
    assert_missing(&tx, &[renamed], 0).await;
    let local = read_native_named_relations(&tx, &[name(&schema, "local")])
        .await
        .unwrap();
    assert_eq!(local.relation_oids(), &[original_oid]);
    assert_eq!(local.catalog().relations[&original_oid].columns.len(), 2);
    tx.rollback().await.unwrap();
    assert_eq!(
        read_native_named_relations(&client, &[renamed])
            .await
            .unwrap(),
        changed
    );
    external
        .batch_execute(&format!(
            "DROP TABLE {schema}.\"Renamed\";
             CREATE TABLE {schema}.\"Renamed\" (replacement text);"
        ))
        .await
        .unwrap();
    let replacement = read_native_named_relations(&client, &[renamed])
        .await
        .unwrap();
    assert_ne!(replacement.relation_oids()[0], original_oid);
    assert_eq!(first_column_type(&replacement, 0), Type::TEXT.oid());
    assert_eq!(
        replacement.catalog().relations[&replacement.relation_oids()[0]].columns[0].name,
        "replacement"
    );
    external
        .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
    assert_missing(&client, &[renamed], 0).await;
    drop(client);
    drop(external);
    connection.await.unwrap().unwrap();
    external_connection.await.unwrap().unwrap();
}

#[tokio::test]
async fn named_resolution_and_facts_honor_the_same_repeatable_read_snapshot() {
    let (mut client, connection) = database_client().await;
    let (external, external_connection) = database_client().await;
    let pid: i32 = client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let schema = format!("darmok_named_snapshot_{pid}");
    external
        .batch_execute(&format!(
            "CREATE SCHEMA {schema}; CREATE TABLE {schema}.snapshot (original integer);"
        ))
        .await
        .unwrap();
    let original = name(&schema, "snapshot");
    let renamed = name(&schema, "renamed");
    let tx = client
        .build_transaction()
        .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
        .start()
        .await
        .unwrap();
    let before = read_native_named_relations(&tx, &[original]).await.unwrap();
    external
        .batch_execute(&format!(
            "ALTER TABLE {schema}.snapshot RENAME TO renamed;
             ALTER TABLE {schema}.renamed ALTER COLUMN original TYPE bigint;"
        ))
        .await
        .unwrap();
    assert_eq!(
        read_native_named_relations(&tx, &[original]).await.unwrap(),
        before
    );
    assert_missing(&tx, &[renamed], 0).await;
    tx.commit().await.unwrap();
    assert_missing(&client, &[original], 0).await;
    let after = read_native_named_relations(&client, &[renamed])
        .await
        .unwrap();
    assert_eq!(after.relation_oids(), before.relation_oids());
    assert_eq!(first_column_type(&before, 0), Type::INT4.oid());
    assert_eq!(first_column_type(&after, 0), Type::INT8.oid());
    external
        .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
    drop(client);
    drop(external);
    connection.await.unwrap().unwrap();
    external_connection.await.unwrap().unwrap();
}
