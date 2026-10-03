//! Required ordinary initialization and typed helper fixtures, not frontend
//! admission, credential policy, concurrent DDL or catalog lease certification.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use super::*;
use crate::{
    NATIVE_SCHEMA_VERSION, NativeDatabaseAction, NativeDatabaseError, NativeDatabaseFailure,
};

struct Fixture {
    admin: NativeBackend,
    backend: NativeBackend,
    database: String,
    config: Config,
}

impl Fixture {
    async fn new() -> Self {
        Self::new_at("DARMOK_TEST_DATABASE_URL").await
    }

    async fn new_at(variable: &str) -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let database = format!("darmok_database_{}_{}", std::process::id(), serial);
        let url = std::env::var(variable).expect("required disposable native profile");
        let mut config = url.parse::<Config>().unwrap();
        let admin = NativeBackend::connect(&config, NoTls).await.unwrap();
        setup(
            &admin,
            &format!("CREATE DATABASE {database} TEMPLATE template0 ENCODING 'UTF8'"),
        )
        .await;
        config.dbname(&database);
        let backend = NativeBackend::connect(&config, NoTls).await.unwrap();
        Self {
            admin,
            backend,
            database,
            config,
        }
    }

    async fn close(self) {
        let _ = self.backend.dispose().await.unwrap();
        setup(&self.admin, &format!("DROP DATABASE {}", self.database)).await;
        let _ = self.admin.dispose().await.unwrap();
    }
}

async fn snapshot(backend: &NativeBackend) -> String {
    // Includes OIDs and row/catalog xmins: idempotent operations must preserve
    // artifacts, rather than replacing definitions with equivalent objects.
    client(backend).query_one(
        "SELECT pg_catalog.jsonb_build_object(\
            'namespaces', (SELECT pg_catalog.jsonb_agg((pg_catalog.to_jsonb(n) - 'nspacl' - 'nspowner') || pg_catalog.jsonb_build_object('xmin', n.xmin::pg_catalog.text) ORDER BY n.oid) FROM pg_catalog.pg_namespace n WHERE nspname IN ('darmok', 'darmok_server')), \
            'server_extension', (SELECT (pg_catalog.to_jsonb(e) - 'extowner') || pg_catalog.jsonb_build_object('xmin', e.xmin::pg_catalog.text) FROM pg_catalog.pg_extension e WHERE extname = 'darmok_server'), \
            'relations', (SELECT pg_catalog.jsonb_agg(pg_catalog.to_jsonb(c) - 'relacl' - 'relowner' ORDER BY c.oid) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'darmok'), \
            'functions', (SELECT pg_catalog.jsonb_agg(pg_catalog.jsonb_build_object('definition', pg_catalog.pg_get_functiondef(p.oid), 'oid', p.oid, 'xmin', p.xmin::pg_catalog.text) ORDER BY p.oid) FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname = 'darmok')\
        )::pg_catalog.text", &[]
    ).await.unwrap().get(0)
}

async fn metadata(backend: &NativeBackend) -> String {
    client(backend).query_one(
        "SELECT pg_catalog.jsonb_agg(pg_catalog.jsonb_build_object('row', pg_catalog.to_jsonb(i), 'xmin', i.xmin::pg_catalog.text))::pg_catalog.text FROM darmok.installation i", &[]
    ).await.unwrap().get(0)
}

