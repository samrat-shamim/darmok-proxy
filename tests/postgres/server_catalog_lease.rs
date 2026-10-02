//! Ordinary server-module correctness fixtures. No frontend SQL support is
//! inferred from these native observations. A missing module is a test failure.
use std::time::{Duration, Instant};
use tokio_postgres::{Client, NoTls, error::SqlState};

static TEST_SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const BEGIN_LEASE: &str = "SELECT * FROM darmok_server.begin_catalog_lease()";
const END_LEASE: &str = "SELECT darmok_server.end_catalog_lease($1, $2)";
const CHECK_LEASE: &str = "SELECT darmok_server.check_catalog_lease($1, $2)";
const FENCE_LOCKS: &str = "SELECT mode, granted FROM pg_catalog.pg_locks
    WHERE locktype = 'object' AND COALESCE(database, 0) = 0
      AND classid = 'pg_catalog.pg_extension'::pg_catalog.regclass
      AND objid = 0 AND objsubid = 17485 AND pid = $1";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    lease_id: i64,
    cluster_id: Vec<u8>,
    database_oid: u32,
    backend_id: i64,
    generation: i64,
    local_generation: i64,
}

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
    (client, driver)
}

async fn other_database_client() -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL").unwrap();
    let mut config: tokio_postgres::Config = url.parse().unwrap();
    config.dbname("postgres");
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

async fn begin(client: &Client) -> Stamp {
    let row = tokio::time::timeout(Duration::from_secs(20), client.query_one(BEGIN_LEASE, &[]))
        .await
        .expect("catalog lease did not become available")
        .unwrap();
    let stamp = Stamp {
        lease_id: row.get(0),
        cluster_id: row.get(1),
        database_oid: row.get(2),
        backend_id: row.get(3),
        generation: row.get(4),
        local_generation: row.get(5),
    };
    assert_eq!(stamp.cluster_id.len(), 16);
    assert!(stamp.cluster_id.iter().any(|byte| *byte != 0));
    assert!(stamp.lease_id > 0 && stamp.backend_id > 0);
    assert!(stamp.database_oid > 0 && stamp.generation > 0 && stamp.local_generation > 0);
    stamp
}

async fn end(client: &Client, stamp: &Stamp) {
    client
        .query_one(END_LEASE, &[&stamp.lease_id, &stamp.backend_id])
        .await
        .unwrap();
}

async fn check(client: &Client, stamp: &Stamp) {
    client
        .query_one(CHECK_LEASE, &[&stamp.lease_id, &stamp.backend_id])
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
            .query(FENCE_LOCKS, &[&pid])
            .await
            .unwrap()
            .is_empty()
    );
}

async fn expect_lease_error(client: &Client, sql: &str, id: Option<i64>, backend: Option<i64>) {
    client
        .batch_execute("SAVEPOINT expected_lease_error")
        .await
        .unwrap();
    let error = client.query_one(sql, &[&id, &backend]).await.unwrap_err();
    assert_eq!(
        error.code(),
        Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE)
    );
    client
        .batch_execute("ROLLBACK TO expected_lease_error; RELEASE expected_lease_error")
        .await
        .unwrap();
}

#[tokio::test]
async fn lease_identity_requires_a_transaction_and_checked_lifecycle() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (other, other_driver) = client().await;
    let reader_pid = pid(&reader).await;
    let error = reader.query_one(BEGIN_LEASE, &[]).await.unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::NO_ACTIVE_SQL_TRANSACTION));
    no_fence(&other, reader_pid).await;
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    other.batch_execute("BEGIN READ ONLY").await.unwrap();
    let first = begin(&reader).await;
    let foreign = begin(&other).await;
    assert_eq!(first.lease_id, foreign.lease_id);
    assert_eq!(first.cluster_id, foreign.cluster_id);
    assert_eq!(first.database_oid, foreign.database_oid);
    assert_ne!(first.backend_id, foreign.backend_id);
    check(&reader, &first).await;
    reader.batch_execute("SAVEPOINT duplicate").await.unwrap();
    let error = reader.query_one(BEGIN_LEASE, &[]).await.unwrap_err();
    assert_eq!(
        error.code(),
        Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE)
    );
    reader
        .batch_execute("ROLLBACK TO duplicate; RELEASE duplicate")
        .await
        .unwrap();
    check(&reader, &first).await;
    wait_fence(&other, reader_pid, "ShareLock", true).await;
    for id in [None, Some(0), Some(-1), Some(first.lease_id + 1)] {
        expect_lease_error(&reader, END_LEASE, id, Some(first.backend_id)).await;
    }
    expect_lease_error(&reader, END_LEASE, Some(first.lease_id), None).await;
    expect_lease_error(
        &reader,
        END_LEASE,
        Some(foreign.lease_id),
        Some(foreign.backend_id),
    )
    .await;
    // Metadata-free errors do not invalidate catalog facts or release a lease
    // inherited from the parent transaction.
    check(&reader, &first).await;
    end(&reader, &first).await;
    no_fence(&other, reader_pid).await;
    expect_lease_error(
        &reader,
        END_LEASE,
        Some(first.lease_id),
        Some(first.backend_id),
    )
    .await;
    let next = begin(&reader).await;
    assert!(next.lease_id > first.lease_id);
    assert_eq!(next.local_generation, first.local_generation);
    check(&reader, &next).await;
    end(&reader, &next).await;
    end(&other, &foreign).await;
    reader.batch_execute("COMMIT").await.unwrap();
    other.batch_execute("ROLLBACK").await.unwrap();
    close(reader, reader_driver).await;
    close(other, other_driver).await;
}

