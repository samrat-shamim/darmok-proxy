//! Finite private-C interlock fixtures, not application execution admission.
//! The separate native probe's held token is a disclosed test-only lifetime.
use futures_util::{FutureExt, StreamExt};
use serde_json::Value;
use std::{panic::AssertUnwindSafe, time::Duration};
use tokio_postgres::{
    Client, NoTls, SimpleQueryEvent, Statement, TransactionState, error::SqlState,
};

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const DEADLINE: Duration = Duration::from_secs(20);

async fn client(
    variable: &str,
) -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var(variable).expect("a disposable native guard profile is required");
    let (client, driver) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    let driver = tokio::spawn(driver);
    client
        .batch_execute("LOAD '$libdir/darmok_catalog_probe'")
        .await
        .unwrap();
    (client, driver)
}

async fn database_client(
    database: &str,
) -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL").unwrap();
    let mut config: tokio_postgres::Config = url.parse().unwrap();
    config.dbname(database);
    let (client, driver) = config.connect(NoTls).await.unwrap();
    let driver = tokio::spawn(driver);
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
        .expect("bounded native SQL did not complete")
}

async fn pid(client: &Client) -> i32 {
    client
        .query_one("SELECT pg_catalog.pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0)
}

async fn show(client: &Client, name: &str) -> Value {
    tokio::time::timeout(DEADLINE, async {
        let query = format!("SHOW {name}");
        let mut events = client.simple_query_events(&query).unwrap();
        let mut row = None;
        let mut descriptions = 0;
        let mut completions = 0;
        let mut ready = None;
        while let Some(event) = events.next().await {
            match event.unwrap() {
                SimpleQueryEvent::RowDescription(columns) => {
                    assert_eq!(columns.len(), 1);
                    assert_eq!(columns[0].name(), name);
                    assert_eq!(columns[0].type_oid(), 25);
                    assert_eq!(columns[0].format(), 0);
                    descriptions += 1;
                }
                SimpleQueryEvent::Row(value) => {
                    assert!(row.is_none());
                    row = Some(value.get(0).unwrap().to_owned());
                }
                SimpleQueryEvent::CommandComplete(tag) => {
                    assert_eq!(tag, "SHOW");
                    completions += 1;
                }
                SimpleQueryEvent::ReadyForQuery(state) => ready = Some(state),
                other => panic!("unexpected native SHOW event: {other:?}"),
            }
        }
        assert_eq!((descriptions, completions), (1, 1));
        assert!(matches!(
            ready,
            Some(TransactionState::Idle | TransactionState::Transaction)
        ));
        serde_json::from_str(&row.unwrap()).unwrap()
    })
    .await
    .expect("bounded native SHOW did not complete")
}

async fn status(client: &Client) -> Value {
    show(client, "darmok_catalog_probe.guard_status").await
}

struct Observer {
    locks: Statement,
    prepared: Statement,
}

impl Observer {
    async fn new(client: &Client) -> Self {
        sql(client, "SET plan_cache_mode = 'force_generic_plan'")
            .await
            .unwrap();
        let locks = client.prepare("SELECT objsubid::integer, mode, granted FROM pg_catalog.pg_locks WHERE locktype='object' AND COALESCE(database,0)=0 AND classid=3079 AND objid=0 AND objsubid IN (17485,17486,17487) AND pid=$1").await.unwrap();
        let prepared = client.prepare("SELECT objsubid::integer, mode FROM pg_catalog.pg_locks WHERE locktype='object' AND COALESCE(database,0)=0 AND classid=3079 AND objid=0 AND objsubid IN (17485,17486,17487) AND pid IS NULL ORDER BY objsubid,mode").await.unwrap();
        for _ in 0..4 {
            client.query(&locks, &[&-1_i32]).await.unwrap();
            client.query(&prepared, &[]).await.unwrap();
        }
        Self { locks, prepared }
    }

    async fn modes(&self, client: &Client, backend: i32) -> Vec<(i32, String, bool)> {
        client
            .query(&self.locks, &[&backend])
            .await
            .unwrap()
            .into_iter()
            .map(|row| (row.get(0), row.get(1), row.get(2)))
            .collect()
    }

    async fn wait(&self, client: &Client, backend: i32, mode: &str, granted: bool) {
        tokio::time::timeout(DEADLINE, async {
            loop {
                if self
                    .modes(client, backend)
                    .await
                    .iter()
                    .any(|(tag, actual, held)| *tag == 17487 && actual == mode && *held == granted)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("expected native semantic mode was not observed");
    }

    async fn no_modes(&self, client: &Client, backend: i32) {
        assert!(self.modes(client, backend).await.is_empty());
    }

    async fn prepared_modes(&self, client: &Client) -> Vec<(i32, String)> {
        client
            .query(&self.prepared, &[])
            .await
            .unwrap()
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect()
    }
}

fn finish_case(
    outcome: Result<Result<(), tokio::time::error::Elapsed>, Box<dyn std::any::Any + Send>>,
) {
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("bounded native case expired after cleanup: {error}"),
        Err(error) => std::panic::resume_unwind(error),
    }
}

async fn finish_targets(finisher: &Client, gids: &[&str]) {
    for gid in gids {
        match sql(finisher, &format!("ROLLBACK PREPARED '{gid}'")).await {
            Ok(()) => {}
            Err(error) if error.code() == Some(&SqlState::UNDEFINED_OBJECT) => {}
            Err(error) => panic!("normal prepared cleanup failed for {gid}: {error}"),
        }
    }
}

#[tokio::test]
async fn private_reference_ownership_snapshots_and_error_cleanup() {
    let _serial = SERIAL.lock().await;
    let (reader, reader_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let (observer, observer_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(60), async {
        sql(&reader, "BEGIN; SET darmok_catalog_probe.command='guard_hold'").await.unwrap();
        let held = status(&reader).await;
        assert_eq!(held["owned"], true);
        for name in ["first_snapshot", "before_snapshot", "after_snapshot"] { assert_eq!(held[name], false); }
        assert_eq!(probe.modes(&observer, backend).await, vec![(17487, "ShareLock".to_owned(), true)]);
        show(&reader, "darmok_server.catalog_request_v1").await;
        assert_eq!(status(&reader).await["first_snapshot"], false);
        sql(&reader, "SET darmok_catalog_probe.command='guard_copy'; SET darmok_catalog_probe.command='guard_release'; SET darmok_catalog_probe.command='guard_hold'; SAVEPOINT child").await.unwrap();
        let error = sql(&reader, "SET darmok_catalog_probe.command='guard_stale_release'").await.unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE));
        sql(&reader, "ROLLBACK TO child; RELEASE child").await.unwrap();
        assert_eq!(status(&reader).await["owned"], true);
        sql(&reader, "SAVEPOINT child").await.unwrap();
        let error = sql(&reader, "SET darmok_catalog_probe.command='guard_hold'").await.unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE));
        sql(&reader, "ROLLBACK TO child; RELEASE child; SET darmok_catalog_probe.command='guard_release'; COMMIT").await.unwrap();
        probe.no_modes(&observer, backend).await;

        sql(&reader, "BEGIN; SAVEPOINT child; SET darmok_catalog_probe.command='guard_hold'; RELEASE child; SET darmok_catalog_probe.command='guard_release'; COMMIT").await.unwrap();
        probe.no_modes(&observer, backend).await;
        sql(&reader, "BEGIN; SAVEPOINT child; SET darmok_catalog_probe.command='guard_hold'; ROLLBACK TO child; RELEASE child").await.unwrap();
        assert_eq!(status(&reader).await["owned"], false);
        probe.no_modes(&observer, backend).await;
        sql(&reader, "SET darmok_catalog_probe.command='guard_scoped'; COMMIT").await.unwrap();
        probe.no_modes(&observer, backend).await;

        sql(&reader, "BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
        reader.query_one("SELECT 1::integer", &[]).await.unwrap();
        sql(&reader, "SET darmok_catalog_probe.command='guard_hold'").await.unwrap();
        let held = status(&reader).await;
        for name in ["first_snapshot", "before_snapshot", "after_snapshot"] { assert_eq!(held[name], true); }
        sql(&reader, "SET darmok_catalog_probe.command='guard_release'; COMMIT; BEGIN").await.unwrap();
        let error = sql(&reader, "SET darmok_catalog_probe.command='guard_scoped_error'").await.unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::INTERNAL_ERROR));
        probe.no_modes(&observer, backend).await;
        sql(&reader, "ROLLBACK; BEGIN; SET darmok_catalog_probe.command='guard_hold'").await.unwrap();
        let error = sql(&reader, "COMMIT").await.unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE));
        probe.no_modes(&observer, backend).await;
        sql(&reader, "BEGIN; SET darmok_catalog_probe.command='guard_hold'").await.unwrap();
        let error = sql(&reader, "PREPARE TRANSACTION 'guard_reader_forbidden'").await.unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE));
        sql(&reader, "ROLLBACK").await.unwrap();
        probe.no_modes(&observer, backend).await;
    })).catch_unwind().await;
    sql(&reader, "ROLLBACK").await.unwrap();
    close(reader, reader_driver).await;
    close(observer, observer_driver).await;
    finish_case(outcome);
}

