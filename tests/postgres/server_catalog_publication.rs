//! Ordinary server-module correctness fixtures. No frontend SQL support is
//! inferred from these native observations. A missing module is a test failure.
use darmok_catalog::{NativeCatalogStamp, decode_catalog_observation};
use futures_util::{FutureExt, StreamExt};
use std::time::Duration;
use tokio_postgres::{Client, NoTls, error::SqlState};
use tokio_postgres::{CommandEvent, SimpleQueryEvent, TransactionState};

#[path = "support/native_frames.rs"]
mod native_frames;

static TEST_SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// Database removal can require a durable checkpoint. Its filesystem sync cost
// is independent of the module's lock/admission and immediate-read deadlines.
const DATABASE_DROP_COMPLETION: Duration = Duration::from_secs(120);

const FENCE_LOCKS: &str = "SELECT mode, granted FROM pg_catalog.pg_locks
    WHERE locktype = 'object' AND COALESCE(database, 0) = 0
      AND classid = 'pg_catalog.pg_extension'::pg_catalog.regclass
      AND objid = 0 AND objsubid = 17485 AND pid = $1";
const COORDINATION_LOCKS: &str = "SELECT objsubid::integer, mode, granted FROM pg_catalog.pg_locks
    WHERE locktype = 'object' AND COALESCE(database, 0) = 0
      AND classid = 3079 AND objid = 0 AND objsubid IN (17485, 17486) AND pid = $1";

async fn client() -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("DARMOK_TEST_DATABASE_URL must name a disposable module-enabled database");
    let (client, driver) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    let driver = tokio::spawn(driver);
    let version: String = client
        .query_one(
            "SELECT extversion FROM pg_catalog.pg_extension WHERE extname = 'darmok_server'",
            &[],
        )
        .await
        .expect("CREATE EXTENSION darmok_server is required")
        .get(0);
    assert_eq!(version, "1.0");
    client
        .batch_execute("LOAD '$libdir/darmok_catalog_probe'")
        .await
        .expect("separate native test probe module is required");
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
    (client, tokio::spawn(driver))
}