#[tokio::test]
async fn implicit_simple_request_lease_expires_at_request_commit() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let reader_pid = pid(&reader).await;
    let handle = |messages: &[tokio_postgres::SimpleQueryMessage]| {
        messages
            .iter()
            .find_map(|message| match message {
                tokio_postgres::SimpleQueryMessage::Row(row) if row.len() == 6 => Some((
                    row.get(0).unwrap().parse::<i64>().unwrap(),
                    row.get(3).unwrap().parse::<i64>().unwrap(),
                )),
                _ => None,
            })
            .expect("the request must return the actual lease handle")
    };
    let messages = reader
        .simple_query("SELECT * FROM darmok_server.begin_catalog_lease(); SELECT 1")
        .await
        .unwrap();
    let (expired_id, backend) = handle(&messages);
    no_fence(&observer, reader_pid).await;
    let error = reader
        .query_one(CHECK_LEASE, &[&expired_id, &backend])
        .await
        .unwrap_err();
    assert_eq!(
        error.code(),
        Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE)
    );
    // BEGIN within a multi-statement request promotes the native block to an
    // explicit one. That lease remains available to the next protocol request.
    let messages = reader
        .simple_query(
            "BEGIN READ ONLY; SELECT * FROM darmok_server.begin_catalog_lease(); SELECT 1",
        )
        .await
        .unwrap();
    let (live_id, same_backend) = handle(&messages);
    assert!(live_id > expired_id);
    assert_eq!(same_backend, backend);
    wait_fence(&observer, reader_pid, "ShareLock", true).await;
    reader
        .query_one(CHECK_LEASE, &[&live_id, &backend])
        .await
        .unwrap();
    reader
        .query_one(END_LEASE, &[&live_id, &backend])
        .await
        .unwrap();
    reader.batch_execute("COMMIT").await.unwrap();
    no_fence(&observer, reader_pid).await;
    close(reader, reader_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn readers_coexist_with_dml_and_fence_external_metadata_variants() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute(
            "CREATE SCHEMA lease_variants;
             CREATE TABLE lease_variants.items(id integer PRIMARY KEY, value integer);
             INSERT INTO lease_variants.items VALUES (1, 1);
             CREATE FUNCTION lease_variants.answer() RETURNS integer LANGUAGE SQL AS 'SELECT 1';
             CREATE TYPE lease_variants.mood AS ENUM ('one');
             CREATE DOMAIN lease_variants.code AS integer;",
        )
        .await
        .unwrap();
    let writer_pid = pid(&writer).await;
    let variants = [
        "ALTER TABLE lease_variants.items ADD COLUMN added bigint",
        "ALTER TABLE lease_variants.items ALTER COLUMN added TYPE numeric(8,2)",
        "CREATE OR REPLACE FUNCTION lease_variants.answer() RETURNS integer LANGUAGE SQL AS 'SELECT 2'",
        "ALTER TYPE lease_variants.mood ADD VALUE 'two'",
        "ALTER DOMAIN lease_variants.code SET NOT NULL",
        "CREATE COLLATION lease_variants.exact FROM pg_catalog.\"C\"",
        "COMMENT ON TABLE lease_variants.items IS 'changed metadata'",
        "DROP TABLE lease_variants.items; CREATE TABLE lease_variants.items(id integer)",
        "ALTER SCHEMA lease_variants RENAME TO lease_variants_renamed",
    ];
    for (index, sql) in variants.iter().enumerate() {
        if index == 0 {
            writer
                .batch_execute("UPDATE lease_variants.items SET value = 2 WHERE id = 1")
                .await
                .unwrap();
            let value: i32 = reader
                .query_one("SELECT value FROM lease_variants.items WHERE id = 1", &[])
                .await
                .unwrap()
                .get(0);
            assert_eq!(value, 2);
        }
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        observer.batch_execute("BEGIN READ ONLY").await.unwrap();
        let before = begin(&reader).await;
        let second = begin(&observer).await;
        assert_eq!(before.generation, second.generation);
        let ddl = writer.batch_execute(sql);
        tokio::pin!(ddl);
        tokio::select! {
            result = &mut ddl => panic!("DDL escaped its catalog fence: {result:?}"),
            () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
        }
        check(&reader, &before).await;
        check(&observer, &second).await;
        end(&reader, &before).await;
        reader.batch_execute("COMMIT").await.unwrap();
        // The second reader still owns a fence, so releasing one cannot admit DDL.
        wait_fence(&observer, writer_pid, "ExclusiveLock", false).await;
        end(&observer, &second).await;
        observer.batch_execute("COMMIT").await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), &mut ddl)
            .await
            .unwrap()
            .unwrap();
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let after = begin(&reader).await;
        assert!(after.generation > before.generation, "{sql}");
        check(&reader, &after).await;
        end(&reader, &after).await;
        reader.batch_execute("COMMIT").await.unwrap();
        no_fence(&observer, writer_pid).await;
    }
    writer
        .batch_execute("DROP SCHEMA lease_variants_renamed CASCADE")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn savepoint_rollback_expires_child_lease_and_release_promotes_ownership() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let reader_pid = pid(&reader).await;
    reader
        .batch_execute("BEGIN; SAVEPOINT child")
        .await
        .unwrap();
    let promoted = begin(&reader).await;
    reader.batch_execute("RELEASE child").await.unwrap();
    check(&reader, &promoted).await;
    end(&reader, &promoted).await;
    no_fence(&observer, reader_pid).await;
    reader.batch_execute("SAVEPOINT child").await.unwrap();
    let aborted = begin(&reader).await;
    reader
        .batch_execute("ROLLBACK TO child; RELEASE child")
        .await
        .unwrap();
    no_fence(&observer, reader_pid).await;
    expect_lease_error(
        &reader,
        CHECK_LEASE,
        Some(aborted.lease_id),
        Some(aborted.backend_id),
    )
    .await;
    let recovered = begin(&reader).await;
    assert!(recovered.lease_id > aborted.lease_id);
    assert_eq!(recovered.local_generation, aborted.local_generation);
    check(&reader, &recovered).await;
    end(&reader, &recovered).await;
    reader.batch_execute("COMMIT").await.unwrap();
    no_fence(&observer, reader_pid).await;
    close(reader, reader_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn native_commit_and_rollback_release_unfinished_leases() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let reader_pid = pid(&reader).await;
    let mut previous = None;
    for outcome in ["COMMIT", "ROLLBACK", "COMMIT"] {
        reader.batch_execute("BEGIN").await.unwrap();
        let active = begin(&reader).await;
        if let Some(old) = previous {
            assert_eq!(active.local_generation, old);
        }
        reader.batch_execute(outcome).await.unwrap();
        no_fence(&observer, reader_pid).await;
        reader.batch_execute("BEGIN").await.unwrap();
        expect_lease_error(
            &reader,
            CHECK_LEASE,
            Some(active.lease_id),
            Some(active.backend_id),
        )
        .await;
        reader.batch_execute("ROLLBACK").await.unwrap();
        previous = Some(active.local_generation);
    }
    close(reader, reader_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn ordinary_ddl_fences_publication_after_native_locks_and_effects() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (observer, observer_driver) = client().await;
    let writer_pid = pid(&writer).await;
    writer
        .batch_execute("CREATE TABLE lease_writer(id integer)")
        .await
        .unwrap();
    reader.batch_execute("BEGIN").await.unwrap();
    let before = begin(&reader).await;
    end(&reader, &before).await;
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("BEGIN; SAVEPOINT child; ALTER TABLE lease_writer ADD COLUMN hidden integer; RELEASE child")
        .await
        .unwrap();
    no_fence(&observer, writer_pid).await;
    reader.batch_execute("BEGIN").await.unwrap();
    let during = begin(&reader).await;
    assert_eq!(during.generation, before.generation);
    let columns: i64 = reader
        .query_one("SELECT count(*) FROM pg_catalog.pg_attribute WHERE attrelid = 'lease_writer'::pg_catalog.regclass AND attnum > 0 AND NOT attisdropped", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(columns, 1);
    let mut committing = Box::pin(writer.batch_execute("COMMIT"));
    tokio::select! {
        result = &mut committing => panic!("DDL published through a held catalog lease: {result:?}"),
        () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
    }
    check(&reader, &during).await;
    end(&reader, &during).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut committing)
        .await
        .unwrap()
        .unwrap();
    drop(committing);
    reader.batch_execute("BEGIN").await.unwrap();
    let after = begin(&reader).await;
    assert!(after.generation > before.generation);
    end(&reader, &after).await;
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("BEGIN; SAVEPOINT bad_ddl")
        .await
        .unwrap();
    let error = writer
        .batch_execute("ALTER TABLE lease_writer ADD COLUMN id integer")
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::DUPLICATE_COLUMN));
    writer
        .batch_execute("ROLLBACK TO bad_ddl; RELEASE bad_ddl")
        .await
        .unwrap();
    no_fence(&observer, writer_pid).await;
    // The recovered parent can acquire, check and end a lease.
    let recovered = begin(&writer).await;
    check(&writer, &recovered).await;
    end(&writer, &recovered).await;
    writer
        .batch_execute("COMMIT; DROP TABLE lease_writer")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn own_metadata_mutation_requires_ending_the_lease_first() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (observer, observer_driver) = client().await;
    reader
        .batch_execute("CREATE TABLE lease_own(id integer); BEGIN")
        .await
        .unwrap();
    let active = begin(&reader).await;
    reader.batch_execute("SAVEPOINT own_ddl").await.unwrap();
    let error = reader
        .batch_execute("ALTER TABLE lease_own ADD COLUMN nope integer")
        .await
        .unwrap_err();
    assert_eq!(
        error.code(),
        Some(&SqlState::OBJECT_NOT_IN_PREREQUISITE_STATE)
    );
    reader
        .batch_execute("ROLLBACK TO own_ddl; RELEASE own_ddl")
        .await
        .unwrap();
    end(&reader, &active).await;
    reader
        .batch_execute("ALTER TABLE lease_own ADD COLUMN yes integer")
        .await
        .unwrap();
    let changed = begin(&reader).await;
    assert_eq!(changed.generation, active.generation);
    assert!(changed.local_generation > active.local_generation);
    check(&reader, &changed).await;
    end(&reader, &changed).await;
    reader.batch_execute("COMMIT").await.unwrap();
    let columns: i64 = observer
        .query_one("SELECT count(*) FROM pg_catalog.pg_attribute WHERE attrelid = 'lease_own'::pg_catalog.regclass AND attnum > 0 AND NOT attisdropped", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(columns, 2);
    reader.batch_execute("DROP TABLE lease_own").await.unwrap();
    close(reader, reader_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn concurrent_index_fences_publication_without_blocking_old_snapshots() {
    let _serial = TEST_SERIAL.lock().await;
    let (blocker, blocker_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute("CREATE TABLE lease_index(id integer PRIMARY KEY, value integer); INSERT INTO lease_index VALUES (1, 1)")
        .await
        .unwrap();
    blocker
        .batch_execute("BEGIN; UPDATE lease_index SET value = 2 WHERE id = 1")
        .await
        .unwrap();
    let writer_pid = pid(&writer).await;
    let reader_pid = pid(&reader).await;
    let mut building = Box::pin(
        writer.batch_execute("CREATE INDEX CONCURRENTLY lease_index_value ON lease_index(value)"),
    );
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
    // reader waiting to acquire its own lease.
    no_fence(&observer, writer_pid).await;
    reader.batch_execute("BEGIN").await.unwrap();
    let intermediate = begin(&reader).await;
    check(&reader, &intermediate).await;
    blocker.batch_execute("COMMIT").await.unwrap();
    tokio::select! {
        result = &mut building => panic!("index published its next phase through a held lease: {result:?}"),
        () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
    }
    wait_fence(&observer, reader_pid, "ShareLock", true).await;
    check(&reader, &intermediate).await;
    end(&reader, &intermediate).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut building)
        .await
        .unwrap()
        .unwrap();
    drop(building);
    reader.batch_execute("BEGIN").await.unwrap();
    let stamp = begin(&reader).await;
    assert!(stamp.generation > intermediate.generation);
    check(&reader, &stamp).await;
    let valid: bool = reader
        .query_one("SELECT indisvalid FROM pg_catalog.pg_index WHERE indexrelid = 'lease_index_value'::pg_catalog.regclass", &[])
        .await
        .unwrap()
        .get(0);
    assert!(valid);
    end(&reader, &stamp).await;
    reader.batch_execute("COMMIT").await.unwrap();
    no_fence(&observer, writer_pid).await;
    writer
        .batch_execute("DROP TABLE lease_index")
        .await
        .unwrap();
    close(blocker, blocker_driver).await;
    close(writer, writer_driver).await;
    close(reader, reader_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn concurrent_index_final_valid_transition_has_distinct_generation() {
    let _serial = TEST_SERIAL.lock().await;
    let (blocker, blocker_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (reader, reader_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute("CREATE TABLE lease_index_final(id integer PRIMARY KEY, value integer); INSERT INTO lease_index_final VALUES (1, 1)")
        .await
        .unwrap();
    blocker
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .unwrap();
    assert_eq!(
        blocker
            .query_one("SELECT value FROM lease_index_final WHERE id = 1", &[])
            .await
            .unwrap()
            .get::<_, i32>(0),
        1
    );
    let writer_pid = pid(&writer).await;
    let mut building = Box::pin(writer.batch_execute(
        "CREATE INDEX CONCURRENTLY lease_index_final_value ON lease_index_final(value)",
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
    let intermediate = begin(&reader).await;
    let flags_sql = "SELECT indisready, indisvalid FROM pg_catalog.pg_index
        WHERE indexrelid = 'lease_index_final_value'::pg_catalog.regclass";
    let flags = reader.query_one(flags_sql, &[]).await.unwrap();
    assert!(flags.get::<_, bool>(0));
    assert!(!flags.get::<_, bool>(1));
    // A cache can contain the committed ready-but-invalid index at this stamp.
    // The final publication must wait for the lease and assign a new stamp.
    blocker.batch_execute("COMMIT").await.unwrap();
    tokio::select! {
        result = &mut building => panic!("index published validity through a held lease: {result:?}"),
        () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
    }
    check(&reader, &intermediate).await;
    assert!(
        !reader
            .query_one(flags_sql, &[])
            .await
            .unwrap()
            .get::<_, bool>(1)
    );
    end(&reader, &intermediate).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut building)
        .await
        .unwrap()
        .unwrap();
    drop(building);
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let published = begin(&reader).await;
    check(&reader, &published).await;
    let final_flags = reader.query_one(flags_sql, &[]).await.unwrap();
    let ready: bool = final_flags.get(0);
    let valid: bool = final_flags.get(1);
    end(&reader, &published).await;
    reader.batch_execute("COMMIT").await.unwrap();
    no_fence(&observer, writer_pid).await;
    writer
        .batch_execute("DROP TABLE lease_index_final")
        .await
        .unwrap();
    close(blocker, blocker_driver).await;
    close(writer, writer_driver).await;
    close(reader, reader_driver).await;
    close(observer, observer_driver).await;
    eprintln!(
        "concurrent index validity stamps: invalid={}, valid={}, indisready={ready}, indisvalid={valid}",
        intermediate.generation, published.generation
    );
    assert!(ready && valid);
    assert!(
        published.generation > intermediate.generation,
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

async fn prepared_marker_modes(observer: &Client, gid: &str) -> Vec<String> {
    observer
        .query(
            "SELECT l.mode FROM pg_catalog.pg_locks l JOIN pg_catalog.pg_prepared_xacts p
         ON l.objid::text = p.transaction::text
         WHERE l.locktype = 'object' AND l.classid = 'pg_catalog.pg_extension'::pg_catalog.regclass
         AND l.objsubid = 17486 AND l.pid IS NULL AND l.granted AND p.gid = $1
         ORDER BY l.mode",
            &[&gid],
        )
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get(0))
        .collect()
}

#[tokio::test]
async fn default_native_two_phase_setting_also_allows_catalog_leases() {
    let _serial = TEST_SERIAL.lock().await;
    let url = std::env::var("DARMOK_TEST_NO_TWO_PHASE_DATABASE_URL")
        .expect("DARMOK_TEST_NO_TWO_PHASE_DATABASE_URL must name a disposable module server with native 2PC disabled");
    let (reader, driver) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    let reader_driver = tokio::spawn(driver);
    let enabled: i32 = reader
        .query_one(
            "SELECT current_setting('max_prepared_transactions')::integer",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(enabled, 0);
    reader.batch_execute("BEGIN").await.unwrap();
    let stamp = begin(&reader).await;
    check(&reader, &stamp).await;
    end(&reader, &stamp).await;
    reader.batch_execute("COMMIT").await.unwrap();
    close(reader, reader_driver).await;
}

#[tokio::test]
async fn prepared_dml_and_rolled_back_child_ddl_do_not_fence_catalog_readers() {
    let _serial = TEST_SERIAL.lock().await;
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
    writer.batch_execute("CREATE TABLE lease_prepared_dml(id integer PRIMARY KEY, value integer); INSERT INTO lease_prepared_dml VALUES (1, 1)").await.unwrap();
    let rows_pid = pid(&rows).await;
    for (index, outcome) in ["COMMIT", "ROLLBACK", "COMMIT", "ROLLBACK"]
        .iter()
        .enumerate()
    {
        let child = if index >= 2 {
            "SAVEPOINT child; ALTER TABLE lease_prepared_dml ADD COLUMN rolled_back integer; ROLLBACK TO child; RELEASE child;"
        } else {
            ""
        };
        writer.batch_execute(&format!(
            "BEGIN; {child} UPDATE lease_prepared_dml SET value = value + 10; PREPARE TRANSACTION 'darmok_prepared_dml'"
        )).await.unwrap();
        assert_eq!(
            prepared_marker_modes(&observer, "darmok_prepared_dml").await,
            ["AccessShareLock"]
        );
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let stamp = begin(&reader).await;
        rows.batch_execute("BEGIN; LOCK TABLE lease_prepared_dml IN ROW EXCLUSIVE MODE")
            .await
            .unwrap();
        let mut updating =
            Box::pin(rows.batch_execute("UPDATE lease_prepared_dml SET value = value + 1"));
        tokio::select! {
            result = &mut updating => panic!("row update did not wait for the prepared native transaction: {result:?}"),
            () = wait_native_lock(&observer, rows_pid, "UPDATE lease_prepared_dml") => {}
        }
        // This row waiter owns no catalog lease. The separate catalog reader
        // still owns one, and metadata-free prepared completion does not wait.
        tokio::time::timeout(
            Duration::from_secs(20),
            writer.batch_execute(&format!("{outcome} PREPARED 'darmok_prepared_dml'")),
        )
        .await
        .unwrap()
        .unwrap();
        check(&reader, &stamp).await;
        tokio::time::timeout(Duration::from_secs(20), &mut updating)
            .await
            .unwrap()
            .unwrap();
        drop(updating);
        rows.batch_execute("COMMIT").await.unwrap();
        end(&reader, &stamp).await;
        reader.batch_execute("COMMIT; BEGIN").await.unwrap();
        let after = begin(&reader).await;
        assert_eq!(after.generation, stamp.generation);
        end(&reader, &after).await;
        reader.batch_execute("COMMIT").await.unwrap();
        assert!(
            prepared_marker_modes(&observer, "darmok_prepared_dml")
                .await
                .is_empty()
        );
    }
    let value: i32 = rows
        .query_one("SELECT value FROM lease_prepared_dml", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(value, 25);
    writer
        .batch_execute("DROP TABLE lease_prepared_dml")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(rows, rows_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn prepared_ddl_fences_completion_and_native_guards_wait_outside_the_lease() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (guarded, guarded_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute("CREATE TABLE lease_prepared_ddl(id integer)")
        .await
        .unwrap();
    let writer_pid = pid(&writer).await;
    let guarded_pid = pid(&guarded).await;
    for (index, outcome) in ["COMMIT", "ROLLBACK"].iter().enumerate() {
        writer.batch_execute(&format!(
            "BEGIN; ALTER TABLE lease_prepared_ddl ADD COLUMN change_{index} integer; PREPARE TRANSACTION 'darmok_prepared_ddl'"
        )).await.unwrap();
        assert_eq!(
            prepared_marker_modes(&observer, "darmok_prepared_ddl").await,
            ["AccessShareLock", "RowExclusiveLock"]
        );
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        let before = begin(&reader).await;
        let old_columns: i64 = reader.query_one(
            "SELECT count(*) FROM pg_catalog.pg_attribute WHERE attrelid = 'lease_prepared_ddl'::pg_catalog.regclass AND attnum > 0 AND NOT attisdropped", &[]
        ).await.unwrap().get(0);
        assert_eq!(old_columns, 1 + index as i64);
        guarded.batch_execute("BEGIN").await.unwrap();
        let mut locking =
            Box::pin(guarded.batch_execute("LOCK TABLE lease_prepared_ddl IN ACCESS SHARE MODE"));
        tokio::select! {
            result = &mut locking => panic!("dependency guard escaped prepared DDL: {result:?}"),
            () = wait_native_lock(&observer, guarded_pid, "LOCK TABLE lease_prepared_ddl") => {}
        }
        let finish_sql = format!("{outcome} PREPARED 'darmok_prepared_ddl'");
        let mut finishing = Box::pin(writer.batch_execute(&finish_sql));
        tokio::select! {
            result = &mut finishing => panic!("prepared metadata escaped its publication fence: {result:?}"),
            () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
        }
        check(&reader, &before).await;
        end(&reader, &before).await;
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
        let validated = begin(&guarded).await;
        assert!(validated.generation > before.generation);
        check(&guarded, &validated).await;
        end(&guarded, &validated).await;
        // Native dependency guard remains; global catalog lease has ended.
        guarded
            .query("SELECT * FROM lease_prepared_ddl", &[])
            .await
            .unwrap();
        guarded.batch_execute("COMMIT").await.unwrap();
        no_fence(&observer, writer_pid).await;
    }
    writer
        .batch_execute("DROP TABLE lease_prepared_ddl")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(guarded, guarded_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn mixed_prepared_metadata_and_unrelated_row_locks_can_finish() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer.batch_execute("CREATE TABLE lease_mixed_catalog(id integer); CREATE TABLE lease_mixed_rows(id integer PRIMARY KEY, value integer); INSERT INTO lease_mixed_rows VALUES (1, 1)").await.unwrap();
    writer.batch_execute("BEGIN; ALTER TABLE lease_mixed_catalog ADD COLUMN changed integer; UPDATE lease_mixed_rows SET value = 10; PREPARE TRANSACTION 'darmok_prepared_mixed'").await.unwrap();
    reader
        .batch_execute("BEGIN; LOCK TABLE lease_mixed_rows IN ROW EXCLUSIVE MODE")
        .await
        .unwrap();
    let stamp = begin(&reader).await;
    check(&reader, &stamp).await;
    end(&reader, &stamp).await;
    let reader_pid = pid(&reader).await;
    let mut updating =
        Box::pin(reader.batch_execute("UPDATE lease_mixed_rows SET value = value + 1"));
    tokio::select! {
        result = &mut updating => panic!("mixed prepared row lock was not observed: {result:?}"),
        () = wait_native_lock(&observer, reader_pid, "UPDATE lease_mixed_rows") => {}
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
        .query_one("SELECT value FROM lease_mixed_rows", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(value, 11);
    writer
        .batch_execute("DROP TABLE lease_mixed_catalog, lease_mixed_rows")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn concurrent_prepared_transactions_and_gid_reuse_keep_exact_classification() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (dml, dml_driver) = client().await;
    let (replacement, replacement_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer.batch_execute("CREATE TABLE lease_gid_catalog(id integer); CREATE TABLE lease_gid_rows(id integer, value integer); INSERT INTO lease_gid_rows VALUES (1, 1)").await.unwrap();
    writer.batch_execute("BEGIN; ALTER TABLE lease_gid_catalog ADD COLUMN changed integer; PREPARE TRANSACTION 'darmok_gid_reused'").await.unwrap();
    dml.batch_execute(
        "BEGIN; UPDATE lease_gid_rows SET value = 10; PREPARE TRANSACTION 'darmok_gid_dml'",
    )
    .await
    .unwrap();
    assert_eq!(
        prepared_marker_modes(&observer, "darmok_gid_reused").await,
        ["AccessShareLock", "RowExclusiveLock"]
    );
    assert_eq!(
        prepared_marker_modes(&observer, "darmok_gid_dml").await,
        ["AccessShareLock"]
    );
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let before = begin(&reader).await;
    let writer_pid = pid(&writer).await;
    let replacement_pid = pid(&replacement).await;
    let mut finishing = Box::pin(writer.batch_execute("COMMIT PREPARED 'darmok_gid_reused'"));
    tokio::select! {
        result = &mut finishing => panic!("prepared metadata escaped its lease: {result:?}"),
        () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
    }
    replacement.batch_execute("BEGIN").await.unwrap();
    let mut replacing =
        Box::pin(replacement.batch_execute("PREPARE TRANSACTION 'darmok_gid_reused'"));
    tokio::select! {
        result = &mut replacing => panic!("GID reused before native completion released its gate: {result:?}"),
        () = async {
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let waiting: bool = observer.query_one(
                        "SELECT EXISTS (SELECT FROM pg_catalog.pg_locks WHERE locktype = 'object' AND classid = 'pg_catalog.pg_extension'::pg_catalog.regclass AND objsubid = 17487 AND pid = $1 AND NOT granted)", &[&replacement_pid]
                    ).await.unwrap().get(0);
                    if waiting { return; }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.expect("GID serialization wait was not observed");
        } => {}
    }
    tokio::time::timeout(
        Duration::from_secs(20),
        dml.batch_execute("COMMIT PREPARED 'darmok_gid_dml'"),
    )
    .await
    .unwrap()
    .unwrap();
    check(&reader, &before).await;
    end(&reader, &before).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut finishing)
        .await
        .unwrap()
        .unwrap();
    drop(finishing);
    tokio::time::timeout(Duration::from_secs(20), &mut replacing)
        .await
        .unwrap()
        .unwrap();
    drop(replacing);
    assert_eq!(
        prepared_marker_modes(&observer, "darmok_gid_reused").await,
        ["AccessShareLock"]
    );
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let after = begin(&reader).await;
    assert!(after.generation > before.generation);
    tokio::time::timeout(
        Duration::from_secs(20),
        replacement.batch_execute("COMMIT PREPARED 'darmok_gid_reused'"),
    )
    .await
    .unwrap()
    .unwrap();
    check(&reader, &after).await;
    end(&reader, &after).await;
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("DROP TABLE lease_gid_catalog, lease_gid_rows")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(dml, dml_driver).await;
    close(replacement, replacement_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn queued_prepare_retains_gid_gate_through_deferred_trigger_and_marker_transfer() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (finisher, finisher_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute(
            "CREATE TABLE lease_prepare_target(id integer);
         CREATE TABLE lease_prepare_events(id integer);
         CREATE FUNCTION lease_prepare_deferred() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
             PERFORM pg_catalog.pg_advisory_xact_lock(708923);
             EXECUTE 'ALTER TABLE lease_prepare_target ADD COLUMN from_deferred integer';
             RETURN NULL;
         END $$;
         CREATE CONSTRAINT TRIGGER lease_prepare_trigger AFTER INSERT ON lease_prepare_events
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION lease_prepare_deferred()",
        )
        .await
        .unwrap();
    observer
        .query_one("SELECT pg_catalog.pg_advisory_lock(708923)", &[])
        .await
        .unwrap();
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let before = begin(&reader).await;
    // PREPARE from an open child must promote the gate to the root as native
    // CommitTransactionCommand drains subtransactions, then deferred triggers.
    writer
        .batch_execute("BEGIN; SAVEPOINT child; INSERT INTO lease_prepare_events VALUES (1)")
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
    let mut finishing = Box::pin(finisher.batch_execute("COMMIT PREPARED 'darmok_queued_prepare'"));
    tokio::select! {
        result = &mut finishing => panic!("finisher passed the absent-GID preparation gate: {result:?}"),
        () = async {
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let waiting: bool = observer.query_one(
                        "SELECT EXISTS (SELECT FROM pg_catalog.pg_locks WHERE locktype = 'object' AND classid = 'pg_catalog.pg_extension'::pg_catalog.regclass AND objsubid = 17487 AND pid = $1 AND NOT granted)", &[&finisher_pid]
                    ).await.unwrap().get(0);
                    if waiting { return; }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.expect("finisher did not wait for actual native preparation");
        } => {}
    }
    observer
        .query_one("SELECT pg_catalog.pg_advisory_unlock(708923)", &[])
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut preparing)
        .await
        .unwrap()
        .unwrap();
    drop(preparing);
    tokio::select! {
        result = &mut finishing => panic!("deferred metadata preparation completed without a fence: {result:?}"),
        () = wait_fence(&observer, finisher_pid, "ExclusiveLock", false) => {}
    }
    assert_eq!(
        prepared_marker_modes(&observer, "darmok_queued_prepare").await,
        ["AccessShareLock", "RowExclusiveLock"]
    );
    check(&reader, &before).await;
    end(&reader, &before).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut finishing)
        .await
        .unwrap()
        .unwrap();
    drop(finishing);
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let after = begin(&reader).await;
    assert!(after.generation > before.generation);
    let columns: i64 = reader.query_one(
        "SELECT count(*) FROM pg_catalog.pg_attribute WHERE attrelid = 'lease_prepare_target'::pg_catalog.regclass AND attnum > 0 AND NOT attisdropped", &[]
    ).await.unwrap().get(0);
    assert_eq!(columns, 2);
    end(&reader, &after).await;
    reader.batch_execute("COMMIT").await.unwrap();
    writer.batch_execute("DROP TABLE lease_prepare_target, lease_prepare_events; DROP FUNCTION lease_prepare_deferred()").await.unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(finisher, finisher_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn preparation_noop_abort_and_native_errors_release_gid_gates() {
    let _serial = TEST_SERIAL.lock().await;
    let (writer, writer_driver) = client().await;
    let (other, other_driver) = client().await;
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
    let gates: i64 = other.query_one(
        "SELECT count(*) FROM pg_catalog.pg_locks WHERE locktype = 'object' AND classid = 'pg_catalog.pg_extension'::pg_catalog.regclass AND objsubid = 17487 AND pid = $1", &[&writer_pid]
    ).await.unwrap().get(0);
    assert_eq!(
        gates, 0,
        "no-op/already-aborted PREPARE leaked a session gate"
    );
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
    let gates: i64 = other.query_one(
        "SELECT count(*) FROM pg_catalog.pg_locks WHERE locktype = 'object' AND classid = 'pg_catalog.pg_extension'::pg_catalog.regclass AND objsubid = 17487 AND pid = $1", &[&writer_pid]
    ).await.unwrap().get(0);
    assert_eq!(
        gates, 0,
        "failed native preparation leaked its session gate"
    );
    other
        .batch_execute("ROLLBACK PREPARED 'darmok_prepare_cleanup'")
        .await
        .unwrap();
    let error = writer
        .batch_execute("COMMIT PREPARED 'darmok_prepare_cleanup'")
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::UNDEFINED_OBJECT));
    writer.batch_execute("BEGIN; PREPARE TRANSACTION 'darmok_prepare_cleanup'; COMMIT PREPARED 'darmok_prepare_cleanup'").await.unwrap();
    close(writer, writer_driver).await;
    close(other, other_driver).await;
}

#[tokio::test]
async fn database_removal_and_normal_temp_backend_exit_both_complete() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = client().await;
    let (temporary, temporary_driver) = client().await;
    let (observer, observer_driver) = client().await;
    writer
        .batch_execute("CREATE DATABASE darmok_lease_drop_barrier")
        .await
        .unwrap();
    temporary
        .batch_execute(
            "CREATE TEMP TABLE lease_exit_temp(id integer); INSERT INTO lease_exit_temp VALUES (1)",
        )
        .await
        .unwrap();
    let temp_pid = pid(&temporary).await;
    let writer_pid = pid(&writer).await;
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let before = begin(&reader).await;
    let mut dropping = Box::pin(writer.batch_execute("DROP DATABASE darmok_lease_drop_barrier"));
    tokio::select! {
        result = &mut dropping => panic!("database removal escaped the catalog lease: {result:?}"),
        () = wait_fence(&observer, writer_pid, "ExclusiveLock", false) => {}
    }
    close(temporary, temporary_driver).await;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let waiting: bool = observer.query_one(
                "SELECT EXISTS (SELECT FROM pg_catalog.pg_stat_activity WHERE pid = $1 AND wait_event = 'Extension')", &[&temp_pid]
            ).await.unwrap().get(0);
            if waiting { return; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("normal temporary backend exit did not reach its publication wait");
    check(&reader, &before).await;
    end(&reader, &before).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut dropping)
        .await
        .unwrap()
        .unwrap();
    drop(dropping);
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
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let after = begin(&reader).await;
    assert!(after.generation > before.generation);
    let remains: bool = reader.query_one(
        "SELECT EXISTS (SELECT FROM pg_catalog.pg_database WHERE datname = 'darmok_lease_drop_barrier')", &[]
    ).await.unwrap().get(0);
    assert!(!remains);
    end(&reader, &after).await;
    reader.batch_execute("COMMIT").await.unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn ddl_in_an_uninstalled_database_uses_the_cluster_fence() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let (writer, writer_driver) = other_database_client().await;
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
    let before = begin(&reader).await;
    assert_ne!(before.database_oid, other_oid);
    let writer_pid = pid(&writer).await;
    let mut creating = Box::pin(writer.batch_execute("CREATE SCHEMA lease_other_database"));
    tokio::select! {
        result = &mut creating => panic!("another database escaped the cluster fence: {result:?}"),
        () = wait_fence(&reader, writer_pid, "ExclusiveLock", false) => {}
    }
    check(&reader, &before).await;
    end(&reader, &before).await;
    reader.batch_execute("COMMIT").await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut creating)
        .await
        .unwrap()
        .unwrap();
    drop(creating);
    reader.batch_execute("BEGIN").await.unwrap();
    let after = begin(&reader).await;
    assert_eq!(before.cluster_id, after.cluster_id);
    assert_eq!(before.database_oid, after.database_oid);
    assert!(after.generation > before.generation);
    end(&reader, &after).await;
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("DROP SCHEMA lease_other_database")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
}

#[tokio::test]
async fn private_catalog_facts_expire_on_rollback_but_survive_commit() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    reader
        .batch_execute("CREATE TABLE lease_private(id integer)")
        .await
        .unwrap();
    reader
        .batch_execute(
            "BEGIN; SAVEPOINT child; ALTER TABLE lease_private ALTER COLUMN id TYPE bigint",
        )
        .await
        .unwrap();
    let private = begin(&reader).await;
    let own_type: u32 = reader
        .query_one("SELECT atttypid FROM pg_catalog.pg_attribute WHERE attrelid = 'lease_private'::pg_catalog.regclass AND attnum = 1", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(own_type, tokio_postgres::types::Type::INT8.oid());
    end(&reader, &private).await;
    reader
        .batch_execute("ROLLBACK TO child; RELEASE child")
        .await
        .unwrap();
    let recovered = begin(&reader).await;
    assert_eq!(private.generation, recovered.generation);
    assert!(recovered.local_generation > private.local_generation);
    let restored_type: u32 = reader
        .query_one("SELECT atttypid FROM pg_catalog.pg_attribute WHERE attrelid = 'lease_private'::pg_catalog.regclass AND attnum = 1", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(restored_type, tokio_postgres::types::Type::INT4.oid());
    end(&reader, &recovered).await;
    reader.batch_execute("COMMIT; BEGIN").await.unwrap();
    let clean = begin(&reader).await;
    assert_eq!(clean.local_generation, recovered.local_generation);
    end(&reader, &clean).await;
    reader
        .batch_execute("ALTER TABLE lease_private ADD COLUMN aborted integer")
        .await
        .unwrap();
    let uncommitted = begin(&reader).await;
    end(&reader, &uncommitted).await;
    reader.batch_execute("ROLLBACK; BEGIN").await.unwrap();
    let rolled_back = begin(&reader).await;
    assert!(rolled_back.local_generation > uncommitted.local_generation);
    end(&reader, &rolled_back).await;
    reader
        .batch_execute("ALTER TABLE lease_private ADD COLUMN committed integer")
        .await
        .unwrap();
    let committed = begin(&reader).await;
    end(&reader, &committed).await;
    reader.batch_execute("COMMIT; BEGIN").await.unwrap();
    let reusable = begin(&reader).await;
    assert!(reusable.generation > committed.generation);
    assert_eq!(reusable.local_generation, committed.local_generation);
    let columns: i64 = reader
        .query_one("SELECT count(*) FROM pg_catalog.pg_attribute WHERE attrelid = 'lease_private'::pg_catalog.regclass AND attnum > 0 AND NOT attisdropped", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(columns, 2);
    end(&reader, &reusable).await;
    reader
        .batch_execute("COMMIT; DROP TABLE lease_private")
        .await
        .unwrap();
    close(reader, reader_driver).await;
}

#[tokio::test]
async fn lease_round_trip_cost_is_reported_for_a_bounded_sequential_fixture() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = client().await;
    let begin_query = reader.prepare(BEGIN_LEASE).await.unwrap();
    let end_query = reader.prepare(END_LEASE).await.unwrap();
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let mut elapsed = Vec::with_capacity(128);
    let mut previous_generation = None;
    for index in 0..160 {
        let start = Instant::now();
        let row = reader.query_one(&begin_query, &[]).await.unwrap();
        let id: i64 = row.get(0);
        let backend: i64 = row.get(3);
        let generation: i64 = row.get(4);
        if let Some(previous) = previous_generation {
            assert_eq!(generation, previous);
        }
        previous_generation = Some(generation);
        reader
            .query_one(&end_query, &[&id, &backend])
            .await
            .unwrap();
        if index >= 32 {
            elapsed.push(start.elapsed().as_nanos());
        }
    }
    elapsed.sort_unstable();
    println!(
        "catalog_lease_cost measured_pairs=128 warmup_pairs=32 round_trips_per_pair=2 p50_ns={} p95_ns={} max_ns={}",
        elapsed[63], elapsed[121], elapsed[127]
    );
    reader.batch_execute("COMMIT").await.unwrap();
    close(reader, reader_driver).await;
}