#[tokio::test]
async fn publishers_wait_before_raw_fences_and_prepared_metadata_can_finish() {
    let _serial = SERIAL.lock().await;
    let (reader, reader_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let (writer, writer_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let (finisher, finisher_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let (observer, observer_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let reader_pid = pid(&reader).await;
    let writer_pid = pid(&writer).await;
    let probe = Observer::new(&observer).await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(60), async {
        sql(&finisher, "CREATE TABLE guard_rows(id integer)").await.unwrap();
        sql(&reader, "BEGIN; SET darmok_catalog_probe.command='guard_hold'").await.unwrap();
        let publishing = writer.batch_execute("BEGIN; CREATE FUNCTION guard_definition() RETURNS integer LANGUAGE SQL AS 'SELECT 7'; COMMIT");
        tokio::pin!(publishing);
        tokio::select! {
            result = &mut publishing => panic!("publisher escaped reader: {result:?}"),
            () = probe.wait(&observer, writer_pid, "RowExclusiveLock", false) => {}
        }
        assert!(probe.modes(&observer, writer_pid).await.iter().all(|(tag, _, _)| *tag == 17487));
        show(&reader, "darmok_server.catalog_request_v1").await;
        sql(&reader, "SET darmok_catalog_probe.command='guard_release'; COMMIT").await.unwrap();
        tokio::time::timeout(DEADLINE, &mut publishing).await.unwrap().unwrap();

        for (gid, ending) in [("guard_metadata_commit", "COMMIT"), ("guard_metadata_abort", "ROLLBACK")] {
            sql(&writer, &format!("BEGIN; CREATE FUNCTION guard_prepared_definition() RETURNS integer LANGUAGE SQL AS 'SELECT 9'; PREPARE TRANSACTION '{gid}'")).await.unwrap();
            let modes = probe.prepared_modes(&observer).await;
            assert_eq!(modes, vec![(17487, "AccessShareLock".to_owned()), (17487, "RowExclusiveLock".to_owned())]);
            sql(&reader, "BEGIN").await.unwrap();
            let holding = reader.batch_execute("SET darmok_catalog_probe.command='guard_hold'");
            tokio::pin!(holding);
            tokio::select! {
                result = &mut holding => panic!("reader escaped prepared metadata: {result:?}"),
                () = probe.wait(&observer, reader_pid, "ShareLock", false) => {}
            }
            assert!(probe.modes(&observer, reader_pid).await.iter().all(|(tag, _, _)| *tag == 17487));
            sql(&finisher, &format!("{ending} PREPARED '{gid}'")).await.unwrap();
            tokio::time::timeout(DEADLINE, &mut holding).await.unwrap().unwrap();
            assert_eq!(status(&reader).await["owned"], true);
            show(&reader, "darmok_server.catalog_request_v1").await;
            sql(&reader, "SET darmok_catalog_probe.command='guard_release'; COMMIT").await.unwrap();
            assert!(probe.prepared_modes(&observer).await.is_empty());
            if ending == "COMMIT" { sql(&finisher, "DROP FUNCTION guard_prepared_definition()").await.unwrap(); }
        }
        for (gid, ending) in [("guard_rows_commit", "COMMIT"), ("guard_rows_abort", "ROLLBACK")] {
            sql(&writer, &format!("BEGIN; INSERT INTO guard_rows VALUES(1); PREPARE TRANSACTION '{gid}'")).await.unwrap();
            assert_eq!(probe.prepared_modes(&observer).await, vec![(17487, "AccessShareLock".to_owned())]);
            sql(&reader, "BEGIN; SET darmok_catalog_probe.command='guard_hold'").await.unwrap();
            sql(&finisher, &format!("{ending} PREPARED '{gid}'")).await.unwrap();
            assert_eq!(status(&reader).await["owned"], true);
            sql(&reader, "SET darmok_catalog_probe.command='guard_release'; COMMIT").await.unwrap();
        }
        sql(&finisher, "DROP FUNCTION guard_definition(); DROP TABLE guard_rows").await.unwrap();
    })).catch_unwind().await;
    finish_targets(
        &finisher,
        &[
            "guard_metadata_commit",
            "guard_metadata_abort",
            "guard_rows_commit",
            "guard_rows_abort",
        ],
    )
    .await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&writer, "ROLLBACK").await.unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(finisher, finisher_driver).await;
    close(observer, observer_driver).await;
    finish_case(outcome);
}

#[tokio::test]
async fn native_coverage_checks_each_prepared_dummy_without_sql_classification() {
    let _serial = SERIAL.lock().await;
    let variable = "DARMOK_TEST_ORDERED_TWO_PHASE_DATABASE_URL";
    let (marked, marked_driver) = client(variable).await;
    let (unmarked, unmarked_driver) = client(variable).await;
    let (checker, checker_driver) = client(variable).await;
    let (finisher, finisher_driver) = client(variable).await;
    let (observer, observer_driver) = client(variable).await;
    let probe = Observer::new(&observer).await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(60), async {
        let preload: String = finisher.query_one("SELECT current_setting('shared_preload_libraries')", &[]).await.unwrap().get(0);
        assert_eq!(preload, "darmok_catalog_probe,darmok_server");
        sql(&finisher, "CREATE TABLE guard_coverage_rows(id integer)").await.unwrap();
        // One marked target must not certify a different unmarked dummy.
        sql(&marked, "BEGIN; LOCK TABLE pg_catalog.pg_class IN ROW EXCLUSIVE MODE; PREPARE TRANSACTION 'guard_covered_holder'").await.unwrap();
        for (gid, ending) in [("guard_uncovered_commit", "COMMIT"), ("guard_uncovered_abort", "ROLLBACK")] {
            sql(&unmarked, &format!("BEGIN; INSERT INTO guard_coverage_rows VALUES(2); SET darmok_catalog_probe.command='omit_prepare_coverage'; PREPARE TRANSACTION '{gid}'")).await.unwrap();
            assert_eq!(probe.prepared_modes(&observer).await, vec![(17487, "AccessShareLock".to_owned())]);
            sql(&checker, "BEGIN").await.unwrap();
            let error = sql(&checker, "SET darmok_catalog_probe.command='guard_check_prepared'").await.unwrap_err();
            assert_eq!(error.code(), Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE));
            assert!(error.as_db_error().unwrap().message().contains("uncovered native prepared transaction"));
            sql(&checker, "ROLLBACK").await.unwrap();
            sql(&finisher, &format!("{ending} PREPARED '{gid}'")).await.unwrap();
            sql(&checker, "BEGIN; SET darmok_catalog_probe.command='guard_check_prepared'; SET darmok_catalog_probe.command='guard_hold'").await.unwrap();
            let checked = status(&checker).await;
            assert_eq!(checked["owned"], true);
            assert_eq!(checked["first_snapshot"], false);
            assert_eq!(checked["before_snapshot"], false);
            assert_eq!(checked["after_snapshot"], false);
            sql(&checker, "SET darmok_catalog_probe.command='guard_release'; COMMIT").await.unwrap();
        }
        sql(&finisher, "ROLLBACK PREPARED 'guard_covered_holder'; DROP TABLE guard_coverage_rows").await.unwrap();
        assert!(probe.prepared_modes(&observer).await.is_empty());
    })).catch_unwind().await;
    finish_targets(
        &finisher,
        &[
            "guard_uncovered_commit",
            "guard_uncovered_abort",
            "guard_covered_holder",
        ],
    )
    .await;
    sql(&checker, "ROLLBACK").await.unwrap();
    sql(&unmarked, "ROLLBACK").await.unwrap();
    sql(&marked, "ROLLBACK").await.unwrap();
    close(marked, marked_driver).await;
    close(unmarked, unmarked_driver).await;
    close(checker, checker_driver).await;
    close(finisher, finisher_driver).await;
    close(observer, observer_driver).await;
    finish_case(outcome);
}