fn assert_idle(backend: &NativeBackend) {
    assert_eq!(
        backend.state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
}

fn assert_database_error(
    failure: &NativeDatabaseFailure,
    action: NativeDatabaseAction,
    message: &str,
) {
    assert_eq!(failure.action(), action);
    let NativeDatabaseError::Completion(original) = failure.original() else {
        panic!("expected observed setup completion error: {failure:?}");
    };
    assert_eq!(original.matched_events(), 1);
    assert_eq!(
        original.ready_state(),
        Some(TransactionState::FailedTransaction)
    );
    assert!(original.mismatch().is_none());
    assert!(original.stream_error().is_none());
    let error = original.backend_error().unwrap();
    assert_eq!(error.code().unwrap().code(), "P0001");
    assert_eq!(error.as_db_error().unwrap().message(), message);
    let cleanup = failure.cleanup().unwrap().as_ref().unwrap();
    assert_eq!(cleanup.control(), NativeControl::Rollback);
    assert_eq!(cleanup.ready_state(), TransactionState::Idle);
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_two_ordinary_owners_initialize_and_verify_the_same_database() {
    let mut fixture = Fixture::new().await;
    let url = std::env::var("DARMOK_TEST_DATABASE_URL").unwrap();
    let mut config = url.parse::<Config>().unwrap();
    config.dbname(&fixture.database);
    let mut second = NativeBackend::connect(&config, NoTls).await.unwrap();
    let (first_receipt, second_receipt) = tokio::join!(
        fixture.backend.initialize_database(),
        second.initialize_database()
    );
    assert_eq!(first_receipt.unwrap().schema_version(), 1);
    assert_eq!(second_receipt.unwrap().schema_version(), 1);
    let before = snapshot(&fixture.backend).await;
    let data = metadata(&fixture.backend).await;
    assert_eq!(snapshot(&second).await, before);
    assert_eq!(metadata(&second).await, data);
    let (first_receipt, second_receipt) = tokio::join!(
        fixture.backend.initialize_database(),
        second.verify_database()
    );
    assert_eq!(
        first_receipt.unwrap().action(),
        NativeDatabaseAction::Initialize
    );
    assert_eq!(
        second_receipt.unwrap().action(),
        NativeDatabaseAction::Verify
    );
    assert_idle(&fixture.backend);
    assert_idle(&second);
    assert_eq!(snapshot(&fixture.backend).await, before);
    assert_eq!(metadata(&second).await, data);
    let _ = second.dispose().await.unwrap();
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_fresh_repeat_and_readonly_verification_preserve_artifacts() {
    let mut fixture = Fixture::new().await;
    setup(&fixture.backend, "SET default_transaction_read_only = on").await;
    let receipt = fixture.backend.initialize_database().await.unwrap();
    assert_eq!(receipt.schema_version(), NATIVE_SCHEMA_VERSION);
    assert_eq!(receipt.action(), NativeDatabaseAction::Initialize);
    assert_eq!(receipt.server_extension_version(), "1.0");
    assert_eq!(receipt.completion().control(), NativeControl::Commit);
    assert_eq!(receipt.completion().ready_state(), TransactionState::Idle);
    let before = snapshot(&fixture.backend).await;
    let data = metadata(&fixture.backend).await;
    for initialize in [false, true, false] {
        let receipt = if initialize {
            fixture.backend.initialize_database().await
        } else {
            fixture.backend.verify_database().await
        }
        .unwrap();
        assert_eq!(receipt.schema_version(), 1);
        assert_eq!(receipt.server_extension_version(), "1.0");
        assert_eq!(receipt.completion().control(), NativeControl::Commit);
        assert_eq!(
            receipt.action(),
            if initialize {
                NativeDatabaseAction::Initialize
            } else {
                NativeDatabaseAction::Verify
            }
        );
        assert_idle(&fixture.backend);
        assert_eq!(snapshot(&fixture.backend).await, before);
        assert_eq!(metadata(&fixture.backend).await, data);
    }
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_missing_verification_and_partial_installations_fail_without_repair() {
    let mut fixture = Fixture::new().await;
    let before = snapshot(&fixture.backend).await;
    let failure = fixture.backend.verify_database().await.unwrap_err();
    assert_database_error(
        &failure,
        NativeDatabaseAction::Verify,
        "Darmok schema is missing; explicit initialization is required",
    );
    assert_idle(&fixture.backend);
    assert_eq!(snapshot(&fixture.backend).await, before);
    setup(&fixture.backend, "CREATE SCHEMA darmok").await;
    for sql in [
        "",
        "CREATE VIEW darmok.installation AS SELECT true AS singleton, 1 AS format_version, 'unexpected'::pg_catalog.text AS profile",
    ] {
        setup(&fixture.backend, sql).await;
        let before = snapshot(&fixture.backend).await;
        for initialize in [true, false] {
            let failure = if initialize {
                fixture.backend.initialize_database().await
            } else {
                fixture.backend.verify_database().await
            }
            .unwrap_err();
            assert_database_error(
                &failure,
                if initialize {
                    NativeDatabaseAction::Initialize
                } else {
                    NativeDatabaseAction::Verify
                },
                "Darmok installation table definition does not match",
            );
            assert_idle(&fixture.backend);
            assert_eq!(snapshot(&fixture.backend).await, before);
        }
    }
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_version_and_metadata_mismatches_preserve_data_and_recover() {
    let mut fixture = Fixture::new().await;
    for mutation in [
        "UPDATE darmok.installation SET format_version = 0",
        "UPDATE darmok.installation SET format_version = 2",
        "UPDATE darmok.installation SET profile = 'other-profile'",
        "DELETE FROM darmok.installation",
    ] {
        let _ = fixture.backend.initialize_database().await.unwrap();
        setup(&fixture.backend, mutation).await;
        let before = snapshot(&fixture.backend).await;
        let data: Option<String> = client(&fixture.backend).query_one("SELECT pg_catalog.jsonb_agg(pg_catalog.to_jsonb(i))::pg_catalog.text FROM darmok.installation i", &[]).await.unwrap().get(0);
        for initialize in [true, false] {
            let failure = if initialize {
                fixture.backend.initialize_database().await
            } else {
                fixture.backend.verify_database().await
            }
            .unwrap_err();
            assert_database_error(
                &failure,
                if initialize {
                    NativeDatabaseAction::Initialize
                } else {
                    NativeDatabaseAction::Verify
                },
                "Darmok installation version or metadata does not match",
            );
            assert_idle(&fixture.backend);
            assert_eq!(snapshot(&fixture.backend).await, before);
            let after: Option<String> = client(&fixture.backend).query_one("SELECT pg_catalog.jsonb_agg(pg_catalog.to_jsonb(i))::pg_catalog.text FROM darmok.installation i", &[]).await.unwrap().get(0);
            assert_eq!(after, data);
        }
        setup(&fixture.backend, "DROP SCHEMA darmok CASCADE").await;
    }
    let _ = fixture.backend.initialize_database().await.unwrap();
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_definition_and_inventory_mismatches_fail_without_replacing_objects() {
    let mut fixture = Fixture::new().await;
    for (mutation, message) in [
        (
            "ALTER FUNCTION darmok.substring_utf8(pg_catalog.text, pg_catalog.int8) CALLED ON NULL INPUT",
            "Darmok compatibility function definition does not match",
        ),
        (
            "ALTER FUNCTION darmok.substring_bytes(pg_catalog.bytea, pg_catalog.int8) VOLATILE",
            "Darmok compatibility function definition does not match",
        ),
        (
            "ALTER FUNCTION darmok.substring_utf8(pg_catalog.text, pg_catalog.int8, pg_catalog.int8) PARALLEL UNSAFE",
            "Darmok compatibility function definition does not match",
        ),
        (
            "DROP FUNCTION darmok.substring_utf8(pg_catalog.text, pg_catalog.int8); CREATE FUNCTION darmok.substring_utf8(pg_catalog.text, pg_catalog.int8) RETURNS pg_catalog.text LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS 'SELECT $1'",
            "Darmok compatibility function definition does not match",
        ),
        (
            "CREATE FUNCTION darmok.substring_utf8(pg_catalog.text) RETURNS pg_catalog.text LANGUAGE sql AS 'SELECT $1'",
            "Darmok schema contains unexpected objects",
        ),
        (
            "ALTER TABLE darmok.installation ALTER COLUMN profile DROP NOT NULL",
            "Darmok installation table definition does not match",
        ),
        (
            "ALTER TABLE darmok.installation ADD COLUMN extra pg_catalog.int4",
            "Darmok installation table definition does not match",
        ),
        (
            "ALTER TABLE darmok.installation ALTER COLUMN format_version SET DEFAULT 1",
            "Darmok installation table definition does not match",
        ),
        (
            "ALTER TABLE darmok.installation DROP CONSTRAINT installation_singleton_check",
            "Darmok installation constraints do not match",
        ),
        (
            "ALTER TABLE darmok.installation ADD CONSTRAINT extra CHECK (format_version > 0)",
            "Darmok installation constraints do not match",
        ),
        (
            "CREATE INDEX extra ON darmok.installation(profile)",
            "Darmok relation or type inventory does not match",
        ),
        (
            "CREATE COLLATION darmok.extra FROM pg_catalog.\"C\"",
            "Darmok schema contains unexpected objects",
        ),
    ] {
        let _ = fixture.backend.initialize_database().await.unwrap();
        setup(&fixture.backend, mutation).await;
        let before = snapshot(&fixture.backend).await;
        for initialize in [true, false] {
            let failure = if initialize {
                fixture.backend.initialize_database().await
            } else {
                fixture.backend.verify_database().await
            }
            .unwrap_err();
            assert_database_error(
                &failure,
                if initialize {
                    NativeDatabaseAction::Initialize
                } else {
                    NativeDatabaseAction::Verify
                },
                message,
            );
            assert_idle(&fixture.backend);
            assert_eq!(snapshot(&fixture.backend).await, before);
        }
        setup(&fixture.backend, "DROP SCHEMA darmok CASCADE").await;
    }
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_rejects_existing_transaction_without_submitting_or_changing_outer_work() {
    let mut fixture = Fixture::new().await;
    let _ = fixture.backend.begin(READ_COMMITTED_WRITE).await.unwrap();
    setup(&fixture.backend, "CREATE TABLE public.application_values(n pg_catalog.int4); INSERT INTO public.application_values VALUES(7)").await;
    let before = snapshot(&fixture.backend).await;
    for initialize in [true, false] {
        let failure = if initialize {
            fixture.backend.initialize_database().await
        } else {
            fixture.backend.verify_database().await
        }
        .unwrap_err();
        assert!(failure.cleanup().is_none());
        assert!(
            matches!(failure.original(), NativeDatabaseError::Owner(NativeBackendError::InvalidState { operation, state }) if *operation == if initialize { NativeBackendOperation::InitializeDatabase } else { NativeBackendOperation::VerifyDatabase } && *state == NativeBackendState::Ready(TransactionState::Transaction))
        );
        assert_eq!(
            fixture.backend.state(),
            NativeBackendState::Ready(TransactionState::Transaction)
        );
        assert_eq!(snapshot(&fixture.backend).await, before);
        let n: i32 = client(&fixture.backend)
            .query_one("SELECT n FROM public.application_values", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(n, 7);
    }
    let _ = fixture.backend.rollback().await.unwrap();
    let _ = fixture.backend.initialize_database().await.unwrap();
    fixture.close().await;
}

fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(text, 16).unwrap()
        })
        .collect()
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_builtin_types_remain_qualified_with_an_implicit_temporary_namespace() {
    let mut fixture = Fixture::new().await;
    setup(&fixture.backend, "CREATE TEMP TABLE type_namespace_anchor(n pg_catalog.int4); CREATE DOMAIN pg_temp.text AS pg_catalog.int4; CREATE DOMAIN pg_temp.oid AS pg_catalog.text; CREATE DOMAIN pg_temp.bool AS pg_catalog.text; CREATE DOMAIN pg_temp.int4 AS pg_catalog.text").await;
    let row = client(&fixture.backend).query_one(
        "SELECT pg_catalog.current_setting('search_path'), 'text'::pg_catalog.regtype::pg_catalog.oid, 'pg_catalog.text'::pg_catalog.regtype::pg_catalog.oid", &[]
    ).await.unwrap();
    assert_eq!(row.get::<_, String>(0), "pg_catalog");
    assert_ne!(row.get::<_, u32>(1), row.get::<_, u32>(2));
    let _ = fixture.backend.initialize_database().await.unwrap();
    let before = snapshot(&fixture.backend).await;
    let data = metadata(&fixture.backend).await;
    let _ = fixture.backend.verify_database().await.unwrap();
    let _ = fixture.backend.initialize_database().await.unwrap();
    assert_eq!(snapshot(&fixture.backend).await, before);
    assert_eq!(metadata(&fixture.backend).await, data);
    let row = client(&fixture.backend).query_one(
        "SELECT darmok.substring_utf8('Aé😀Z'::pg_catalog.text, -2::pg_catalog.int8, 1::pg_catalog.int8), darmok.substring_bytes('\\x00ff80'::pg_catalog.bytea, -2::pg_catalog.int8, 1::pg_catalog.int8)", &[]
    ).await.unwrap();
    assert_eq!(row.get::<_, String>(0), "😀");
    assert_eq!(row.get::<_, Vec<u8>>(1), vec![255]);
    assert_idle(&fixture.backend);
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_manifest_error_after_creation_rolls_back_all_artifacts() {
    let mut fixture = Fixture::new().await;
    let before = snapshot(&fixture.backend).await;
    // This private fixture changes only the inserted metadata version. The
    // manifest creates compatibility artifacts, then their validation fails.
    // Caller-provided manifests are not exposed by the production API.
    let manifest = super::super::native_database::DATABASE_SQL.replace(
        "(true, 1, 'substring-signed64-utf8-bytes-v1')",
        "(true, 2, 'substring-signed64-utf8-bytes-v1')",
    );
    let error = fixture
        .backend
        .check_database_installation(NativeDatabaseAction::Initialize, &manifest)
        .await
        .unwrap_err();
    let NativeDatabaseError::Completion(failure) = error else {
        panic!("expected observed postcreation manifest failure");
    };
    assert_eq!(failure.matched_events(), 1);
    assert_eq!(
        failure.ready_state(),
        Some(TransactionState::FailedTransaction)
    );
    assert_eq!(
        failure
            .backend_error()
            .unwrap()
            .as_db_error()
            .unwrap()
            .message(),
        "Darmok installation version or metadata does not match"
    );
    assert_eq!(
        fixture.backend.state(),
        NativeBackendState::Ready(TransactionState::FailedTransaction)
    );
    let _ = fixture.backend.rollback().await.unwrap();
    assert_eq!(snapshot(&fixture.backend).await, before);
    let _ = fixture.backend.initialize_database().await.unwrap();
    let _ = fixture.backend.verify_database().await.unwrap();
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_typed_substrings_match_stock_mysql84_corpus() {
    let mut fixture = Fixture::new().await;
    let _ = fixture.backend.initialize_database().await.unwrap();
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../fixtures/native-substring-mysql84.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 810);
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(case["warnings"].as_u64(), Some(0));
        let start = case["start"].as_i64();
        let length = case["length"].as_i64();
        let three = case["arity"].as_u64().unwrap() == 3;
        if case["kind"] == "utf8" {
            let input = case["input"].as_str();
            let sql = if three {
                "SELECT darmok.substring_utf8($1, $2, $3)"
            } else {
                "SELECT darmok.substring_utf8($1, $2)"
            };
            let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = if three {
                vec![&input, &start, &length]
            } else {
                vec![&input, &start]
            };
            let result: Option<String> = client(&fixture.backend)
                .query_one(sql, &params)
                .await
                .unwrap()
                .get(0);
            assert_eq!(
                result.as_deref(),
                case["expected"].as_str(),
                "text corpus case {index}"
            );
        } else {
            let input = case["input"].as_str().map(bytes);
            let expected = case["expected"].as_str().map(bytes);
            let sql = if three {
                "SELECT darmok.substring_bytes($1, $2, $3)"
            } else {
                "SELECT darmok.substring_bytes($1, $2)"
            };
            let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = if three {
                vec![&input, &start, &length]
            } else {
                vec![&input, &start]
            };
            let result: Option<Vec<u8>> = client(&fixture.backend)
                .query_one(sql, &params)
                .await
                .unwrap()
                .get(0);
            assert_eq!(result, expected, "byte corpus case {index}");
        }
    }
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_advisory_lock_is_held_after_checked_show_until_commit() {
    use super::super::native_database::DATABASE_SQL;
    let mut fixture = Fixture::new().await;
    let mut second = NativeBackend::connect(&fixture.config, NoTls)
        .await
        .unwrap();
    for action in [
        NativeDatabaseAction::Initialize,
        NativeDatabaseAction::Verify,
    ] {
        let manifest = if action == NativeDatabaseAction::Initialize {
            DATABASE_SQL.to_owned()
        } else {
            DATABASE_SQL.replace(
                "allow_create constant pg_catalog.bool := true;",
                "allow_create constant pg_catalog.bool := false;",
            )
        };
        // Staging returns only after the exact SHOW and strict empty response.
        fixture
            .backend
            .check_database_installation(action, &manifest)
            .await
            .unwrap();
        assert_eq!(
            fixture.backend.state(),
            NativeBackendState::Ready(TransactionState::Transaction)
        );
        let _ = second.begin(READ_COMMITTED_WRITE).await.unwrap();
        let locked: bool = client(&second)
            .query_one(
                "SELECT pg_catalog.pg_try_advisory_xact_lock(4922526098346491905::pg_catalog.int8)",
                &[],
            )
            .await
            .unwrap()
            .get(0);
        assert!(
            !locked,
            "setup lock disappeared before COMMIT for {action:?}"
        );
        if action == NativeDatabaseAction::Initialize {
            let absent: i64 = client(&second).query_one(
                "SELECT pg_catalog.count(*) FROM pg_catalog.pg_namespace WHERE nspname IN ('darmok','darmok_server')", &[]
            ).await.unwrap().get(0);
            assert_eq!(absent, 0, "uncommitted setup leaked to the other owner");
        }
        let committed = fixture.backend.commit().await.unwrap();
        assert_eq!(committed.control(), NativeControl::Commit);
        assert_eq!(committed.ready_state(), TransactionState::Idle);
        let locked: bool = client(&second)
            .query_one(
                "SELECT pg_catalog.pg_try_advisory_xact_lock(4922526098346491905::pg_catalog.int8)",
                &[],
            )
            .await
            .unwrap()
            .get(0);
        assert!(locked, "completed setup retained its transaction lock");
        let installed = client(&second).query_one(
            "SELECT (SELECT pg_catalog.count(*) FROM pg_catalog.pg_namespace WHERE nspname IN ('darmok','darmok_server')), (SELECT extversion FROM pg_catalog.pg_extension WHERE extname='darmok_server'), (SELECT format_version FROM darmok.installation)", &[]
        ).await.unwrap();
        assert_eq!(installed.get::<_, i64>(0), 2);
        assert_eq!(installed.get::<_, String>(1), "1.0");
        assert_eq!(installed.get::<_, i32>(2), 1);
        let _ = second.commit().await.unwrap();
    }
    let _ = second.dispose().await.unwrap();
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_unpreloaded_placeholder_rolls_back_both_components() {
    let mut fixture = Fixture::new_at("DARMOK_TEST_UNPRELOADED_DATABASE_URL").await;
    let observer = NativeBackend::connect(&fixture.config, NoTls)
        .await
        .unwrap();
    let before = snapshot(&observer).await;
    let failure = fixture.backend.initialize_database().await.unwrap_err();
    assert_eq!(failure.action(), NativeDatabaseAction::Initialize);
    assert!(matches!(
        failure.original(),
        NativeDatabaseError::Observation(_)
    ));
    let rollback = failure.cleanup().unwrap().as_ref().unwrap();
    assert_eq!(rollback.control(), NativeControl::Rollback);
    assert_eq!(rollback.ready_state(), TransactionState::Idle);
    assert_idle(&fixture.backend);
    assert_eq!(snapshot(&observer).await, before);
    let row = client(&observer).query_one(
        "SELECT (SELECT pg_catalog.count(*) FROM pg_catalog.pg_namespace WHERE nspname IN ('darmok','darmok_server')), (SELECT pg_catalog.count(*) FROM pg_catalog.pg_extension WHERE extname='darmok_server')", &[]
    ).await.unwrap();
    assert_eq!(row.get::<_, i64>(0), 0);
    assert_eq!(row.get::<_, i64>(1), 0);
    let _ = observer.dispose().await.unwrap();
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_nonmember_dependencies_are_allowed_and_extra_members_rejected() {
    let mut fixture = Fixture::new().await;
    let _ = fixture.backend.initialize_database().await.unwrap();
    setup(&fixture.backend, "CREATE FUNCTION public.application_helper() RETURNS pg_catalog.int4 LANGUAGE sql AS 'SELECT 7'; ALTER FUNCTION public.application_helper() DEPENDS ON EXTENSION darmok_server").await;
    let normal: String = client(&fixture.backend).query_one(
        "SELECT deptype::pg_catalog.text FROM pg_catalog.pg_depend WHERE classid='pg_catalog.pg_proc'::pg_catalog.regclass AND objid='public.application_helper()'::pg_catalog.regprocedure AND refclassid='pg_catalog.pg_extension'::pg_catalog.regclass", &[]
    ).await.unwrap().get(0);
    assert_eq!(normal, "x");
    let before = snapshot(&fixture.backend).await;
    let _ = fixture.backend.initialize_database().await.unwrap();
    let _ = fixture.backend.verify_database().await.unwrap();
    assert_eq!(snapshot(&fixture.backend).await, before);
    for (mutation, undo, message) in [
        (
            "ALTER EXTENSION darmok_server ADD FUNCTION public.application_helper()",
            "ALTER EXTENSION darmok_server DROP FUNCTION public.application_helper()",
            "Darmok server extension contains unexpected members",
        ),
        (
            "CREATE FUNCTION darmok_server.extra() RETURNS pg_catalog.int4 LANGUAGE sql AS 'SELECT 1'",
            "DROP FUNCTION darmok_server.extra()",
            "Darmok server namespace contains unexpected objects",
        ),
        (
            "CREATE COLLATION darmok_server.extra FROM pg_catalog.\"C\"",
            "DROP COLLATION darmok_server.extra",
            "Darmok server namespace contains unexpected objects",
        ),
    ] {
        setup(&fixture.backend, mutation).await;
        let unchanged = snapshot(&fixture.backend).await;
        for action in [
            NativeDatabaseAction::Initialize,
            NativeDatabaseAction::Verify,
        ] {
            let failure = match action {
                NativeDatabaseAction::Initialize => fixture.backend.initialize_database().await,
                NativeDatabaseAction::Verify => fixture.backend.verify_database().await,
            }
            .unwrap_err();
            assert_database_error(&failure, action, message);
            assert_idle(&fixture.backend);
            assert_eq!(snapshot(&fixture.backend).await, unchanged);
        }
        setup(&fixture.backend, undo).await;
        let _ = fixture.backend.verify_database().await.unwrap();
    }
    fixture.close().await;
}

#[tokio::test]
#[ignore = "required by PostgreSQL 17/18 native-owner CI"]
async fn native_database_missing_extension_and_partial_server_namespace_are_not_repaired() {
    let mut fixture = Fixture::new().await;
    // A pre-existing server namespace must survive an initialization rollback
    // without committing the newly created compatibility schema.
    setup(&fixture.backend, "CREATE SCHEMA darmok_server").await;
    let partial = snapshot(&fixture.backend).await;
    let failure = fixture.backend.initialize_database().await.unwrap_err();
    assert_database_error(
        &failure,
        NativeDatabaseAction::Initialize,
        "Darmok server namespace exists without its extension",
    );
    assert_eq!(snapshot(&fixture.backend).await, partial);
    setup(&fixture.backend, "DROP SCHEMA darmok_server").await;
    let _ = fixture.backend.initialize_database().await.unwrap();
    setup(&fixture.backend, "DROP EXTENSION darmok_server").await;
    let partial = snapshot(&fixture.backend).await;
    let failure = fixture.backend.verify_database().await.unwrap_err();
    assert_database_error(
        &failure,
        NativeDatabaseAction::Verify,
        "Darmok server extension is missing; explicit initialization is required",
    );
    let failure = fixture.backend.initialize_database().await.unwrap_err();
    assert_database_error(
        &failure,
        NativeDatabaseAction::Initialize,
        "Darmok server namespace exists without its extension",
    );
    assert_eq!(snapshot(&fixture.backend).await, partial);
    setup(&fixture.backend, "DROP SCHEMA darmok_server").await;
    let _ = fixture.backend.initialize_database().await.unwrap();
    let _ = fixture.backend.verify_database().await.unwrap();
    fixture.close().await;
}
