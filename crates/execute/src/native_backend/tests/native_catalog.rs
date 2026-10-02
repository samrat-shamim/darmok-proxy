//! Ordinary exclusive-owner discovery and completion fixtures.
use super::*;
use darmok_catalog::NativeRelationName;

#[tokio::test]
#[ignore = "required PostgreSQL 17/18 native-owner catalog discovery fixture"]
async fn private_owner_discovery_preserves_first_data_views_in_both_scope_kinds() {
    let mut backend = connect_backend().await;
    let writer = connect_backend().await;
    let native_pid: i32 = client(&backend)
        .query_one("SELECT pg_catalog.pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let table = format!("owned_discovery_{native_pid}");
    setup(
        &writer,
        &format!(
            "CREATE TABLE public.{table}(value integer); INSERT INTO public.{table} VALUES(1)"
        ),
    )
    .await;
    let names = [NativeRelationName {
        schema_name: "public",
        relation_name: &table,
    }];
    for isolation in [
        NativeIsolation::RepeatableRead,
        NativeIsolation::Serializable,
    ] {
        let spec = NativeTransactionSpec {
            isolation,
            access: NativeTransactionAccess::ReadOnly,
        };
        for savepoint in [false, true] {
            setup(&writer, &format!("UPDATE public.{table} SET value=1")).await;
            let mut scope = if savepoint {
                assert_eq!(
                    backend.begin(spec).await.unwrap().ready_state(),
                    TransactionState::Transaction
                );
                backend.savepoint_scope().await.unwrap()
            } else {
                backend.transaction_scope(spec).await.unwrap()
            };
            assert!(scope.discover_catalog(&[]).await.unwrap().is_none());
            let observation = scope.discover_catalog(&names).await.unwrap().unwrap();
            assert_eq!(observation.named_catalog().relation_oids().len(), 1);
            setup(&writer, &format!("UPDATE public.{table} SET value=2")).await;
            let value: i32 = client(scope.backend)
                .query_one(&format!("SELECT value FROM public.{table}"), &[])
                .await
                .unwrap()
                .get(0);
            assert_eq!(
                value, 2,
                "owned {isolation:?} savepoint={savepoint} fixed its data view early"
            );
            let finished = scope.finish().await.unwrap();
            assert_eq!(
                finished.control(),
                if savepoint {
                    NativeControl::Release
                } else {
                    NativeControl::Commit
                }
            );
            if savepoint {
                assert_eq!(
                    backend.commit().await.unwrap().ready_state(),
                    TransactionState::Idle
                );
            }
            assert_eq!(
                backend.state(),
                NativeBackendState::Ready(TransactionState::Idle)
            );
        }
    }
    setup(&writer, &format!("DROP TABLE public.{table}")).await;
    assert_eq!(
        backend.dispose().await.unwrap().previous_state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
    assert_eq!(
        writer.dispose().await.unwrap().previous_state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
}

#[tokio::test]
#[ignore = "required PostgreSQL 17/18 native-owner catalog discovery fixture"]
async fn complete_native_catalog_error_can_recover_without_freezing_the_parent_view() {
    let mut backend = connect_backend().await;
    let writer = connect_backend().await;
    let native_pid: i32 = client(&backend)
        .query_one("SELECT pg_catalog.pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let table = format!("owned_discovery_error_{native_pid}");
    setup(
        &writer,
        &format!(
            "CREATE TABLE public.{table}(value integer); INSERT INTO public.{table} VALUES(1)"
        ),
    )
    .await;
    let begun = backend
        .begin(NativeTransactionSpec {
            isolation: NativeIsolation::RepeatableRead,
            access: NativeTransactionAccess::ReadOnly,
        })
        .await
        .unwrap();
    assert_eq!(begun.ready_state(), TransactionState::Transaction);
    let mut scope = backend.savepoint_scope().await.unwrap();
    let error = scope
        .discover_catalog(&[NativeRelationName {
            schema_name: "public",
            relation_name: "missing_owned_discovery",
        }])
        .await
        .unwrap_err();
    let NativeCatalogError::Completion(failure) = error else {
        panic!("expected observed native error")
    };
    assert_eq!(
        failure.backend_error().unwrap().code(),
        Some(&SqlState::UNDEFINED_TABLE)
    );
    assert_eq!(
        failure.ready_state(),
        Some(TransactionState::FailedTransaction)
    );
    assert!(failure.mismatch().is_none() && failure.stream_error().is_none());
    let recovered = scope.recover(NativeRecovery::Statement).await.unwrap();
    assert_eq!(recovered.control(), NativeControl::RecoverSavepoint);
    assert_eq!(recovered.ready_state(), TransactionState::Transaction);
    setup(&writer, &format!("UPDATE public.{table} SET value=2")).await;
    assert_eq!(
        client(&backend)
            .query_one(&format!("SELECT value FROM public.{table}"), &[])
            .await
            .unwrap()
            .get::<_, i32>(0),
        2
    );
    assert_eq!(
        backend.commit().await.unwrap().ready_state(),
        TransactionState::Idle
    );
    setup(&writer, &format!("DROP TABLE public.{table}")).await;
    assert_eq!(
        backend.dispose().await.unwrap().previous_state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
    assert_eq!(
        writer.dispose().await.unwrap().previous_state(),
        NativeBackendState::Ready(TransactionState::Idle)
    );
}

#[tokio::test]
#[ignore = "required PostgreSQL 17/18 native-owner catalog discovery fixture"]
async fn placeholder_echo_is_disposal_only_and_scope_controls_cannot_repair_it() {
    let url = std::env::var("DARMOK_TEST_UNPRELOADED_DATABASE_URL")
        .expect("required native profile without module preloading");
    let config: Config = url.parse().unwrap();
    for recovery in [false, true] {
        let mut backend = NativeBackend::connect(&config, NoTls).await.unwrap();
        let mut scope = backend
            .transaction_scope(READ_COMMITTED_WRITE)
            .await
            .unwrap();
        let error = scope
            .discover_catalog(&[NativeRelationName {
                schema_name: "public",
                relation_name: "unused",
            }])
            .await
            .unwrap_err();
        assert!(matches!(error, NativeCatalogError::Observation(_)));
        let error = if recovery {
            scope
                .recover(NativeRecovery::Transaction)
                .await
                .unwrap_err()
        } else {
            scope.finish().await.unwrap_err()
        };
        assert!(matches!(
            error,
            NativeBackendError::InvalidState {
                state: NativeBackendState::Uncertain,
                ..
            }
        ));
        assert_eq!(backend.state(), NativeBackendState::Uncertain);
        assert_eq!(
            backend.dispose().await.unwrap().previous_state(),
            NativeBackendState::Uncertain
        );
    }
}
