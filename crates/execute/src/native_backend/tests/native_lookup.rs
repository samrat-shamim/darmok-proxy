//! Normal namespace settings and fully qualified fixture SQL, not admission.

use super::*;
use darmok_catalog::{NativeRelationName, read_native_named_relations};

async fn lookup_context(backend: &NativeBackend) -> (String, Vec<String>) {
    let row = client(backend)
        .query_one(
            "SELECT pg_catalog.current_setting('search_path'), \
             pg_catalog.current_schemas(true)::pg_catalog.text[]",
            &[],
        )
        .await
        .unwrap();
    (row.get(0), row.get(1))
}

async fn assert_initial_context(backend: &NativeBackend) {
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
    assert_eq!(
        lookup_context(backend).await,
        ("pg_catalog".to_owned(), vec!["pg_catalog".to_owned()])
    );
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_lookup_initialization_replaces_startup_path_and_controls_preserve_it() {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must point to a disposable PostgreSQL database");
    for options in [
        None,
        Some("-c search_path=public,pg_catalog"),
        Some("-c search_path=pg_temp,public"),
    ] {
        let mut config = url.parse::<Config>().unwrap();
        if let Some(options) = options {
            config.options(options);
        }
        let mut backend = NativeBackend::connect(&config, NoTls).await.unwrap();
        assert_initial_context(&backend).await;
        for (spec, _) in transaction_cases() {
            let scope = backend.transaction_scope(spec).await.unwrap();
            assert_eq!(
                lookup_context(scope.backend).await,
                ("pg_catalog".to_owned(), vec!["pg_catalog".to_owned()])
            );
            let _ = scope.finish().await.unwrap();
            assert_initial_context(&backend).await;
        }
        let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
        let scope = backend.savepoint_scope().await.unwrap();
        let _ = scope.recover(NativeRecovery::Statement).await.unwrap();
        assert_eq!(lookup_context(&backend).await.0, "pg_catalog");
        let scope = backend.savepoint_scope().await.unwrap();
        let _ = scope.finish().await.unwrap();
        let _ = backend.rollback().await.unwrap();
        assert_initial_context(&backend).await;
        let _ = backend.dispose().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_lookup_qualified_application_names_and_implicit_temporary_namespace() {
    let mut backend = connect_backend().await;
    assert_initial_context(&backend).await;
    let pid: i32 = client(&backend)
        .query_one("SELECT pg_catalog.pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let schema_name = format!("Darmok.Lookup {pid}");
    let quoted_schema = format!("\"{schema_name}\"");
    setup(
        &backend,
        &format!(
            "CREATE SCHEMA {quoted_schema}; \
             CREATE TABLE {quoted_schema}.\"Items.Set\"(n pg_catalog.int4); \
             INSERT INTO {quoted_schema}.\"Items.Set\" VALUES(7)"
        ),
    )
    .await;
    let scope = backend
        .transaction_scope(READ_COMMITTED_WRITE)
        .await
        .unwrap();
    let names = [NativeRelationName {
        schema_name: &schema_name,
        relation_name: "Items.Set",
    }];
    let catalog = read_native_named_relations(client(scope.backend), &names)
        .await
        .unwrap();
    let statement = client(scope.backend)
        .prepare(&format!("SELECT n FROM {quoted_schema}.\"Items.Set\""))
        .await
        .unwrap();
    assert_eq!(
        statement.columns()[0].table_oid(),
        Some(catalog.relation_oids()[0])
    );
    let row = client(scope.backend)
        .query_one(&statement, &[])
        .await
        .unwrap();
    assert_eq!(row.get::<_, i32>(0), 7);
    drop(statement);
    let _ = scope.finish().await.unwrap();
    assert_initial_context(&backend).await;
    setup(
        &backend,
        "CREATE TEMP TABLE lookup_values(n pg_catalog.int4); INSERT INTO lookup_values VALUES(9)",
    )
    .await;
    let (configured_path, effective_path) = lookup_context(&backend).await;
    assert_eq!(configured_path, "pg_catalog");
    assert_eq!(effective_path.len(), 2);
    assert!(effective_path[0].starts_with("pg_temp_"));
    assert_eq!(effective_path[1], "pg_catalog");
    let row = client(&backend)
        .query_one("SELECT n FROM pg_temp.lookup_values", &[])
        .await
        .unwrap();
    assert_eq!(row.get::<_, i32>(0), 9);
    let count: i64 = client(&backend)
        .query_one("SELECT pg_catalog.count(*) FROM lookup_values", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1);
    setup(&backend, &format!("DROP SCHEMA {quoted_schema} CASCADE")).await;
    let absent: bool = client(&backend)
        .query_one(
            "SELECT pg_catalog.to_regnamespace($1::pg_catalog.text) IS NULL",
            &[&schema_name],
        )
        .await
        .unwrap()
        .get(0);
    assert!(absent, "fixture cleanup was not confirmed");
    let _ = backend.dispose().await.unwrap();
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_lookup_partial_initialization_is_uncertain_despite_idle_readiness() {
    let mut backend = connect_backend().await;
    let error = backend
        .control(NativeControl::Initialize, "ROLLBACK")
        .await
        .unwrap_err();
    let failure = control_failure(&error);
    assert_eq!(failure.control(), NativeControl::Initialize);
    assert_eq!(failure.matched_tags(), 1);
    assert_eq!(failure.ready_state(), Some(TransactionState::Idle));
    assert_eq!(
        failure.mismatch(),
        Some(&NativeControlMismatch::MissingTags {
            expected: 2,
            matched: 1
        })
    );
    assert_eq!(backend.state(), NativeBackendState::Uncertain);
    assert!(backend.begin(READ_COMMITTED_WRITE).await.is_err());
    let disposal = backend.dispose().await.unwrap();
    assert_eq!(disposal.previous_state(), NativeBackendState::Uncertain);
}