async fn close(client: Client, driver: tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>) {
    drop(client);
    tokio::time::timeout(Duration::from_secs(20), driver)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

// ErrorResponse precedes native abort cleanup. Observe ReadyForQuery before
// returning either outcome to callers that inspect native locks afterward.
async fn complete_drop(client: &Client, sql: &str) -> Result<(), tokio_postgres::Error> {
    let mut events = client.command_events(sql)?;
    let mut error = None;
    let mut commands = 0_usize;
    let mut ready = false;
    while let Some(event) = events.next().await {
        match event? {
            CommandEvent::CommandComplete(tag) => {
                assert_eq!(tag, "DROP DATABASE");
                commands += 1;
            }
            CommandEvent::BackendError(value) => {
                assert!(error.is_none());
                error = Some(value);
            }
            CommandEvent::ReadyForQuery(state) => {
                assert!(!ready);
                assert!(matches!(state, TransactionState::Idle));
                ready = true;
            }
            other => panic!("unexpected database-drop event: {other:?}"),
        }
    }
    assert!(ready, "database-drop completion was not observed");
    assert_eq!(commands, usize::from(error.is_none()));
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

// Epochs come from a distinct unheld backend, never combined with probe state.
// Staged wait fixtures start this receive operation before releasing their
// native blockers; the caller bounds completion after the release.
async fn receive_observation(client: &Client) -> NativeCatalogStamp {
    let mut events = client
        .simple_query_events("SHOW darmok_server.catalog_request_v1")
        .unwrap();
    let mut text = None;
    let mut descriptions = 0;
    let mut tags = 0;
    let mut ready = None;
    while let Some(event) = events.next().await {
        match event.unwrap() {
            SimpleQueryEvent::RowDescription(columns) => {
                assert_eq!(columns.len(), 1);
                assert_eq!(columns[0].name(), "darmok_server.catalog_request_v1");
                assert_eq!(columns[0].type_oid(), 25);
                assert_eq!(columns[0].format(), 0);
                descriptions += 1;
            }
            SimpleQueryEvent::Row(row) => {
                assert!(text.is_none());
                assert_eq!(row.len(), 1);
                text = Some(row.get(0).unwrap().to_owned());
            }
            SimpleQueryEvent::CommandComplete(tag) => {
                assert_eq!(tag, "SHOW");
                tags += 1;
            }
            SimpleQueryEvent::ReadyForQuery(state) => ready = Some(state),
            other => panic!("unexpected observation event: {other:?}"),
        }
    }
    assert_eq!(descriptions, 1);
    assert_eq!(tags, 1);
    assert!(matches!(
        ready,
        Some(TransactionState::Idle | TransactionState::Transaction)
    ));
    decode_catalog_observation(&text.unwrap(), &[])
        .unwrap()
        .stamp()
}

async fn observe(client: &Client) -> NativeCatalogStamp {
    tokio::time::timeout(Duration::from_secs(20), receive_observation(client))
        .await
        .expect("one-shot epoch observation did not complete")
}

async fn hold_probe(client: &Client) {
    client
        .batch_execute("SET darmok_catalog_probe.command = 'hold'")
        .await
        .unwrap();
}

async fn release_probe(client: &Client) {
    client
        .batch_execute("SET darmok_catalog_probe.command = 'release'")
        .await
        .unwrap();
}

async fn pid(client: &Client) -> i32 {
    client
        .query_one("SELECT pg_catalog.pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0)
}

async fn wait_fence(observer: &Client, pid: i32, mode: &str, granted: bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let rows = observer.query(FENCE_LOCKS, &[&pid]).await.unwrap();
            if rows
                .iter()
                .any(|row| row.get::<_, String>(0) == mode && row.get::<_, bool>(1) == granted)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("expected server fence was not observed");
}

async fn no_fence(observer: &Client, pid: i32) {
    assert!(
        observer
            .query(COORDINATION_LOCKS, &[&pid])
            .await
            .unwrap()
            .is_empty()
    );
}

async fn wait_reader_admission(observer: &Client, backend: i32, phase: &str) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let waiting: bool = observer.query_one(
                "SELECT EXISTS (SELECT FROM pg_catalog.pg_stat_activity WHERE pid = $1 AND wait_event = 'Extension')", &[&backend]
            ).await.unwrap().get(0);
            if waiting { return; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap_or_else(|error| panic!("shared-drop reader admission wait {phase} was not observed: {error}"));
    no_fence(observer, backend).await;
}

async fn wait_shared_drop_barrier(observer: &Client, backend: i32) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let waiting: bool = observer.query_one(
                "SELECT EXISTS (SELECT FROM pg_catalog.pg_locks WHERE locktype='advisory' AND classid=17485 AND objid=21316 AND objsubid=2 AND pid=$1 AND mode='ShareLock' AND NOT granted)", &[&backend]
            ).await.unwrap().get(0);
            if waiting { return; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("second shared drop did not reach its post-intent test barrier");
}

// Prepare these observation queries before the pg_class holder is prepared.
// Force and warm generic probe plans before that holder. This controlled
// observer history does not prove arbitrary native planning nonblocking.
struct PublicationLocks {
    coordination: tokio_postgres::Statement,
    late_class_wait: tokio_postgres::Statement,
    prepared_module_locks: tokio_postgres::Statement,
    prepared_relation: tokio_postgres::Statement,
}

impl PublicationLocks {
    async fn prepare(observer: &Client) -> Self {
        observer
            .batch_execute("SET plan_cache_mode = force_generic_plan")
            .await
            .unwrap();
        Self {
            coordination: observer.prepare(COORDINATION_LOCKS).await.unwrap(),
            late_class_wait: observer.prepare(
                "SELECT EXISTS (SELECT FROM pg_catalog.pg_locks WHERE locktype='relation' AND relation=1259 AND pid=$1 AND mode='RowExclusiveLock' AND NOT granted)"
            ).await.unwrap(),
            prepared_module_locks: observer.prepare(
                "SELECT count(*) FROM pg_catalog.pg_locks WHERE locktype='object' AND COALESCE(database,0)=0 AND classid=3079 AND objid=0 AND objsubid IN (17485,17486) AND pid IS NULL"
            ).await.unwrap(),
            prepared_relation: observer.prepare(
                "SELECT EXISTS (SELECT FROM pg_catalog.pg_locks WHERE locktype='relation' AND relation=$1 AND granted AND pid IS NULL)"
            ).await.unwrap(),
        }
    }

    async fn warm(&self, observer: &Client) {
        // Parse is insufficient: first execution builds a plan. Force generic
        // plans and execute every probe before the prepared catalog holder.
        observer.query(&self.coordination, &[&0_i32]).await.unwrap();
        observer
            .query(&self.late_class_wait, &[&0_i32])
            .await
            .unwrap();
        observer
            .query(&self.prepared_module_locks, &[])
            .await
            .unwrap();
        observer
            .query(&self.prepared_relation, &[&1259_u32])
            .await
            .unwrap();
    }

    async fn wait_late_cleanup(&self, observer: &Client, backend: i32) {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if observer
                    .query_one(&self.late_class_wait, &[&backend])
                    .await
                    .unwrap()
                    .get::<_, bool>(0)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("late native pg_class RowExclusive wait was not observed");
        self.no_global(observer, backend).await;
    }

    async fn no_global(&self, observer: &Client, backend: i32) {
        let locks = observer
            .query(&self.coordination, &[&backend])
            .await
            .unwrap();
        assert!(locks.iter().all(|row| row.get::<_, i32>(0) != 17485));
    }

    async fn gate(&self, observer: &Client, backend: i32, mode: &str, granted: bool) {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let locks = observer
                    .query(&self.coordination, &[&backend])
                    .await
                    .unwrap();
                if locks.iter().any(|row| {
                    row.get::<_, i32>(0) == 17486
                        && row.get::<_, String>(1) == mode
                        && row.get::<_, bool>(2) == granted
                }) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("expected native publication gate state was not observed");
        self.no_global(observer, backend).await;
    }

    async fn no_prepared_module_locks(&self, observer: &Client) {
        assert_eq!(
            observer
                .query_one(&self.prepared_module_locks, &[])
                .await
                .unwrap()
                .get::<_, i64>(0),
            0
        );
    }
}

// Cleanup never turns a failed fixture into acceptance. Native absence and
// cleanup errors are logged; the original failure is always rethrown. A timeout
// leaves that disposable profile quarantined instead of cancelling its backend.
async fn cleanup_failed_prepared(finisher: &Client, gids: &[&str]) {
    for gid in gids {
        let sql = format!("ROLLBACK PREPARED '{gid}'");
        match tokio::time::timeout(Duration::from_secs(20), finisher.batch_execute(&sql)).await {
            Ok(Ok(())) => eprintln!("failed-fixture native rollback completed: {gid}"),
            Ok(Err(error)) => eprintln!(
                "failed-fixture native rollback error: {gid} code={:?}",
                error.code()
            ),
            Err(_) => {
                eprintln!("failed-fixture native rollback timed out; profile quarantined: {gid}")
            }
        }
    }
}

#[tokio::test]
async fn late_indexed_temp_publishers_allow_both_prepared_outcomes() {
    let _serial = TEST_SERIAL.lock().await;
    let (first, first_driver) = client().await;
    let (second, second_driver) = client().await;
    let (holder, holder_driver) = client().await;
    let (finisher, finisher_driver) = client().await;
    let (epochs, epochs_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let first_pid = pid(&first).await;
    let second_pid = pid(&second).await;
    let epochs_pid = pid(&epochs).await;
    first.batch_execute(
        "CREATE FUNCTION publication_late_first() RETURNS integer LANGUAGE SQL VOLATILE AS 'SELECT 1';
         CREATE TEMP TABLE publication_late_temp(id integer) ON COMMIT DELETE ROWS;
         CREATE INDEX ON publication_late_temp(id)"
    ).await.unwrap();
    second.batch_execute(
        "CREATE FUNCTION publication_late_second() RETURNS integer LANGUAGE SQL VOLATILE AS 'SELECT 1';
         CREATE TEMP TABLE publication_late_temp(id integer) ON COMMIT DELETE ROWS;
         CREATE INDEX ON publication_late_temp(id)"
    ).await.unwrap();
    let locks = PublicationLocks::prepare(&observer).await;
    for outcome in ["COMMIT", "ROLLBACK"] {
        let phase = std::panic::AssertUnwindSafe(async {
            tokio::time::timeout(Duration::from_secs(60), async {
                first
                    .batch_execute("ALTER FUNCTION publication_late_first() STABLE")
                    .await
                    .unwrap();
                second
                    .batch_execute("ALTER FUNCTION publication_late_second() STABLE")
                    .await
                    .unwrap();
                let before = observe(&epochs).await;
                first.batch_execute("BEGIN; ALTER FUNCTION publication_late_first() IMMUTABLE; INSERT INTO publication_late_temp VALUES (1)").await.unwrap();
                second.batch_execute("BEGIN; ALTER FUNCTION publication_late_second() IMMUTABLE; INSERT INTO publication_late_temp VALUES (2)").await.unwrap();
                locks.warm(&observer).await;
                holder.batch_execute("BEGIN; LOCK TABLE pg_catalog.pg_class IN ACCESS EXCLUSIVE MODE; PREPARE TRANSACTION 'darmok_late_pg_class'").await.unwrap();
                locks.no_prepared_module_locks(&observer).await;

                let mut first_commit = Box::pin(first.batch_execute("COMMIT"));
                tokio::select! {
                    result = &mut first_commit => panic!("first publisher did not reach late cleanup: {result:?}"),
                    () = locks.wait_late_cleanup(&observer, first_pid) => {}
                }
                locks
                    .gate(&observer, first_pid, "RowExclusiveLock", true)
                    .await;
                let mut second_commit = Box::pin(second.batch_execute("COMMIT"));
                tokio::select! {
                    result = &mut second_commit => panic!("second publisher did not reach late cleanup: {result:?}"),
                    () = locks.wait_late_cleanup(&observer, second_pid) => {}
                }
                locks
                    .gate(&observer, second_pid, "RowExclusiveLock", true)
                    .await;

                let mut reading = Box::pin(receive_observation(&epochs));
                tokio::select! {
                    result = &mut reading => panic!("observer escaped unfinished publications: {result:?}"),
                    () = locks.gate(&observer, epochs_pid, "ShareLock", false) => {}
                }
                tokio::time::timeout(
                    Duration::from_secs(20),
                    finisher.batch_execute(&format!("{outcome} PREPARED 'darmok_late_pg_class'")),
                )
                .await
                .unwrap()
                .unwrap();
                tokio::time::timeout(Duration::from_secs(20), &mut first_commit)
                    .await
                    .unwrap()
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(20), &mut second_commit)
                    .await
                    .unwrap()
                    .unwrap();
                let after = tokio::time::timeout(Duration::from_secs(20), &mut reading)
                    .await
                    .unwrap();
                assert!(after.generation() > before.generation());
                drop(first_commit);
                drop(second_commit);
                drop(reading);
                for (writer, backend) in [(&first, first_pid), (&second, second_pid)] {
                    assert_eq!(
                        writer
                            .query_one("SELECT count(*) FROM publication_late_temp", &[])
                            .await
                            .unwrap()
                            .get::<_, i64>(0),
                        0
                    );
                    no_fence(&observer, backend).await;
                }
                assert_eq!(observer.query_one("SELECT count(*) FROM pg_catalog.pg_proc WHERE proname IN ('publication_late_first','publication_late_second') AND provolatile='i'", &[]).await.unwrap().get::<_, i64>(0), 2);
                assert_eq!(observer.query_one("SELECT count(*) FROM pg_catalog.pg_prepared_xacts WHERE gid='darmok_late_pg_class'", &[]).await.unwrap().get::<_, i64>(0), 0);
                no_fence(&observer, epochs_pid).await;
            }).await.expect("late publication fixture phase exceeded 60 seconds");
        }).catch_unwind().await;
        if let Err(panic) = phase {
            cleanup_failed_prepared(&finisher, &["darmok_late_pg_class"]).await;
            std::panic::resume_unwind(panic);
        }
    }

    first
        .batch_execute("DROP FUNCTION publication_late_first(), publication_late_second()")
        .await
        .unwrap();
    close(first, first_driver).await;
    close(second, second_driver).await;
    close(holder, holder_driver).await;
    close(finisher, finisher_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn prior_native_parse_bind_temp_history_releases_prepared_completion_fence() {
    let _serial = TEST_SERIAL.lock().await;
    let (target, target_driver) = client().await;
    let (holder, holder_driver) = client().await;
    let (finisher, finisher_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let mut native = native_frames::NativeFrames::connect().await;
    let (_, backend) = native.simple("SELECT pg_catalog.pg_backend_pid()").await;
    let native_pid: i32 = backend[0].parse().unwrap();
    let (commands, rows) = native
        .simple(
            "CREATE TEMP TABLE publication_caller_temp(id integer) ON COMMIT DELETE ROWS;
         CREATE INDEX ON publication_caller_temp(id)",
        )
        .await;
    assert_eq!(commands, ["CREATE TABLE", "CREATE INDEX"]);
    assert!(rows.is_empty());
    target.batch_execute("CREATE TABLE publication_caller_target(id integer PRIMARY KEY, value integer); INSERT INTO publication_caller_target VALUES(1, 1)").await.unwrap();
    let target_oid: u32 = observer
        .query_one(
            "SELECT 'publication_caller_target'::pg_catalog.regclass::oid",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let locks = PublicationLocks::prepare(&observer).await;
    let value_statement = observer
        .prepare("SELECT value FROM publication_caller_target")
        .await
        .unwrap();
    for outcome in ["COMMIT", "ROLLBACK"] {
        let phase = std::panic::AssertUnwindSafe(async {
            tokio::time::timeout(Duration::from_secs(60), async {
                target
                    .batch_execute("UPDATE publication_caller_target SET value=1")
                    .await
                    .unwrap();
                target.batch_execute("BEGIN; UPDATE publication_caller_target SET value=2; PREPARE TRANSACTION 'darmok_caller_target'").await.unwrap();
                native.trace.clear();
                native
                    .parse_bind(
                        &format!("temp_history_{outcome}"),
                        "temp_portal",
                        "SELECT id FROM publication_caller_temp",
                    )
                    .await;
                native.flush_parse_bind().await;
                assert_eq!(native.trace, ["Parse", "Bind", "Flush"]);
                locks.warm(&observer).await;
                // Also warm the effect oracle's generic plan before the holder.
                observer.query(&value_statement, &[]).await.unwrap();
                holder.batch_execute("BEGIN; LOCK TABLE pg_catalog.pg_class IN ACCESS EXCLUSIVE MODE; PREPARE TRANSACTION 'darmok_caller_pg_class'").await.unwrap();
                locks.no_prepared_module_locks(&observer).await;
                native
                    .parse_bind(
                        &format!("finish_target_{outcome}"),
                        "finish_portal",
                        &format!("{outcome} PREPARED 'darmok_caller_target'"),
                    )
                    .await;
                native.execute_sync("finish_portal").await;
                assert_eq!(
                    native.trace,
                    ["Parse", "Bind", "Flush", "Parse", "Bind", "Execute", "Sync"]
                );
                println!(
                    "native_no_sync_trace outcome={outcome} frames={:?}",
                    native.trace
                );
                locks.wait_late_cleanup(&observer, native_pid).await;
                assert!(
                    !observer
                        .query_one(&locks.prepared_relation, &[&target_oid])
                        .await
                        .unwrap()
                        .get::<_, bool>(0),
                    "first prepared target retained native locks after utility completion"
                );
                let expected = if outcome == "COMMIT" { 2 } else { 1 };
                assert_eq!(
                    observer
                        .query_one(&value_statement, &[])
                        .await
                        .unwrap()
                        .get::<_, i32>(0),
                    expected
                );
                tokio::time::timeout(
                    Duration::from_secs(20),
                    finisher.batch_execute(&format!("{outcome} PREPARED 'darmok_caller_pg_class'")),
                )
                .await
                .unwrap()
                .unwrap();
                native.completion(&format!("{outcome} PREPARED")).await;
                let (commands, rows) = native
                    .simple("SELECT count(*) FROM publication_caller_temp")
                    .await;
                assert_eq!(commands, ["SELECT 1"]);
                assert_eq!(rows, ["0"]);
                no_fence(&observer, native_pid).await;
                assert_eq!(observer.query_one("SELECT count(*) FROM pg_catalog.pg_prepared_xacts WHERE gid IN ('darmok_caller_target','darmok_caller_pg_class')", &[]).await.unwrap().get::<_, i64>(0), 0);
            }).await.expect("native prepared caller fixture phase exceeded 60 seconds");
        }).catch_unwind().await;
        if let Err(panic) = phase {
            cleanup_failed_prepared(
                &finisher,
                &["darmok_caller_target", "darmok_caller_pg_class"],
            )
            .await;
            std::panic::resume_unwind(panic);
        }
    }

    native.close().await;
    target
        .batch_execute("DROP TABLE publication_caller_target")
        .await
        .unwrap();
    close(target, target_driver).await;
    close(holder, holder_driver).await;
    close(finisher, finisher_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn late_on_commit_drop_and_native_cleanup_error_release_publication_gate() {
    let _serial = TEST_SERIAL.lock().await;
    let (writer, writer_driver) = client().await;
    let (epochs, epochs_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let writer_pid = pid(&writer).await;
    let before = observe(&epochs).await;
    writer
        .batch_execute(
            "BEGIN; CREATE TEMP TABLE publication_drop(id integer) ON COMMIT DROP;
         CREATE INDEX ON publication_drop(id);
         CREATE TEMP VIEW publication_drop_view AS SELECT id FROM publication_drop;
         COMMIT",
        )
        .await
        .unwrap();
    assert!(observe(&epochs).await.generation() > before.generation());
    assert!(writer.query_one("SELECT pg_catalog.to_regclass('publication_drop') IS NULL AND pg_catalog.to_regclass('publication_drop_view') IS NULL", &[]).await.unwrap().get::<_, bool>(0));
    no_fence(&observer, writer_pid).await;
    writer.batch_execute("CREATE FUNCTION publication_cleanup_error() RETURNS integer LANGUAGE SQL VOLATILE AS 'SELECT 1'").await.unwrap();
    writer.batch_execute(
        "BEGIN; ALTER FUNCTION publication_cleanup_error() IMMUTABLE;
         CREATE TEMP TABLE publication_cleanup_parent(id integer PRIMARY KEY) ON COMMIT DELETE ROWS;
         CREATE TEMP TABLE publication_cleanup_child(id integer REFERENCES publication_cleanup_parent(id)) ON COMMIT PRESERVE ROWS;
         INSERT INTO publication_cleanup_parent VALUES (1); INSERT INTO publication_cleanup_child VALUES (1)"
    ).await.unwrap();
    let error = writer.batch_execute("COMMIT").await.unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::FEATURE_NOT_SUPPORTED));
    no_fence(&observer, writer_pid).await;
    assert!(writer.query_one("SELECT pg_catalog.to_regclass('publication_cleanup_parent') IS NULL AND pg_catalog.to_regclass('publication_cleanup_child') IS NULL", &[]).await.unwrap().get::<_, bool>(0));
    assert_eq!(writer.query_one("SELECT provolatile::text FROM pg_catalog.pg_proc WHERE oid='publication_cleanup_error()'::pg_catalog.regprocedure", &[]).await.unwrap().get::<_, String>(0), "v");
    observe(&epochs).await;
    writer
        .batch_execute("DROP FUNCTION publication_cleanup_error()")
        .await
        .unwrap();
    close(writer, writer_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn readers_coexist_with_dml_and_fence_external_metadata_variants() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute(
            "CREATE SCHEMA publication_variants;
             CREATE TABLE publication_variants.items(id integer PRIMARY KEY, value integer);
             INSERT INTO publication_variants.items VALUES (1, 1);
             CREATE FUNCTION publication_variants.answer() RETURNS integer LANGUAGE SQL AS 'SELECT 1';
             CREATE TYPE publication_variants.mood AS ENUM ('one');
             CREATE DOMAIN publication_variants.code AS integer;",
        )
        .await
        .unwrap();
    let writer_pid = pid(&writer).await;
    let variants = [
        "ALTER TABLE publication_variants.items ADD COLUMN added bigint",
        "ALTER TABLE publication_variants.items ALTER COLUMN added TYPE numeric(8,2)",
        "CREATE OR REPLACE FUNCTION publication_variants.answer() RETURNS integer LANGUAGE SQL AS 'SELECT 2'",
        "ALTER TYPE publication_variants.mood ADD VALUE 'two'",
        "ALTER DOMAIN publication_variants.code SET NOT NULL",
        "CREATE COLLATION publication_variants.exact FROM pg_catalog.\"C\"",
        "COMMENT ON TABLE publication_variants.items IS 'changed metadata'",
        "DROP TABLE publication_variants.items; CREATE TABLE publication_variants.items(id integer)",
        "ALTER SCHEMA publication_variants RENAME TO publication_variants_renamed",
    ];
    for (index, sql) in variants.iter().enumerate() {
        if index == 0 {
            writer
                .batch_execute("UPDATE publication_variants.items SET value = 2 WHERE id = 1")
                .await
                .unwrap();
            let value: i32 = reader
                .query_one(
                    "SELECT value FROM publication_variants.items WHERE id = 1",
                    &[],
                )
                .await
                .unwrap()
                .get(0);
            assert_eq!(value, 2);
        }
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        observer.batch_execute("BEGIN READ ONLY").await.unwrap();
        let unheld = observe(&epochs).await;
        hold_probe(&reader).await;
        // The one-shot observation releases its fence before returning. A
        // publication may precede the first retained Share, so use the held
        // observation as the DDL baseline rather than equating the two scopes.
        let before = observe(&epochs).await;
        hold_probe(&observer).await;
        assert!(before.generation() >= unheld.generation());
        let ddl = writer.batch_execute(sql);
        tokio::pin!(ddl);
        tokio::select! {
            result = &mut ddl => panic!("DDL escaped its catalog fence: {result:?}"),
            () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
        }

        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
        // The second reader still owns a fence, so releasing one cannot admit DDL.
        wait_fence(&observer, writer_pid, "ExclusiveLock", false).await;
        release_probe(&observer).await;
        observer.batch_execute("COMMIT").await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), &mut ddl)
            .await
            .unwrap()
            .unwrap();
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let after = observe(&epochs).await;
        hold_probe(&reader).await;
        assert!(after.generation() > before.generation(), "{sql}");

        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
        no_fence(&observer, writer_pid).await;
    }
    writer
        .batch_execute("DROP SCHEMA publication_variants_renamed CASCADE")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn ordinary_ddl_fences_publication_after_native_locks_and_effects() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let writer_pid = pid(&writer).await;
    writer
        .batch_execute("CREATE TABLE publication_writer(id integer)")
        .await
        .unwrap();
    reader.batch_execute("BEGIN").await.unwrap();
    let before = observe(&epochs).await;
    hold_probe(&reader).await;
    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("BEGIN; SAVEPOINT child; ALTER TABLE publication_writer ADD COLUMN hidden integer; RELEASE child")
        .await
        .unwrap();
    no_fence(&observer, writer_pid).await;
    reader.batch_execute("BEGIN").await.unwrap();
    let during = observe(&epochs).await;
    hold_probe(&reader).await;
    assert_eq!(during.generation(), before.generation());
    let columns: i64 = reader
        .query_one("SELECT count(*) FROM pg_catalog.pg_attribute WHERE attrelid = 'publication_writer'::pg_catalog.regclass AND attnum > 0 AND NOT attisdropped", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(columns, 1);
    let mut committing = Box::pin(writer.batch_execute("COMMIT"));
    tokio::select! {
        result = &mut committing => panic!("DDL published through a held native test probe: {result:?}"),
        () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
    }

    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut committing)
        .await
        .unwrap()
        .unwrap();
    drop(committing);
    reader.batch_execute("BEGIN").await.unwrap();
    let after = observe(&epochs).await;
    hold_probe(&reader).await;
    assert!(after.generation() > before.generation());
    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("BEGIN; SAVEPOINT bad_ddl")
        .await
        .unwrap();
    let error = writer
        .batch_execute("ALTER TABLE publication_writer ADD COLUMN id integer")
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::DUPLICATE_COLUMN));
    writer
        .batch_execute("ROLLBACK TO bad_ddl; RELEASE bad_ddl")
        .await
        .unwrap();
    no_fence(&observer, writer_pid).await;
    // The recovered parent can acquire, check and complete a one-shot observation.
    observe(&epochs).await;
    hold_probe(&writer).await;

    release_probe(&writer).await;
    writer
        .batch_execute("COMMIT; DROP TABLE publication_writer")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn concurrent_index_fences_publication_without_blocking_old_snapshots() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (blocker, blocker_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute("CREATE TABLE publication_index(id integer PRIMARY KEY, value integer); INSERT INTO publication_index VALUES (1, 1)")
        .await
        .unwrap();
    blocker
        .batch_execute("BEGIN; UPDATE publication_index SET value = 2 WHERE id = 1")
        .await
        .unwrap();
    let writer_pid = pid(&writer).await;
    let reader_pid = pid(&reader).await;
    let mut building = Box::pin(writer.batch_execute(
        "CREATE INDEX CONCURRENTLY publication_index_value ON publication_index(value)",
    ));
    tokio::select! {
        result = &mut building => panic!("index did not wait for the ordinary writer: {result:?}"),
        () = async {
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let rows = observer.query("SELECT phase FROM pg_catalog.pg_stat_progress_create_index WHERE pid = $1", &[&writer_pid]).await.unwrap();
                    if rows.iter().any(|row| row.get::<_, String>(0) == "waiting for writers before build") {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.expect("concurrent index did not reach its post-commit writer wait");
        } => {}
    }
    // The first catalog publication is finished. Keeping an exclusive session
    // fence during this wait would deadlock later old-snapshot waits with a
    // reader waiting for its next one-shot observation.
    no_fence(&observer, writer_pid).await;
    reader.batch_execute("BEGIN").await.unwrap();
    let intermediate = observe(&epochs).await;
    hold_probe(&reader).await;

    blocker.batch_execute("COMMIT").await.unwrap();
    tokio::select! {
        result = &mut building => panic!("index published its next phase through a held test probe: {result:?}"),
        () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
    }
    wait_fence(&observer, reader_pid, "ShareLock", true).await;

    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut building)
        .await
        .unwrap()
        .unwrap();
    drop(building);
    reader.batch_execute("BEGIN").await.unwrap();
    let stamp = observe(&epochs).await;
    hold_probe(&reader).await;
    assert!(stamp.generation() > intermediate.generation());

    let valid: bool = reader
        .query_one("SELECT indisvalid FROM pg_catalog.pg_index WHERE indexrelid = 'publication_index_value'::pg_catalog.regclass", &[])
        .await
        .unwrap()
        .get(0);
    assert!(valid);
    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    no_fence(&observer, writer_pid).await;
    writer
        .batch_execute("DROP TABLE publication_index")
        .await
        .unwrap();
    close(blocker, blocker_driver).await;
    close(writer, writer_driver).await;
    close(reader, reader_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn concurrent_index_final_valid_transition_has_distinct_generation() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (blocker, blocker_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute("CREATE TABLE publication_index_final(id integer PRIMARY KEY, value integer); INSERT INTO publication_index_final VALUES (1, 1)")
        .await
        .unwrap();
    blocker
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .unwrap();
    assert_eq!(
        blocker
            .query_one(
                "SELECT value FROM publication_index_final WHERE id = 1",
                &[]
            )
            .await
            .unwrap()
            .get::<_, i32>(0),
        1
    );
    let writer_pid = pid(&writer).await;
    let mut building = Box::pin(writer.batch_execute(
        "CREATE INDEX CONCURRENTLY publication_index_final_value ON publication_index_final(value)",
    ));
    tokio::select! {
        result = &mut building => panic!("index did not wait for the old ordinary snapshot: {result:?}"),
        () = async {
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let rows = observer.query("SELECT phase FROM pg_catalog.pg_stat_progress_create_index WHERE pid = $1", &[&writer_pid]).await.unwrap();
                    if rows.iter().any(|row| row.get::<_, String>(0) == "waiting for old snapshots") {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.expect("concurrent index did not reach its final old-snapshot wait");
        } => {}
    }
    no_fence(&observer, writer_pid).await;
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let intermediate = observe(&epochs).await;
    hold_probe(&reader).await;
    let flags_sql = "SELECT indisready, indisvalid FROM pg_catalog.pg_index
        WHERE indexrelid = 'publication_index_final_value'::pg_catalog.regclass";
    let flags = reader.query_one(flags_sql, &[]).await.unwrap();
    assert!(flags.get::<_, bool>(0));
    assert!(!flags.get::<_, bool>(1));
    // A cache can contain the committed ready-but-invalid index at this stamp.
    // The final publication must wait for the native test probe and assign a new stamp.
    blocker.batch_execute("COMMIT").await.unwrap();
    tokio::select! {
        result = &mut building => panic!("index published validity through a held test probe: {result:?}"),
        () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
    }

    assert!(
        !reader
            .query_one(flags_sql, &[])
            .await
            .unwrap()
            .get::<_, bool>(1)
    );
    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut building)
        .await
        .unwrap()
        .unwrap();
    drop(building);
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let published = observe(&epochs).await;
    hold_probe(&reader).await;

    let final_flags = reader.query_one(flags_sql, &[]).await.unwrap();
    let ready: bool = final_flags.get(0);
    let valid: bool = final_flags.get(1);
    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    no_fence(&observer, writer_pid).await;
    writer
        .batch_execute("DROP TABLE publication_index_final")
        .await
        .unwrap();
    close(blocker, blocker_driver).await;
    close(writer, writer_driver).await;
    close(reader, reader_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
    eprintln!(
        "concurrent index validity stamps: invalid={}, valid={}, indisready={ready}, indisvalid={valid}",
        intermediate.generation(),
        published.generation()
    );
    assert!(ready && valid);
    assert!(
        published.generation() > intermediate.generation(),
        "distinct committed catalog facts reused a generation"
    );
}

async fn wait_native_lock(observer: &Client, backend: i32, query_prefix: &str) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let waiting: bool = observer.query_one(
                "SELECT EXISTS (SELECT FROM pg_catalog.pg_stat_activity WHERE pid = $1 AND wait_event_type = 'Lock' AND starts_with(query, $2))", &[&backend, &query_prefix]
            ).await.unwrap().get(0);
            if waiting { return; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("native dependency wait was not observed");
}

#[tokio::test]
async fn prepared_dml_and_rolled_back_child_ddl_fence_completion() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (rows, rows_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let enabled: i32 = observer
        .query_one(
            "SELECT current_setting('max_prepared_transactions')::integer",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        enabled >= 2,
        "required concurrent native 2PC fixture is unavailable"
    );
    writer.batch_execute("CREATE TABLE publication_prepared_dml(id integer PRIMARY KEY, value integer); INSERT INTO publication_prepared_dml VALUES (1, 1)").await.unwrap();
    let rows_pid = pid(&rows).await;
    let writer_pid = pid(&writer).await;
    for (index, outcome) in ["COMMIT", "ROLLBACK", "COMMIT", "ROLLBACK"]
        .iter()
        .enumerate()
    {
        let child = if index >= 2 {
            "SAVEPOINT child; ALTER TABLE publication_prepared_dml ADD COLUMN rolled_back integer; ROLLBACK TO child; RELEASE child;"
        } else {
            ""
        };
        writer.batch_execute(&format!(
            "BEGIN; {child} UPDATE publication_prepared_dml SET value = value + 10; PREPARE TRANSACTION 'darmok_prepared_dml'"
        )).await.unwrap();
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let stamp = observe(&epochs).await;
        hold_probe(&reader).await;
        rows.batch_execute("BEGIN; LOCK TABLE publication_prepared_dml IN ROW EXCLUSIVE MODE")
            .await
            .unwrap();
        let mut updating =
            Box::pin(rows.batch_execute("UPDATE publication_prepared_dml SET value = value + 1"));
        tokio::select! {
            result = &mut updating => panic!("row update did not wait for the prepared native transaction: {result:?}"),
            () = wait_native_lock(&observer, rows_pid, "UPDATE publication_prepared_dml") => {}
        }
        // Row execution owns no global fence. Even metadata-free prepared
        // completion must first drain the separate native probe.
        let finish_sql = format!("{outcome} PREPARED 'darmok_prepared_dml'");
        let mut finishing = Box::pin(writer.batch_execute(&finish_sql));
        tokio::select! {
            result = &mut finishing => panic!("prepared DML bypassed its publication fence: {result:?}"),
            () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
        }

        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), &mut finishing)
            .await
            .unwrap()
            .unwrap();
        drop(finishing);
        tokio::time::timeout(Duration::from_secs(20), &mut updating)
            .await
            .unwrap()
            .unwrap();
        drop(updating);
        rows.batch_execute("COMMIT").await.unwrap();
        reader.batch_execute("BEGIN").await.unwrap();
        let after = observe(&epochs).await;
        hold_probe(&reader).await;
        assert_eq!(after.generation(), stamp.generation() + 1);
        let rolled_back: bool = reader.query_one(
            "SELECT EXISTS (SELECT FROM pg_catalog.pg_attribute WHERE attrelid = 'publication_prepared_dml'::pg_catalog.regclass AND attname = 'rolled_back' AND NOT attisdropped)", &[]
        ).await.unwrap().get(0);
        assert!(!rolled_back);
        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
        no_fence(&observer, writer_pid).await;
    }
    let value: i32 = rows
        .query_one("SELECT value FROM publication_prepared_dml", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(value, 25);
    writer
        .batch_execute("DROP TABLE publication_prepared_dml")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(rows, rows_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn prepared_ddl_fences_completion_and_native_guards_wait_outside_the_fence() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (guarded, guarded_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute("CREATE TABLE publication_prepared_ddl(id integer)")
        .await
        .unwrap();
    let writer_pid = pid(&writer).await;
    let guarded_pid = pid(&guarded).await;
    for (index, outcome) in ["COMMIT", "ROLLBACK"].iter().enumerate() {
        writer.batch_execute(&format!(
            "BEGIN; ALTER TABLE publication_prepared_ddl ADD COLUMN change_{index} integer; PREPARE TRANSACTION 'darmok_prepared_ddl'"
        )).await.unwrap();
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let before = observe(&epochs).await;
        hold_probe(&reader).await;
        let old_columns: i64 = reader.query_one(
            "SELECT count(*) FROM pg_catalog.pg_attribute WHERE attrelid = 'publication_prepared_ddl'::pg_catalog.regclass AND attnum > 0 AND NOT attisdropped", &[]
        ).await.unwrap().get(0);
        assert_eq!(old_columns, 1 + index as i64);
        guarded.batch_execute("BEGIN").await.unwrap();
        let mut locking = Box::pin(
            guarded.batch_execute("LOCK TABLE publication_prepared_ddl IN ACCESS SHARE MODE"),
        );
        tokio::select! {
            result = &mut locking => panic!("dependency guard escaped prepared DDL: {result:?}"),
            () = wait_native_lock(&observer, guarded_pid, "LOCK TABLE publication_prepared_ddl") => {}
        }
        let finish_sql = format!("{outcome} PREPARED 'darmok_prepared_ddl'");
        let mut finishing = Box::pin(writer.batch_execute(&finish_sql));
        tokio::select! {
            result = &mut finishing => panic!("prepared metadata escaped its publication fence: {result:?}"),
            () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
        }

        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), &mut finishing)
            .await
            .unwrap()
            .unwrap();
        drop(finishing);
        tokio::time::timeout(Duration::from_secs(20), &mut locking)
            .await
            .unwrap()
            .unwrap();
        drop(locking);
        let validated = observe(&guarded).await;
        assert!(validated.generation() > before.generation());

        no_fence(&observer, guarded_pid).await;
        // Native dependency guard remains; global catalog fence has ended.
        guarded
            .query("SELECT * FROM publication_prepared_ddl", &[])
            .await
            .unwrap();
        guarded.batch_execute("COMMIT").await.unwrap();
        no_fence(&observer, writer_pid).await;
    }
    writer
        .batch_execute("DROP TABLE publication_prepared_ddl")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(guarded, guarded_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn mixed_prepared_metadata_and_unrelated_row_locks_can_finish() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer.batch_execute("CREATE TABLE publication_mixed_catalog(id integer); CREATE TABLE publication_mixed_rows(id integer PRIMARY KEY, value integer); INSERT INTO publication_mixed_rows VALUES (1, 1)").await.unwrap();
    writer.batch_execute("BEGIN; ALTER TABLE publication_mixed_catalog ADD COLUMN changed integer; UPDATE publication_mixed_rows SET value = 10; PREPARE TRANSACTION 'darmok_prepared_mixed'").await.unwrap();
    reader
        .batch_execute("BEGIN; LOCK TABLE publication_mixed_rows IN ROW EXCLUSIVE MODE")
        .await
        .unwrap();
    observe(&epochs).await;
    hold_probe(&reader).await;

    release_probe(&reader).await;
    let reader_pid = pid(&reader).await;
    let mut updating =
        Box::pin(reader.batch_execute("UPDATE publication_mixed_rows SET value = value + 1"));
    tokio::select! {
        result = &mut updating => panic!("mixed prepared row lock was not observed: {result:?}"),
        () = wait_native_lock(&observer, reader_pid, "UPDATE publication_mixed_rows") => {}
    }
    tokio::time::timeout(
        Duration::from_secs(20),
        writer.batch_execute("COMMIT PREPARED 'darmok_prepared_mixed'"),
    )
    .await
    .unwrap()
    .unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut updating)
        .await
        .unwrap()
        .unwrap();
    drop(updating);
    reader.batch_execute("COMMIT").await.unwrap();
    let value: i32 = reader
        .query_one("SELECT value FROM publication_mixed_rows", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(value, 11);
    writer
        .batch_execute("DROP TABLE publication_mixed_catalog, publication_mixed_rows")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn concurrent_prepared_transactions_and_gid_reuse_keep_native_identity() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (dml, dml_driver) = client().await;
    let (replacement, replacement_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer.batch_execute("CREATE TABLE publication_gid_catalog(id integer); CREATE TABLE publication_gid_rows(id integer, value integer); INSERT INTO publication_gid_rows VALUES (1, 1)").await.unwrap();
    writer.batch_execute("BEGIN; ALTER TABLE publication_gid_catalog ADD COLUMN changed integer; PREPARE TRANSACTION 'darmok_gid_reused'").await.unwrap();
    dml.batch_execute(
        "BEGIN; UPDATE publication_gid_rows SET value = 10; PREPARE TRANSACTION 'darmok_gid_dml'",
    )
    .await
    .unwrap();
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let before = observe(&epochs).await;
    hold_probe(&reader).await;
    let writer_pid = pid(&writer).await;
    let dml_pid = pid(&dml).await;
    let replacement_pid = pid(&replacement).await;
    let mut finishing = Box::pin(writer.batch_execute("COMMIT PREPARED 'darmok_gid_reused'"));
    tokio::select! {
        result = &mut finishing => panic!("prepared metadata escaped the held test probe: {result:?}"),
        () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
    }
    // Core retains exact GID uniqueness. A second PREPARE fails while the
    // original is still valid; no module gate changes native error behavior.
    let error = replacement
        .batch_execute("BEGIN; PREPARE TRANSACTION 'darmok_gid_reused'")
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::DUPLICATE_OBJECT));
    replacement.batch_execute("ROLLBACK").await.unwrap();
    no_fence(&observer, replacement_pid).await;
    let mut finishing_dml = Box::pin(dml.batch_execute("COMMIT PREPARED 'darmok_gid_dml'"));
    tokio::select! {
        result = &mut finishing_dml => panic!("prepared DML escaped the held test probe: {result:?}"),
        () = wait_fence(&observer, dml_pid, "ExclusiveLock", false) => {}
    }

    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut finishing)
        .await
        .unwrap()
        .unwrap();
    drop(finishing);
    tokio::time::timeout(Duration::from_secs(20), &mut finishing_dml)
        .await
        .unwrap()
        .unwrap();
    drop(finishing_dml);
    // Reuse is admitted only after the native original has finished.
    replacement
        .batch_execute("BEGIN; PREPARE TRANSACTION 'darmok_gid_reused'")
        .await
        .unwrap();
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let after = observe(&epochs).await;
    hold_probe(&reader).await;
    assert_eq!(after.generation(), before.generation() + 2);
    let value: i32 = reader
        .query_one("SELECT value FROM publication_gid_rows", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(value, 10);
    let changed: bool = reader.query_one(
        "SELECT EXISTS (SELECT FROM pg_catalog.pg_attribute WHERE attrelid = 'publication_gid_catalog'::pg_catalog.regclass AND attname = 'changed' AND NOT attisdropped)", &[]
    ).await.unwrap().get(0);
    assert!(changed);
    let mut finishing_reuse =
        Box::pin(replacement.batch_execute("COMMIT PREPARED 'darmok_gid_reused'"));
    tokio::select! {
        result = &mut finishing_reuse => panic!("reused GID completion escaped the held test probe: {result:?}"),
        () = wait_fence(&observer, replacement_pid, "ExclusiveLock", false) => {}
    }

    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut finishing_reuse)
        .await
        .unwrap()
        .unwrap();
    drop(finishing_reuse);
    no_fence(&observer, replacement_pid).await;
    writer
        .batch_execute("DROP TABLE publication_gid_catalog, publication_gid_rows")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(dml, dml_driver).await;
    close(replacement, replacement_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn queued_prepare_stays_private_and_completion_uses_native_gid_validity() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (finisher, finisher_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute(
            "CREATE TABLE publication_prepare_target(id integer);
         CREATE TABLE publication_prepare_events(id integer);
         CREATE FUNCTION publication_prepare_deferred() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
             PERFORM pg_catalog.pg_advisory_xact_lock(708923);
             EXECUTE 'ALTER TABLE publication_prepare_target ADD COLUMN from_deferred integer';
             RETURN NULL;
         END $$;
         CREATE CONSTRAINT TRIGGER publication_prepare_trigger AFTER INSERT ON publication_prepare_events
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION publication_prepare_deferred()",
        )
        .await
        .unwrap();
    observer
        .query_one("SELECT pg_catalog.pg_advisory_lock(708923)", &[])
        .await
        .unwrap();
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let before = observe(&epochs).await;
    hold_probe(&reader).await;
    // Core drains the open child and deferred triggers after utility entry.
    writer
        .batch_execute("BEGIN; SAVEPOINT child; INSERT INTO publication_prepare_events VALUES (1)")
        .await
        .unwrap();
    let writer_pid = pid(&writer).await;
    let finisher_pid = pid(&finisher).await;
    let mut preparing =
        Box::pin(writer.batch_execute("PREPARE TRANSACTION 'darmok_queued_prepare'"));
    tokio::select! {
        result = &mut preparing => panic!("native prepare did not reach its deferred trigger wait: {result:?}"),
        () = wait_native_lock(&observer, writer_pid, "PREPARE TRANSACTION") => {}
    }
    let visible: bool = observer.query_one(
        "SELECT EXISTS (SELECT FROM pg_catalog.pg_prepared_xacts WHERE gid = 'darmok_queued_prepare')", &[]
    ).await.unwrap().get(0);
    assert!(
        !visible,
        "native preparation must still be before GID validity"
    );
    no_fence(&observer, writer_pid).await;
    let mut finishing = Box::pin(finisher.batch_execute("COMMIT PREPARED 'darmok_queued_prepare'"));
    tokio::select! {
        result = &mut finishing => panic!("completion bypassed the publication fence: {result:?}"),
        () = wait_fence(&observer, finisher_pid, "ExclusiveLock", false) => {}
    }

    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    // The trigger is still blocked. Native core reports that this GID is not
    // yet valid; the module does not turn a queued PREPARE into a valid target.
    let error = tokio::time::timeout(Duration::from_secs(20), &mut finishing)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::UNDEFINED_OBJECT));
    drop(finishing);
    no_fence(&observer, finisher_pid).await;
    observer
        .query_one("SELECT pg_catalog.pg_advisory_unlock(708923)", &[])
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut preparing)
        .await
        .unwrap()
        .unwrap();
    drop(preparing);
    no_fence(&observer, writer_pid).await;
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let private = observe(&epochs).await;
    hold_probe(&reader).await;
    assert_eq!(private.generation(), before.generation() + 1);
    let mut finishing = Box::pin(finisher.batch_execute("COMMIT PREPARED 'darmok_queued_prepare'"));
    tokio::select! {
        result = &mut finishing => panic!("deferred metadata completed without a fence: {result:?}"),
        () = wait_fence(&observer, finisher_pid, "ExclusiveLock", false) => {}
    }

    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut finishing)
        .await
        .unwrap()
        .unwrap();
    drop(finishing);
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let after = observe(&epochs).await;
    hold_probe(&reader).await;
    assert_eq!(after.generation(), private.generation() + 1);
    let columns: i64 = reader.query_one(
        "SELECT count(*) FROM pg_catalog.pg_attribute WHERE attrelid = 'publication_prepare_target'::pg_catalog.regclass AND attnum > 0 AND NOT attisdropped", &[]
    ).await.unwrap().get(0);
    assert_eq!(columns, 2);
    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    writer.batch_execute("DROP TABLE publication_prepare_target, publication_prepare_events; DROP FUNCTION publication_prepare_deferred()").await.unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(finisher, finisher_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn preparation_noop_abort_and_native_errors_release_publication_fences() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (other, other_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let writer_pid = pid(&writer).await;
    writer
        .batch_execute("PREPARE TRANSACTION 'darmok_prepare_cleanup'")
        .await
        .unwrap();
    writer.batch_execute("BEGIN").await.unwrap();
    let error = writer.query_one("SELECT 1 / 0", &[]).await.unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::DIVISION_BY_ZERO));
    writer
        .batch_execute("PREPARE TRANSACTION 'darmok_prepare_cleanup'")
        .await
        .unwrap();
    no_fence(&observer, writer_pid).await;
    let prepared: i64 = other.query_one(
        "SELECT count(*) FROM pg_catalog.pg_prepared_xacts WHERE gid = 'darmok_prepare_cleanup'", &[]
    ).await.unwrap().get(0);
    assert_eq!(prepared, 0);
    other
        .batch_execute("BEGIN; PREPARE TRANSACTION 'darmok_prepare_cleanup'")
        .await
        .unwrap();
    let error = writer
        .batch_execute("BEGIN; SAVEPOINT child; PREPARE TRANSACTION 'darmok_prepare_cleanup'")
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::DUPLICATE_OBJECT));
    writer.batch_execute("ROLLBACK").await.unwrap();
    no_fence(&observer, writer_pid).await;
    let transferred: i64 = observer.query_one(
        "SELECT count(*) FROM pg_catalog.pg_locks WHERE locktype = 'object' AND COALESCE(database, 0) = 0 AND classid = 'pg_catalog.pg_extension'::pg_catalog.regclass AND objid = 0 AND objsubid IN (17485, 17486) AND pid IS NULL", &[]
    ).await.unwrap().get(0);
    assert_eq!(
        transferred, 0,
        "PREPARE transferred a catalog coordination lock"
    );
    other
        .batch_execute("ROLLBACK PREPARED 'darmok_prepare_cleanup'")
        .await
        .unwrap();
    for outcome in ["COMMIT", "ROLLBACK"] {
        other.batch_execute("BEGIN READ ONLY").await.unwrap();
        let before = observe(&epochs).await;
        hold_probe(&other).await;
        let sql = format!("{outcome} PREPARED 'darmok_prepare_cleanup'");
        let mut finishing = Box::pin(writer.batch_execute(&sql));
        tokio::select! {
            result = &mut finishing => panic!("attempted native finish bypassed its fence: {result:?}"),
            () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
        }

        release_probe(&other).await;
        other.batch_execute("COMMIT").await.unwrap();
        let error = tokio::time::timeout(Duration::from_secs(20), &mut finishing)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::UNDEFINED_OBJECT));
        drop(finishing);
        no_fence(&observer, writer_pid).await;
        other.batch_execute("BEGIN READ ONLY").await.unwrap();
        let after = observe(&epochs).await;
        hold_probe(&other).await;
        assert_eq!(after.generation(), before.generation() + 1);
        release_probe(&other).await;
        other.batch_execute("COMMIT").await.unwrap();
    }
    for outcome in ["COMMIT", "ROLLBACK"] {
        other.batch_execute("BEGIN READ ONLY").await.unwrap();
        let before = observe(&epochs).await;
        hold_probe(&other).await;
        writer
            .batch_execute("BEGIN; SAVEPOINT child")
            .await
            .unwrap();
        let sql = format!("{outcome} PREPARED 'darmok_prepare_cleanup'");
        let mut finishing = Box::pin(writer.batch_execute(&sql));
        tokio::select! {
            result = &mut finishing => panic!("child native finish bypassed its fence: {result:?}"),
            () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
        }
        release_probe(&other).await;
        other.batch_execute("COMMIT").await.unwrap();
        let error = tokio::time::timeout(Duration::from_secs(20), &mut finishing)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code(), Some(&SqlState::ACTIVE_SQL_TRANSACTION));
        drop(finishing);
        no_fence(&observer, writer_pid).await;
        writer
            .batch_execute("ROLLBACK TO child; RELEASE child")
            .await
            .unwrap();
        let recovered = observe(&epochs).await;
        hold_probe(&writer).await;
        assert_eq!(recovered.generation(), before.generation() + 1);
        release_probe(&writer).await;
        writer.batch_execute("COMMIT").await.unwrap();
        no_fence(&observer, writer_pid).await;
    }
    close(writer, writer_driver).await;
    close(other, other_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn prepared_catalog_view_locks_do_not_block_their_own_completion() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    // Open all backends and resolve observer dependencies before a prepared
    // LOCK on this view recursively retains its underlying catalog locks.
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (finisher, finisher_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let finisher_pid = pid(&finisher).await;
    let view_oid: u32 = observer
        .query_one(
            "SELECT 'pg_catalog.pg_prepared_xacts'::pg_catalog.regclass::oid",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let view_lock = observer.prepare(
        "SELECT EXISTS (SELECT FROM pg_catalog.pg_locks WHERE relation = $1 AND mode = 'AccessExclusiveLock' AND granted AND pid IS NULL)"
    ).await.unwrap();
    observer.query(FENCE_LOCKS, &[&finisher_pid]).await.unwrap();
    for outcome in ["COMMIT", "ROLLBACK"] {
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let before = observe(&epochs).await;
        hold_probe(&reader).await;
        tokio::time::timeout(Duration::from_secs(20), writer.batch_execute(
            "BEGIN; LOCK TABLE pg_catalog.pg_prepared_xacts IN ACCESS EXCLUSIVE MODE; PREPARE TRANSACTION 'darmok_prepared_view_lock'"
        )).await.unwrap().unwrap();
        let retained: bool = observer
            .query_one(&view_lock, &[&view_oid])
            .await
            .unwrap()
            .get(0);
        assert!(retained, "native view lock was not transferred to PREPARE");
        let sql = format!("{outcome} PREPARED 'darmok_prepared_view_lock'");
        let mut finishing = Box::pin(finisher.batch_execute(&sql));
        tokio::select! {
            result = &mut finishing => panic!("prepared view-lock finish bypassed the reader fence: {result:?}"),
            () = wait_fence(&observer, finisher_pid, "ExclusiveLock", false) => {}
        }

        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), &mut finishing)
            .await
            .unwrap()
            .unwrap();
        drop(finishing);
        let retained: bool = observer
            .query_one(&view_lock, &[&view_oid])
            .await
            .unwrap()
            .get(0);
        assert!(
            !retained,
            "native completion retained its catalog view lock"
        );
        let remaining: i64 = observer.query_one(
            "SELECT count(*) FROM pg_catalog.pg_prepared_xacts WHERE gid = 'darmok_prepared_view_lock'", &[]
        ).await.unwrap().get(0);
        assert_eq!(remaining, 0);
        no_fence(&observer, finisher_pid).await;
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let after = observe(&epochs).await;
        hold_probe(&reader).await;
        assert_eq!(after.generation(), before.generation() + 1);
        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
    }
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(finisher, finisher_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn prepared_gid_identity_is_independent_of_the_native_text_operator_path() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (lower, lower_driver) = client().await;
    let (upper, upper_driver) = client().await;
    let (finisher, finisher_driver) = client().await;
    let (observer, observer_driver) = client().await;
    lower.batch_execute(
        "CREATE TABLE publication_case_lower(id integer);
         CREATE TABLE publication_case_upper(id integer);
         CREATE SCHEMA publication_gid_operators;
         CREATE FUNCTION publication_gid_operators.equal_folded(text, text) RETURNS boolean
         LANGUAGE SQL IMMUTABLE AS $$
             SELECT pg_catalog.lower($1) OPERATOR(pg_catalog.=) pg_catalog.lower($2)
         $$;
         CREATE OPERATOR publication_gid_operators.= (LEFTARG = text, RIGHTARG = text, FUNCTION = publication_gid_operators.equal_folded)"
    ).await.unwrap();
    finisher
        .batch_execute("SET search_path = publication_gid_operators, pg_catalog")
        .await
        .unwrap();
    let comparison = finisher
        .query_one(
            "SELECT 'darmok_case_gid'::text = 'DARMOK_CASE_GID'::text,
                'darmok_case_gid'::text OPERATOR(pg_catalog.=) 'DARMOK_CASE_GID'::text",
            &[],
        )
        .await
        .unwrap();
    assert!(comparison.get::<_, bool>(0));
    assert!(!comparison.get::<_, bool>(1));
    lower.batch_execute("BEGIN; ALTER TABLE publication_case_lower ADD COLUMN changed integer; PREPARE TRANSACTION 'darmok_case_gid'").await.unwrap();
    upper.batch_execute("BEGIN; ALTER TABLE publication_case_upper ADD COLUMN changed integer; PREPARE TRANSACTION 'DARMOK_CASE_GID'").await.unwrap();
    let finisher_pid = pid(&finisher).await;
    for (outcome, gid, remaining) in [
        ("COMMIT", "darmok_case_gid", 1_i64),
        ("ROLLBACK", "DARMOK_CASE_GID", 0_i64),
    ] {
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let before = observe(&epochs).await;
        hold_probe(&reader).await;
        let sql = format!("{outcome} PREPARED '{gid}'");
        let mut finishing = Box::pin(finisher.batch_execute(&sql));
        tokio::select! {
            result = &mut finishing => panic!("operator-path prepared finish bypassed the fence: {result:?}"),
            () = wait_fence(&observer, finisher_pid, "ExclusiveLock", false) => {}
        }

        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), &mut finishing)
            .await
            .unwrap()
            .unwrap();
        drop(finishing);
        no_fence(&observer, finisher_pid).await;
        let count: i64 = observer.query_one(
            "SELECT count(*) FROM pg_catalog.pg_prepared_xacts WHERE gid IN ('darmok_case_gid', 'DARMOK_CASE_GID')", &[]
        ).await.unwrap().get(0);
        assert_eq!(count, remaining);
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let after = observe(&epochs).await;
        hold_probe(&reader).await;
        assert_eq!(after.generation(), before.generation() + 1);
        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
    }
    for (table, expected) in [
        ("publication_case_lower", true),
        ("publication_case_upper", false),
    ] {
        let changed: bool = observer.query_one(
            "SELECT EXISTS (SELECT FROM pg_catalog.pg_attribute WHERE attrelid = $1::text::pg_catalog.regclass AND attname = 'changed' AND NOT attisdropped)", &[&table]
        ).await.unwrap().get(0);
        assert_eq!(changed, expected);
    }
    finisher.batch_execute("RESET search_path").await.unwrap();
    lower.batch_execute("DROP TABLE publication_case_lower, publication_case_upper; DROP SCHEMA publication_gid_operators CASCADE").await.unwrap();
    close(reader, reader_driver).await;
    close(lower, lower_driver).await;
    close(upper, upper_driver).await;
    close(finisher, finisher_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn database_removal_and_target_or_unrelated_temp_backend_exit_complete() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let writer_pid = pid(&writer).await;
    for target_database in [false, true] {
        let database = format!("darmok_publication_drop_exit_{}", u8::from(target_database));
        writer
            .batch_execute(&format!("CREATE DATABASE {database}"))
            .await
            .unwrap();
        let (temporary, temporary_driver) = if target_database {
            database_client(&database).await
        } else {
            client().await
        };
        let (late_reader, late_driver) = client().await;
        temporary.batch_execute(
            "CREATE TEMP TABLE publication_exit_temp(id integer); INSERT INTO publication_exit_temp VALUES (1)"
        ).await.unwrap();
        let temp_pid = pid(&temporary).await;
        let late_pid = pid(&late_reader).await;
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        late_reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let before = observe(&epochs).await;
        hold_probe(&reader).await;
        let drop_sql = format!("DROP DATABASE {database}");
        let mut dropping = Box::pin(complete_drop(&writer, &drop_sql));
        tokio::select! {
            result = &mut dropping => panic!("database removal escaped the catalog fence: {result:?}"),
            () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
        }
        close(temporary, temporary_driver).await;
        wait_fence(&observer, temp_pid, "ExclusiveLock", false).await;
        let mut late_acquisition = Box::pin(receive_observation(&late_reader));
        tokio::select! {
            result = &mut late_acquisition => panic!("new reader bypassed shared-drop admission: {result:?}"),
            () = wait_reader_admission(&observer, late_pid, "single drop before completion") => {}
        }

        release_probe(&reader).await;
        reader.batch_execute("COMMIT").await.unwrap();
        tokio::time::timeout(DATABASE_DROP_COMPLETION, &mut dropping)
            .await
            .unwrap()
            .unwrap();
        drop(dropping);
        let after = tokio::time::timeout(Duration::from_secs(20), &mut late_acquisition)
            .await
            .unwrap();
        drop(late_acquisition);
        assert!(after.generation() > before.generation());

        let remains: bool = late_reader
            .query_one(
                "SELECT EXISTS (SELECT FROM pg_catalog.pg_database WHERE datname = $1)",
                &[&database],
            )
            .await
            .unwrap()
            .get(0);
        assert!(!remains);
        no_fence(&observer, late_pid).await;
        late_reader.batch_execute("COMMIT").await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let present: bool = observer
                    .query_one(
                        "SELECT EXISTS (SELECT FROM pg_catalog.pg_stat_activity WHERE pid = $1)",
                        &[&temp_pid],
                    )
                    .await
                    .unwrap()
                    .get(0);
                if !present {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("temporary backend did not complete ordinary cleanup");
        close(late_reader, late_driver).await;
    }
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(epochs, epochs_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn concurrent_shared_drops_keep_reader_admission_closed_until_native_busy_error() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (first_writer, first_driver) = client().await;
    let (second_writer, second_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let (late_reader, late_driver) = client().await;
    let (barrier, barrier_driver) = client().await;
    // Database utilities each require their own native top-level request.
    first_writer
        .batch_execute("CREATE DATABASE darmok_publication_drop_first")
        .await
        .unwrap();
    first_writer
        .batch_execute("CREATE DATABASE darmok_publication_drop_busy")
        .await
        .unwrap();
    let (temporary, temporary_driver) = database_client("darmok_publication_drop_first").await;
    temporary
        .batch_execute("CREATE TEMP TABLE publication_drop_temp(id integer)")
        .await
        .unwrap();
    let (busy, busy_driver) = database_client("darmok_publication_drop_busy").await;
    let first_pid = pid(&first_writer).await;
    let second_pid = pid(&second_writer).await;
    let late_pid = pid(&late_reader).await;
    // This ordinary advisory holder blocks the test probe only after the real
    // second drop has registered its intent and released all module fences.
    // Future polling controls reply consumption, not native backend progress.
    barrier
        .batch_execute("BEGIN; SELECT pg_catalog.pg_advisory_xact_lock(17485,21316)")
        .await
        .unwrap();
    second_writer
        .batch_execute("SET darmok_catalog_probe.shared_drop_barrier = on")
        .await
        .unwrap();
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    late_reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let before = observe(&epochs).await;
    hold_probe(&reader).await;
    let mut first = Box::pin(complete_drop(
        &first_writer,
        "DROP DATABASE darmok_publication_drop_first",
    ));
    tokio::select! {
        result = &mut first => panic!("first drop escaped the held native test probe: {result:?}"),
        () = wait_fence(&observer, first_pid, "ExclusiveLock", false) => {}
    }
    let mut second = Box::pin(complete_drop(
        &second_writer,
        "DROP DATABASE darmok_publication_drop_busy",
    ));
    tokio::select! {
        result = &mut second => panic!("second drop escaped the held native test probe: {result:?}"),
        () = wait_fence(&observer, second_pid, "ExclusiveLock", false) => {}
    }
    close(temporary, temporary_driver).await;
    let mut acquisition = Box::pin(receive_observation(&late_reader));
    tokio::select! {
        result = &mut acquisition => panic!("reader bypassed shared-drop admission: {result:?}"),
        () = wait_reader_admission(&observer, late_pid, "both drop intents pending") => {}
    }
    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    wait_shared_drop_barrier(&observer, second_pid).await;
    tokio::time::timeout(DATABASE_DROP_COMPLETION, &mut first)
        .await
        .unwrap()
        .unwrap();
    drop(first);
    no_fence(&observer, second_pid).await;
    // The first native transaction's commit must not clear the second intent.
    wait_reader_admission(&observer, late_pid, "second drop after first commit").await;
    // Release the explicit native barrier after observing the still-live second
    // intent. Its existing busy-database error clears that intent and wakes the
    // reader; their two replies still have no required ordering.
    barrier.batch_execute("ROLLBACK").await.unwrap();
    let error = tokio::time::timeout(DATABASE_DROP_COMPLETION, &mut second)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::OBJECT_IN_USE));
    drop(second);
    let advisory_remains: bool = observer
        .query_one(
            "SELECT EXISTS (SELECT FROM pg_catalog.pg_locks WHERE locktype='advisory' AND pid=$1)",
            &[&second_pid],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!advisory_remains);
    let after = tokio::time::timeout(Duration::from_secs(20), &mut acquisition)
        .await
        .unwrap();
    drop(acquisition);
    assert!(after.generation() > before.generation());

    let exists: bool = late_reader.query_one(
        "SELECT EXISTS (SELECT FROM pg_catalog.pg_database WHERE datname = 'darmok_publication_drop_busy')", &[]
    ).await.unwrap().get(0);
    assert!(exists);
    no_fence(&observer, late_pid).await;
    late_reader.batch_execute("COMMIT").await.unwrap();
    close(busy, busy_driver).await;
    second_writer
        .batch_execute("SET darmok_catalog_probe.shared_drop_barrier = off")
        .await
        .unwrap();
    second_writer
        .batch_execute("DROP DATABASE darmok_publication_drop_busy")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(first_writer, first_driver).await;
    close(second_writer, second_driver).await;
    close(observer, observer_driver).await;
    close(epochs, epochs_driver).await;
    close(late_reader, late_driver).await;
    close(barrier, barrier_driver).await;
}

#[tokio::test]
async fn ddl_in_an_uninstalled_database_uses_the_cluster_fence() {
    let _serial = TEST_SERIAL.lock().await;
    let (epochs, epochs_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = database_client("postgres").await;
    let other_oid: u32 = writer
        .query_one(
            "SELECT oid FROM pg_catalog.pg_database WHERE datname = current_database()",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let installed: i64 = writer
        .query_one(
            "SELECT count(*) FROM pg_catalog.pg_extension WHERE extname = 'darmok_server'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        installed, 0,
        "the secondary physical database must be an uninstalled fixture"
    );
    reader.batch_execute("BEGIN").await.unwrap();
    let before = observe(&epochs).await;
    hold_probe(&reader).await;
    assert_ne!(before.database_oid(), other_oid);
    let writer_pid = pid(&writer).await;
    let mut creating = Box::pin(writer.batch_execute("CREATE SCHEMA publication_other_database"));
    tokio::select! {
        result = &mut creating => panic!("another database escaped the cluster fence: {result:?}"),
        () = wait_fence(&reader, writer_pid, "ExclusiveLock", false) => {}
    }

    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut creating)
        .await
        .unwrap()
        .unwrap();
    drop(creating);
    reader.batch_execute("BEGIN").await.unwrap();
    let after = observe(&epochs).await;
    hold_probe(&reader).await;
    assert_eq!(before.cluster_id(), after.cluster_id());
    assert_eq!(before.database_oid(), after.database_oid());
    assert!(after.generation() > before.generation());
    release_probe(&reader).await;
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("DROP SCHEMA publication_other_database")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(epochs, epochs_driver).await;
    close(writer, writer_driver).await;
}
