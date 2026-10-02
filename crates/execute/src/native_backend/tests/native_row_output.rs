// Private, ordinary native execution fixtures. They demonstrate output checked
// while an existing owner scope remains open, not semantic SQL admission or a
// public executor. No interruption, lock-retention or resource stress probes.
use super::*;
use crate::{NativeResultError, NativeResultUtc, NativeRowFormat, NativeStatementUtc};
use bytes::{Bytes, BytesMut};
use darmok_protocol::ColumnDefinition;
use darmok_types::mysql_const::{charset, field_type};
use futures_util::{StreamExt, stream::FusedStream};
use tokio_postgres::QueryEvent;

fn column(kind: u8) -> ColumnDefinition {
    ColumnDefinition {
        catalog: Bytes::from_static(b"def"),
        schema: Bytes::new(),
        table: Bytes::new(),
        org_table: Bytes::new(),
        name: Bytes::from_static(b"fixture"),
        org_name: Bytes::new(),
        character_set: charset::BINARY,
        column_length: if kind == field_type::NEWDECIMAL {
            66
        } else {
            11
        },
        column_type: kind,
        flags: 0,
        decimals: 0,
    }
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn later_returning_output_failure_recovers_writes_and_preserves_outer_work() {
    for outer in [false, true] {
        for decode_error in [false, true] {
            for format in [NativeRowFormat::Text, NativeRowFormat::Binary] {
                let mut backend = connect_backend().await;
                setup(
                    &backend,
                    "CREATE TEMP TABLE owned_values(n integer); INSERT INTO owned_values VALUES(1)",
                )
                .await;
                if outer {
                    let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
                    setup(&backend, "INSERT INTO owned_values VALUES(2)").await;
                }
                let scope = if outer {
                    backend.savepoint_scope().await.unwrap()
                } else {
                    backend
                        .transaction_scope(READ_COMMITTED_WRITE)
                        .await
                        .unwrap()
                };
                let expected_state = NativeBackendState::Scoped(scope.boundary());
                let sql = if decode_error {
                    "WITH inserted AS (INSERT INTO owned_values VALUES(3),(1000) RETURNING n)
                     SELECT n, CASE WHEN n=1000 THEN 'NaN'::numeric ELSE 1::numeric END
                     FROM inserted ORDER BY n"
                } else {
                    "WITH inserted AS (INSERT INTO owned_values VALUES(3),(1000) RETURNING n)
                     SELECT n, n FROM inserted ORDER BY n"
                };
                let statement = client(scope.backend).prepare(sql).await.unwrap();
                let columns = [
                    column(field_type::LONG),
                    column(if decode_error {
                        field_type::NEWDECIMAL
                    } else {
                        field_type::TINY
                    }),
                ];
                let description = NativeStatementUtc::new(&statement).unwrap();
                let bindings = description.bind(&[]).unwrap();
                let portal = client(scope.backend)
                    .bind_described_builtin(bindings.statement(), bindings.parameters())
                    .await
                    .unwrap();
                let checked = description.check_portal(&portal).unwrap();
                let result = NativeResultUtc::from_portal(checked, &columns).unwrap();
                let mut events = client(scope.backend)
                    .query_portal_events(&portal, 0)
                    .unwrap();
                let mut output = BytesMut::from(b"prefix".as_slice());
                let mut rows = 0;
                let mut tag = None;
                let mut ready = None;
                while let Some(event) = events.next().await {
                    assert!(ready.is_none());
                    assert_eq!(scope.backend.state(), expected_state);
                    match event.unwrap() {
                        QueryEvent::Row(row) => {
                            rows += 1;
                            assert!(std::ptr::eq(row.columns(), portal.columns().unwrap()));
                            if rows == 1 {
                                assert_eq!(row.get::<_, i32>(0), 3);
                                result.encode_row(&row, format, &mut output).unwrap();
                            } else {
                                assert_eq!(rows, 2);
                                assert_eq!(row.get::<_, i32>(0), 1000);
                                let valid_output = output.clone();
                                let error =
                                    result.encode_row(&row, format, &mut output).unwrap_err();
                                if decode_error {
                                    assert!(matches!(error, NativeResultError::Statement(_)));
                                } else {
                                    assert!(matches!(
                                        error,
                                        NativeResultError::Encoding { column: 1, .. }
                                    ));
                                }
                                assert_eq!(output, valid_output);
                            }
                        }
                        QueryEvent::CommandComplete(value) => {
                            assert!(tag.is_none());
                            tag = Some(value);
                        }
                        QueryEvent::ReadyForQuery(state) => ready = Some(state),
                        other => panic!("unexpected {other:?}"),
                    }
                }
                assert!(events.is_terminated());
                assert_eq!(rows, 2);
                assert_eq!(tag.as_deref(), Some("SELECT 2"));
                assert_eq!(ready, Some(TransactionState::Transaction));
                assert_eq!(scope.backend.state(), expected_state);
                assert_eq!(
                    values(scope.backend).await,
                    if outer {
                        vec![1, 2, 3, 1000]
                    } else {
                        vec![1, 3, 1000]
                    }
                );
                drop(events);
                drop(portal);
                drop(statement);
                let completion = scope.recover(NativeRecovery::Statement).await.unwrap();
                assert_eq!(
                    completion.control(),
                    if outer {
                        NativeControl::RecoverSavepoint
                    } else {
                        NativeControl::Rollback
                    }
                );
                assert_eq!(
                    completion.ready_state(),
                    if outer {
                        TransactionState::Transaction
                    } else {
                        TransactionState::Idle
                    }
                );
                assert_eq!(
                    values(&backend).await,
                    if outer { vec![1, 2] } else { vec![1] }
                );
                let scope = if outer {
                    backend.savepoint_scope().await.unwrap()
                } else {
                    backend
                        .transaction_scope(READ_COMMITTED_WRITE)
                        .await
                        .unwrap()
                };
                setup(scope.backend, "INSERT INTO owned_values VALUES(4)").await;
                let _ = scope.finish().await.unwrap();
                if outer {
                    let _ = backend.commit().await.unwrap();
                }
                assert_eq!(
                    values(&backend).await,
                    if outer { vec![1, 2, 4] } else { vec![1, 4] }
                );
                let _ = backend.dispose().await.unwrap();
            }
        }
    }
}

#[tokio::test]
#[ignore = "required by the PostgreSQL 17/18 native-owner CI step"]
async fn returning_rows_are_encoded_before_confirmed_scope_finish() {
    for outer in [false, true] {
        for format in [NativeRowFormat::Text, NativeRowFormat::Binary] {
            let mut backend = connect_backend().await;
            setup(
                &backend,
                "CREATE TEMP TABLE owned_values(n integer); INSERT INTO owned_values VALUES(1)",
            )
            .await;
            if outer {
                let _ = backend.begin(READ_COMMITTED_WRITE).await.unwrap();
                setup(&backend, "INSERT INTO owned_values VALUES(2)").await;
            }
            let scope = if outer {
                backend.savepoint_scope().await.unwrap()
            } else {
                backend
                    .transaction_scope(READ_COMMITTED_WRITE)
                    .await
                    .unwrap()
            };
            let expected_state = NativeBackendState::Scoped(scope.boundary());
            let statement = client(scope.backend)
                .prepare(
                    "WITH inserted AS (INSERT INTO owned_values VALUES(3),(4) RETURNING n)
                 SELECT n FROM inserted ORDER BY n",
                )
                .await
                .unwrap();
            let columns = [column(field_type::LONG)];
            let description = NativeStatementUtc::new(&statement).unwrap();
            let bindings = description.bind(&[]).unwrap();
            let portal = client(scope.backend)
                .bind_described_builtin(bindings.statement(), bindings.parameters())
                .await
                .unwrap();
            let checked = description.check_portal(&portal).unwrap();
            let result = NativeResultUtc::from_portal(checked, &columns).unwrap();
            let mut events = client(scope.backend)
                .query_portal_events(&portal, 0)
                .unwrap();
            let mut rows = 0;
            let mut tag = None;
            let mut ready = None;
            let mut output = BytesMut::new();
            while let Some(event) = events.next().await {
                assert!(ready.is_none());
                assert_eq!(scope.backend.state(), expected_state);
                match event.unwrap() {
                    QueryEvent::Row(row) => {
                        rows += 1;
                        assert!(std::ptr::eq(row.columns(), portal.columns().unwrap()));
                        result.encode_row(&row, format, &mut output).unwrap();
                    }
                    QueryEvent::CommandComplete(value) => {
                        assert!(tag.is_none());
                        tag = Some(value);
                    }
                    QueryEvent::ReadyForQuery(state) => ready = Some(state),
                    other => panic!("unexpected {other:?}"),
                }
            }
            assert!(events.is_terminated());
            assert_eq!(rows, 2);
            assert_eq!(tag.as_deref(), Some("SELECT 2"));
            assert_eq!(ready, Some(TransactionState::Transaction));
            assert_eq!(
                output.as_ref(),
                if format == NativeRowFormat::Text {
                    &[1, b'3', 1, b'4'][..]
                } else {
                    &[0, 0, 3, 0, 0, 0, 0, 0, 4, 0, 0, 0][..]
                }
            );
            drop(events);
            drop(portal);
            drop(statement);
            let completion = scope.finish().await.unwrap();
            assert_eq!(
                completion.control(),
                if outer {
                    NativeControl::Release
                } else {
                    NativeControl::Commit
                }
            );
            assert_eq!(
                completion.ready_state(),
                if outer {
                    TransactionState::Transaction
                } else {
                    TransactionState::Idle
                }
            );
            if outer {
                let _ = backend.commit().await.unwrap();
            }
            assert_eq!(
                values(&backend).await,
                if outer {
                    vec![1, 2, 3, 4]
                } else {
                    vec![1, 3, 4]
                }
            );
            let _ = backend.dispose().await.unwrap();
        }
    }
}