#[tokio::test]
async fn shared_drop_drains_before_intent_without_sleeping_under_a_guard() {
    let _serial = SERIAL.lock().await;
    let (reader, reader_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let (dropper, dropper_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let (observer, observer_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let dropper_pid = pid(&dropper).await;
    let probe = Observer::new(&observer).await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(60), async {
        sql(&dropper, "CREATE DATABASE guard_drop_before_intent")
            .await
            .unwrap();
        sql(
            &reader,
            "BEGIN; SET darmok_catalog_probe.command='guard_hold'",
        )
        .await
        .unwrap();
        let dropping = dropper.batch_execute("DROP DATABASE guard_drop_before_intent");
        tokio::pin!(dropping);
        tokio::select! {
            result = &mut dropping => panic!("DROP escaped existing S: {result:?}"),
            () = probe.wait(&observer, dropper_pid, "RowExclusiveLock", false) => {}
        }
        show(&reader, "darmok_server.catalog_request_v1").await;
        assert_eq!(status(&reader).await["owned"], true);
        sql(
            &reader,
            "SET darmok_catalog_probe.command='guard_release'; COMMIT",
        )
        .await
        .unwrap();
        tokio::time::timeout(DEADLINE, &mut dropping)
            .await
            .unwrap()
            .unwrap();
        probe.no_modes(&observer, dropper_pid).await;
    }))
    .catch_unwind()
    .await;
    sql(&reader, "ROLLBACK").await.unwrap();
    close(reader, reader_driver).await;
    close(dropper, dropper_driver).await;
    close(observer, observer_driver).await;
    finish_case(outcome);
}

#[tokio::test]
async fn queued_reader_yields_before_normal_temp_backend_retirement() {
    let _serial = SERIAL.lock().await;
    let (holder, holder_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let (reader, reader_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let (dropper, dropper_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let (observer, observer_driver) = client("DARMOK_TEST_DATABASE_URL").await;
    let reader_pid = pid(&reader).await;
    let dropper_pid = pid(&dropper).await;
    let probe = Observer::new(&observer).await;
    sql(&dropper, "CREATE DATABASE guard_drop_retirement")
        .await
        .unwrap();
    let (exiting, exiting_driver) = database_client("guard_drop_retirement").await;
    let exiting_pid = pid(&exiting).await;
    sql(
        &exiting,
        "CREATE TEMP TABLE guard_exit_temp(id integer); CREATE INDEX ON guard_exit_temp(id)",
    )
    .await
    .unwrap();
    let mut exiting = Some(exiting);
    let outcome = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(60), async {
        // An ordinary raw test ref pauses only the drop's short initial drain.
        sql(&holder, "BEGIN; SET darmok_catalog_probe.command='hold'")
            .await
            .unwrap();
        let dropping = dropper.batch_execute("DROP DATABASE guard_drop_retirement");
        tokio::pin!(dropping);
        tokio::select! {
            result = &mut dropping => panic!("DROP escaped raw drain: {result:?}"),
            () = probe.wait(&observer, dropper_pid, "RowExclusiveLock", true) => {}
        }
        sql(&reader, "BEGIN").await.unwrap();
        let holding = reader.batch_execute("SET darmok_catalog_probe.command='guard_hold'");
        tokio::pin!(holding);
        tokio::select! {
            result = &mut holding => panic!("new reader escaped drain RX: {result:?}"),
            () = probe.wait(&observer, reader_pid, "ShareLock", false) => {}
        }
        // Normal client close, never cancellation, a backend signal or restart.
        drop(exiting.take());
        probe
            .wait(&observer, exiting_pid, "RowExclusiveLock", false)
            .await;
        sql(
            &holder,
            "SET darmok_catalog_probe.command='release'; COMMIT",
        )
        .await
        .unwrap();
        tokio::time::timeout(DEADLINE, &mut holding)
            .await
            .unwrap()
            .unwrap();
        let yielded = status(&reader).await;
        assert_eq!(yielded["outcome"], "retry");
        assert_eq!(yielded["owned"], false);
        assert_eq!(yielded["first_snapshot"], false);
        tokio::time::timeout(DEADLINE, &mut dropping)
            .await
            .unwrap()
            .unwrap();
        probe.no_modes(&observer, reader_pid).await;
        probe.no_modes(&observer, dropper_pid).await;
        probe.no_modes(&observer, exiting_pid).await;
        sql(&reader, "COMMIT").await.unwrap();
    }))
    .catch_unwind()
    .await;
    // Release the owned raw pause before queued native operations can recover.
    sql(&holder, "ROLLBACK").await.unwrap();
    drop(exiting.take());
    sql(&reader, "ROLLBACK").await.unwrap();
    tokio::time::timeout(DEADLINE, exiting_driver)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    close(holder, holder_driver).await;
    close(reader, reader_driver).await;
    close(dropper, dropper_driver).await;
    close(observer, observer_driver).await;
    finish_case(outcome);
}
