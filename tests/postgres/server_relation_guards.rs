//! Private native reference ownership, not complete closure or SQL admission.
use futures_util::{FutureExt, StreamExt};
use serde_json::Value;
use std::{panic::AssertUnwindSafe, time::Duration};
use tokio_postgres::{Client, NoTls, SimpleQueryEvent, Statement, error::SqlState};

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const DEADLINE: Duration = Duration::from_secs(20);
const A: u32 = 4_200_000_001;
const B: u32 = 4_200_000_002;

async fn client() -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL").expect("native reference profile required");
    let (client, connection) = tokio::time::timeout(DEADLINE, tokio_postgres::connect(&url, NoTls))
        .await
        .unwrap()
        .unwrap();
    let driver = tokio::spawn(connection);
    sql(&client, "LOAD '$libdir/darmok_catalog_probe'")
        .await
        .unwrap();
    (client, driver)
}

async fn close(client: Client, driver: tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>) {
    drop(client);
    tokio::time::timeout(DEADLINE, driver)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

async fn sql(client: &Client, query: &str) -> Result<(), tokio_postgres::Error> {
    tokio::time::timeout(DEADLINE, client.batch_execute(query))
        .await
        .expect("bounded native reference SQL did not complete")
}

async fn command(client: &Client, value: &str) -> Result<(), tokio_postgres::Error> {
    sql(
        client,
        &format!("SET darmok_catalog_probe.command='{value}'"),
    )
    .await
}

async fn status(client: &Client) -> Value {
    tokio::time::timeout(DEADLINE, async {
        let mut events = client
            .simple_query_events("SHOW darmok_catalog_probe.relation_status")
            .unwrap();
        let mut result = None;
        let mut descriptions = 0;
        let mut completions = 0;
        let mut ready = false;
        while let Some(event) = events.next().await {
            match event.unwrap() {
                SimpleQueryEvent::RowDescription(columns) => {
                    assert_eq!(columns.len(), 1);
                    assert_eq!(columns[0].name(), "darmok_catalog_probe.relation_status");
                    assert_eq!(columns[0].type_oid(), 25);
                    assert_eq!(columns[0].format(), 0);
                    descriptions += 1;
                }
                SimpleQueryEvent::Row(row) => {
                    assert!(result.is_none());
                    result = Some(serde_json::from_str(row.get(0).unwrap()).unwrap());
                }
                SimpleQueryEvent::CommandComplete(tag) => {
                    assert_eq!(tag, "SHOW");
                    completions += 1;
                }
                SimpleQueryEvent::ReadyForQuery(_) => ready = true,
                other => panic!("unexpected native reference status event: {other:?}"),
            }
        }
        assert_eq!((descriptions, completions, ready), (1, 1, true));
        result.unwrap()
    })
    .await
    .expect("bounded native reference status did not complete")
}

struct Observer {
    locks: Statement,
    coordination: Statement,
    database: u32,
}

impl Observer {
    async fn new(client: &Client) -> Self {
        tokio::time::timeout(DEADLINE, async {
            sql(client, "SET plan_cache_mode='force_generic_plan'").await.unwrap();
            let database = client.query_one("SELECT oid FROM pg_catalog.pg_database WHERE datname=current_database()", &[]).await.unwrap().get(0);
            let locks = client.prepare("SELECT relation, COALESCE(database,0), mode, granted FROM pg_catalog.pg_locks WHERE locktype='relation' AND pid IS NOT DISTINCT FROM $1 AND relation=ANY($2) ORDER BY relation, mode").await.unwrap();
            let coordination = client.prepare("SELECT objsubid::integer, mode, granted FROM pg_catalog.pg_locks WHERE locktype='object' AND COALESCE(database,0)=0 AND classid=3079 AND objid=0 AND objsubid IN (17485,17486,17487) AND pid=$1").await.unwrap();
            for _ in 0..4 {
                client.query(&locks, &[&Some(-1_i32), &vec![A,B]]).await.unwrap();
                client.query(&coordination, &[&-1_i32]).await.unwrap();
            }
            Self { locks, coordination, database }
        }).await.expect("bounded native reference observer setup did not complete")
    }

    async fn modes(
        &self,
        client: &Client,
        pid: Option<i32>,
        oids: &[u32],
    ) -> Vec<(u32, u32, String, bool)> {
        tokio::time::timeout(DEADLINE, client.query(&self.locks, &[&pid, &oids.to_vec()]))
            .await
            .unwrap()
            .unwrap()
            .into_iter()
            .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
            .collect()
    }

    async fn no_coordination(&self, client: &Client, pid: i32) {
        assert!(
            tokio::time::timeout(DEADLINE, client.query(&self.coordination, &[&pid]))
                .await
                .unwrap()
                .unwrap()
                .is_empty()
        );
    }

    async fn wait(&self, client: &Client, pid: i32, oid: u32, granted: bool) {
        tokio::time::timeout(DEADLINE, async {
            loop {
                if self
                    .modes(client, Some(pid), &[oid])
                    .await
                    .iter()
                    .any(|row| row.3 == granted)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("expected native relation wait was not observed");
    }
}

async fn pid(client: &Client) -> i32 {
    tokio::time::timeout(
        DEADLINE,
        client.query_one("SELECT pg_catalog.pg_backend_pid()", &[]),
    )
    .await
    .unwrap()
    .unwrap()
    .get(0)
}

fn finish(outcome: Result<Result<(), tokio::time::error::Elapsed>, Box<dyn std::any::Any + Send>>) {
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("native reference case expired: {error}"),
        Err(error) => std::panic::resume_unwind(error),
    }
}

async fn finish_targets(client: &Client, gids: &[&str]) {
    for gid in gids {
        match sql(client, &format!("ROLLBACK PREPARED '{gid}'")).await {
            Ok(()) => {}
            Err(error) if error.code() == Some(&SqlState::UNDEFINED_OBJECT) => {}
            Err(error) => panic!("normal native reference target cleanup failed: {error}"),
        }
    }
}

#[tokio::test]
async fn exact_modes_borrowed_counts_copy_validation_and_snapshot_neutrality() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(60), async {
        sql(&reader, "BEGIN").await.unwrap();
        command(&reader, &format!("relation_borrow:{A}/1")).await.unwrap();
        command(&reader, &format!("relation_hold:{A}/1,{A}/2,{A}/3,1262/1")).await.unwrap();
        let state = status(&reader).await;
        assert_eq!(state["owned"], true);
        for name in ["first_snapshot", "before_snapshot", "after_snapshot"] { assert_eq!(state[name], false); }
        assert_eq!(probe.modes(&observer, Some(backend), &[A,1262]).await, vec![
            (1262,0,"AccessShareLock".to_owned(),true),
            (A,probe.database,"AccessShareLock".to_owned(),true),
            (A,probe.database,"RowExclusiveLock".to_owned(),true),
            (A,probe.database,"RowShareLock".to_owned(),true)]);
        probe.no_coordination(&observer, backend).await;
        command(&reader, "relation_release").await.unwrap();
        assert_eq!(probe.modes(&observer, Some(backend), &[A,1262]).await, vec![(A,probe.database,"AccessShareLock".to_owned(),true)]);
        command(&reader, "relation_unborrow").await.unwrap();
        assert!(probe.modes(&observer, Some(backend), &[A,1262]).await.is_empty());
        command(&reader, &format!("relation_mutated:{A}/1,{B}/3")).await.unwrap();
        assert!(probe.modes(&observer, Some(backend), &[A,B]).await.is_empty());
        for value in [format!("relation_hold:{A}/1,{A}/1"), format!("relation_hold:{A}/1,0/2"), format!("relation_hold:{A}/8"), "relation_empty".to_owned()] {
            sql(&reader, "SAVEPOINT validation").await.unwrap();
            let error = command(&reader, &value).await.unwrap_err();
            assert_eq!(error.code(), Some(&SqlState::INVALID_PARAMETER_VALUE));
            sql(&reader, "ROLLBACK TO validation; RELEASE validation").await.unwrap();
            assert!(probe.modes(&observer, Some(backend), &[A,B]).await.is_empty());
        }
        // A bounded declaration-limit check, with no native lock acquisition.
        let limit = (0..4097).map(|i| format!("{}/1", A+i)).collect::<Vec<_>>().join(",");
        sql(&reader, "SAVEPOINT validation").await.unwrap();
        let error = command(&reader, &format!("relation_hold:{limit}")).await.unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::INVALID_PARAMETER_VALUE));
        assert!(error.as_db_error().unwrap().message().contains("count must be between"));
        sql(&reader, "ROLLBACK TO validation; RELEASE validation; COMMIT; BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
        reader.query_one("SELECT 1::integer", &[]).await.unwrap();
        command(&reader, &format!("relation_scoped:{A}/2")).await.unwrap();
        let state = status(&reader).await;
        for name in ["first_snapshot", "before_snapshot", "after_snapshot"] { assert_eq!(state[name], true); }
        assert_eq!(state["owned"], false);
        sql(&reader, "COMMIT").await.unwrap();
    })).catch_unwind().await;
    sql(&reader, "ROLLBACK").await.unwrap();
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn child_ownership_stale_tokens_and_scoped_error_cleanup() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(60), async {
        sql(&reader, "BEGIN").await.unwrap();
        command(&reader, &format!("relation_borrow:{A}/1"))
            .await
            .unwrap();
        sql(&reader, "SAVEPOINT child").await.unwrap();
        command(&reader, &format!("relation_hold:{A}/1"))
            .await
            .unwrap();
        command(&reader, "relation_copy").await.unwrap();
        sql(&reader, "RELEASE child").await.unwrap();
        command(&reader, "relation_release").await.unwrap();
        command(&reader, &format!("relation_hold:{A}/1,{B}/3"))
            .await
            .unwrap();
        for value in ["relation_stale_release", "relation_stale_retain"] {
            sql(&reader, "SAVEPOINT child").await.unwrap();
            let error = command(&reader, value).await.unwrap_err();
            assert_eq!(
                error.code(),
                Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE)
            );
            sql(&reader, "ROLLBACK TO child; RELEASE child")
                .await
                .unwrap();
            assert_eq!(status(&reader).await["owned"], true);
        }
        command(&reader, "guard_hold").await.unwrap();
        for value in ["relation_release", "relation_retain"] {
            sql(&reader, "SAVEPOINT child").await.unwrap();
            let error = command(&reader, value).await.unwrap_err();
            assert_eq!(
                error.code(),
                Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE)
            );
            sql(&reader, "ROLLBACK TO child; RELEASE child")
                .await
                .unwrap();
        }
        command(&reader, "guard_release").await.unwrap();
        sql(&reader, "SAVEPOINT child").await.unwrap();
        let error = command(&reader, &format!("relation_hold:{B}/1"))
            .await
            .unwrap_err();
        assert_eq!(
            error.code(),
            Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE)
        );
        sql(&reader, "ROLLBACK TO child; RELEASE child")
            .await
            .unwrap();
        assert_eq!(status(&reader).await["owned"], true);
        command(&reader, "relation_release").await.unwrap();
        assert_eq!(
            probe.modes(&observer, Some(backend), &[A, B]).await,
            vec![(A, probe.database, "AccessShareLock".to_owned(), true)]
        );
        sql(&reader, "SAVEPOINT child").await.unwrap();
        command(&reader, &format!("relation_hold:{A}/1,{B}/3"))
            .await
            .unwrap();
        sql(&reader, "ROLLBACK TO child; RELEASE child")
            .await
            .unwrap();
        assert_eq!(status(&reader).await["owned"], false);
        command(&reader, "relation_unborrow").await.unwrap();
        assert!(
            probe
                .modes(&observer, Some(backend), &[A, B])
                .await
                .is_empty()
        );
        sql(&reader, "SAVEPOINT child").await.unwrap();
        let error = command(&reader, &format!("relation_scoped_error:{A}/1,{B}/3"))
            .await
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::INTERNAL_ERROR));
        assert!(
            probe
                .modes(&observer, Some(backend), &[A, B])
                .await
                .is_empty()
        );
        sql(&reader, "ROLLBACK TO child; RELEASE child; COMMIT; BEGIN")
            .await
            .unwrap();
        command(&reader, &format!("relation_hold:{A}/1"))
            .await
            .unwrap();
        let error = sql(&reader, "COMMIT").await.unwrap_err();
        assert_eq!(
            error.code(),
            Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE)
        );
        assert!(
            probe
                .modes(&observer, Some(backend), &[A, B])
                .await
                .is_empty()
        );
        sql(&reader, "BEGIN").await.unwrap();
        command(&reader, &format!("relation_hold:{A}/1"))
            .await
            .unwrap();
        let error = sql(&reader, "PREPARE TRANSACTION 'relation_active_forbidden'")
            .await
            .unwrap_err();
        assert_eq!(
            error.code(),
            Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE)
        );
        sql(&reader, "ROLLBACK").await.unwrap();
        assert!(
            probe
                .modes(&observer, Some(backend), &[A, B])
                .await
                .is_empty()
        );
    }))
    .catch_unwind()
    .await;
    sql(&reader, "ROLLBACK").await.unwrap();
    finish_targets(&observer, &["relation_active_forbidden"]).await;
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn retained_references_follow_native_subabort_commit_and_prepare() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let gids = ["relation_retained_commit", "relation_retained_abort"];
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(60), async {
        for ending in ["COMMIT", "ROLLBACK"] {
            sql(&reader, "BEGIN").await.unwrap();
            command(
                &reader,
                &format!("relation_scoped_retain:{A}/1,{A}/2,{B}/3"),
            )
            .await
            .unwrap();
            assert_eq!(status(&reader).await["owned"], false);
            assert_eq!(
                probe.modes(&observer, Some(backend), &[A, B]).await.len(),
                3
            );
            probe.no_coordination(&observer, backend).await;
            sql(&reader, ending).await.unwrap();
            assert!(
                probe
                    .modes(&observer, Some(backend), &[A, B])
                    .await
                    .is_empty()
            );
        }
        sql(&reader, "BEGIN").await.unwrap();
        command(&reader, &format!("relation_borrow:{A}/1"))
            .await
            .unwrap();
        sql(&reader, "SAVEPOINT child").await.unwrap();
        command(&reader, &format!("relation_scoped_retain:{A}/1,{B}/3"))
            .await
            .unwrap();
        sql(&reader, "ROLLBACK TO child; RELEASE child")
            .await
            .unwrap();
        assert_eq!(
            probe.modes(&observer, Some(backend), &[A, B]).await,
            vec![(A, probe.database, "AccessShareLock".to_owned(), true)]
        );
        sql(&reader, "SAVEPOINT child").await.unwrap();
        command(&reader, &format!("relation_scoped_retain:{A}/1,{B}/3"))
            .await
            .unwrap();
        sql(&reader, "RELEASE child").await.unwrap();
        command(&reader, "relation_unborrow").await.unwrap();
        assert_eq!(
            probe.modes(&observer, Some(backend), &[A, B]).await.len(),
            2
        );
        sql(&reader, "COMMIT").await.unwrap();
        assert!(
            probe
                .modes(&observer, Some(backend), &[A, B])
                .await
                .is_empty()
        );
        for (gid, ending) in gids.iter().zip(["COMMIT", "ROLLBACK"]) {
            sql(&reader, "BEGIN").await.unwrap();
            command(
                &reader,
                &format!("relation_scoped_retain:{A}/1,{A}/2,{B}/3"),
            )
            .await
            .unwrap();
            sql(&reader, &format!("PREPARE TRANSACTION '{gid}'"))
                .await
                .unwrap();
            assert!(
                probe
                    .modes(&observer, Some(backend), &[A, B])
                    .await
                    .is_empty()
            );
            assert_eq!(
                probe.modes(&observer, None, &[A, B]).await,
                vec![
                    (A, probe.database, "AccessShareLock".to_owned(), true),
                    (A, probe.database, "RowShareLock".to_owned(), true),
                    (B, probe.database, "RowExclusiveLock".to_owned(), true)
                ]
            );
            sql(&observer, &format!("{ending} PREPARED '{gid}'"))
                .await
                .unwrap();
            assert!(probe.modes(&observer, None, &[A, B]).await.is_empty());
        }
    }))
    .catch_unwind()
    .await;
    finish_targets(&observer, &gids).await;
    sql(&reader, "ROLLBACK").await.unwrap();
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn ordered_physical_waits_hold_no_publication_fence_and_native_two_phase_can_finish() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (writer, wd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let gids = ["relation_wait_commit", "relation_wait_abort"];
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(60), async {
        // The second request has the lower OID, distinguishing caller order
        // from an accidental sorted acquisition order.
        sql(&observer,"CREATE TABLE relation_guard_low(id integer); CREATE TABLE relation_guard_high(id integer)").await.unwrap();
        let low: u32 = observer.query_one("SELECT 'relation_guard_low'::regclass::oid",&[]).await.unwrap().get(0);
        let high: u32 = observer.query_one("SELECT 'relation_guard_high'::regclass::oid",&[]).await.unwrap().get(0);
        assert!(low < high);
        for (gid,ending) in gids.iter().zip(["COMMIT","ROLLBACK"]) {
            sql(&writer,&format!("BEGIN; ALTER TABLE relation_guard_low ADD COLUMN {gid} integer; PREPARE TRANSACTION '{gid}'")).await.unwrap();
            sql(&reader,"BEGIN").await.unwrap();
            let request = format!("relation_hold:{high}/1,{low}/1");
            let waiting = command(&reader,&request);
            tokio::pin!(waiting);
            tokio::select! {
                result = &mut waiting => panic!("physical acquisition unexpectedly completed: {result:?}"),
                () = probe.wait(&observer,backend,low,false) => {},
            }
            assert_eq!(probe.modes(&observer,Some(backend),&[high]).await,vec![(high,probe.database,"AccessShareLock".to_owned(),true)]);
            probe.no_coordination(&observer,backend).await;
            sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
            waiting.await.unwrap();
            let state = status(&reader).await;
            assert_eq!(state["owned"],true);
            for name in ["first_snapshot","before_snapshot","after_snapshot"] { assert_eq!(state[name],false); }
            command(&reader,"guard_hold").await.unwrap();
            command(&reader,"guard_release").await.unwrap();
            command(&reader,"relation_release").await.unwrap();
            sql(&reader,"COMMIT").await.unwrap();
            assert!(probe.modes(&observer,Some(backend),&[low,high]).await.is_empty());
            probe.no_coordination(&observer,backend).await;
        }
    })).catch_unwind().await;
    finish_targets(&observer, &gids).await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&writer, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        "DROP TABLE IF EXISTS relation_guard_low, relation_guard_high",
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(writer, wd).await;
    close(observer, od).await;
    finish(outcome);
}
