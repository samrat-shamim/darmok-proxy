//! One-shot native observations. No execution lease or MySQL read-view policy
//! is inferred from these ordinary PostgreSQL facts.
use std::time::{Duration, Instant};

use darmok_catalog::{
    CatalogObservation, NativeRelationName, decode_catalog_observation, read_native_named_relations,
};
use futures_util::StreamExt;
use tokio_postgres::{
    Client, NoTls, QueryEvent, SimpleQueryEvent, TransactionState, error::SqlState,
};

static TEST_SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const SHOW: &str = "SHOW darmok_server.catalog_request_v1";
const FENCES: &str = "SELECT count(*) FROM pg_catalog.pg_locks WHERE locktype='object' AND COALESCE(database,0)=0 AND classid=3079 AND objid=0 AND objsubid=17485 AND pid=$1";

async fn connect(
    url: Option<String>,
) -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url = url.unwrap_or_else(|| {
        std::env::var("DARMOK_TEST_DATABASE_URL").expect("required module database URL")
    });
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    (client, tokio::spawn(connection))
}

async fn close(client: Client, driver: tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>) {
    drop(client);
    tokio::time::timeout(Duration::from_secs(20), driver)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

fn name<'a>(schema_name: &'a str, relation_name: &'a str) -> NativeRelationName<'a> {
    NativeRelationName {
        schema_name,
        relation_name,
    }
}

fn request(names: &[NativeRelationName<'_>]) -> String {
    let pairs: Vec<_> = names
        .iter()
        .map(|name| [name.schema_name, name.relation_name])
        .collect();
    let json = serde_json::to_string(&pairs)
        .unwrap()
        .replace('\\', "\\\\")
        .replace('\'', "''");
    format!("SET LOCAL darmok_server.catalog_request_v1 = E'{json}'; {SHOW}")
}

async fn batch(
    client: &Client,
    sql: &str,
    names: &[NativeRelationName<'_>],
    tags: &[&str],
    state: TransactionState,
) -> CatalogObservation {
    tokio::time::timeout(Duration::from_secs(20), async {
        let mut events = client.simple_query_events(sql).unwrap();
        let mut observed_tags = Vec::new();
        let mut row = None;
        let mut descriptions = 0;
        let mut ready = None;
        while let Some(event) = events.next().await {
            match event.unwrap() {
                SimpleQueryEvent::CommandComplete(tag) => observed_tags.push(tag),
                SimpleQueryEvent::RowDescription(columns) => {
                    assert_eq!(columns.len(), 1);
                    let column = &columns[0];
                    assert_eq!(column.name(), "darmok_server.catalog_request_v1");
                    assert_eq!(column.type_oid(), 25);
                    assert_eq!(column.type_size(), -1);
                    assert_eq!(column.type_modifier(), -1);
                    assert_eq!(column.format(), 0);
                    assert_eq!(column.table_oid(), None);
                    assert_eq!(column.column_id(), None);
                    descriptions += 1;
                }
                SimpleQueryEvent::Row(value) => {
                    assert!(row.is_none());
                    assert_eq!(value.len(), 1);
                    row = Some(value.get(0).unwrap().to_owned());
                }
                SimpleQueryEvent::ReadyForQuery(value) => {
                    assert!(ready.is_none());
                    ready = Some(value);
                }
                other => panic!("unexpected catalog event: {other:?}"),
            }
        }
        assert_eq!(observed_tags, tags);
        assert_eq!(descriptions, 1);
        assert_eq!(ready, Some(state));
        decode_catalog_observation(&row.unwrap(), names).unwrap()
    })
    .await
    .expect("catalog batch did not complete")
}

async fn read(client: &Client, names: &[NativeRelationName<'_>]) -> CatalogObservation {
    batch(
        client,
        &request(names),
        names,
        &["SET", "SHOW"],
        TransactionState::Transaction,
    )
    .await
}

async fn pid(client: &Client) -> i32 {
    client
        .query_one("SELECT pg_catalog.pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0)
}

async fn no_fence(observer: &Client, backend: i32) {
    assert_eq!(
        observer
            .query_one(FENCES, &[&backend])
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
}

#[tokio::test]
async fn first_rr_and_serializable_data_views_follow_discovery() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = connect(None).await;
    let (writer, writer_driver) = connect(None).await;
    writer.batch_execute("CREATE TABLE discovery_first(id integer, value integer); INSERT INTO discovery_first VALUES(1,1)").await.unwrap();
    let reader_pid = pid(&reader).await;
    for isolation in ["REPEATABLE READ", "SERIALIZABLE"] {
        for access in ["READ WRITE", "READ ONLY"] {
            writer
                .batch_execute("UPDATE discovery_first SET value=1")
                .await
                .unwrap();
            reader
                .batch_execute(&format!("BEGIN ISOLATION LEVEL {isolation} {access}"))
                .await
                .unwrap();
            let observation = read(&reader, &[name("public", "discovery_first")]).await;
            assert_eq!(observation.named_catalog().catalog().relations.len(), 1);
            no_fence(&writer, reader_pid).await;
            writer
                .batch_execute("UPDATE discovery_first SET value=2")
                .await
                .unwrap();
            let value: i32 = reader
                .query_one("SELECT value FROM discovery_first", &[])
                .await
                .unwrap()
                .get(0);
            assert_eq!(
                value, 2,
                "{isolation} {access} acquired its first view too early"
            );
            reader.batch_execute("COMMIT").await.unwrap();
        }
    }
    writer
        .batch_execute("DROP TABLE discovery_first")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
}

#[tokio::test]
async fn existing_rr_view_stays_old_while_metadata_is_current() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = connect(None).await;
    let (writer, writer_driver) = connect(None).await;
    writer.batch_execute("CREATE TABLE discovery_data(value integer); INSERT INTO discovery_data VALUES(1); CREATE TABLE discovery_metadata(id integer)").await.unwrap();
    reader
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .unwrap();
    assert_eq!(
        reader
            .query_one("SELECT value FROM discovery_data", &[])
            .await
            .unwrap()
            .get::<_, i32>(0),
        1
    );
    let names = [name("public", "discovery_metadata")];
    let before = read(&reader, &names).await;
    writer.batch_execute("UPDATE discovery_data SET value=2; ALTER TABLE discovery_metadata ADD COLUMN current_fact bigint").await.unwrap();
    let after = read(&reader, &names).await;
    assert!(after.stamp().generation() > before.stamp().generation());
    let relation =
        &after.named_catalog().catalog().relations[&after.named_catalog().relation_oids()[0]];
    assert_eq!(relation.columns.len(), 2);
    assert_eq!(relation.columns[1].name, "current_fact");
    assert_eq!(
        reader
            .query_one("SELECT value FROM discovery_data", &[])
            .await
            .unwrap()
            .get::<_, i32>(0),
        1
    );
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("DROP TABLE discovery_data, discovery_metadata")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
}

#[tokio::test]
async fn fixed_facts_match_native_names_columns_types_domains_and_kinds() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, driver) = connect(None).await;
    let version: i32 = reader
        .query_one("SELECT current_setting('server_version_num')::integer", &[])
        .await
        .unwrap()
        .get(0);
    reader.batch_execute(r#"BEGIN;
        CREATE SCHEMA "Discovery.'\分析";
        CREATE DOMAIN "Discovery.'\分析".amount AS numeric(8,2) NOT NULL;
        CREATE DOMAIN "Discovery.'\分析".amount_child AS "Discovery.'\分析".amount;
        CREATE TYPE "Discovery.'\分析".mood AS ENUM ('one','two');
        CREATE TYPE "Discovery.'\分析".pair AS (x integer, y text);
        CREATE TABLE "Discovery.'\分析"."Items.Set"(
          id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
          dropped integer, label text COLLATE "C" DEFAULT 'name',
          amount "Discovery.'\分析".amount_child, items integer[][],
          mood "Discovery.'\分析".mood, seed integer,
          generated integer GENERATED ALWAYS AS (seed+1) STORED);
        ALTER TABLE "Discovery.'\分析"."Items.Set" DROP COLUMN dropped;
        CREATE TABLE "Discovery.'\分析".empty();
        CREATE TABLE "Discovery.'\分析".parts(id integer) PARTITION BY RANGE(id);
        CREATE TABLE "Discovery.'\分析".part0 PARTITION OF "Discovery.'\分析".parts FOR VALUES FROM (0) TO (10);
        CREATE VIEW "Discovery.'\分析".view0 AS SELECT id FROM "Discovery.'\分析"."Items.Set";
        CREATE MATERIALIZED VIEW "Discovery.'\分析".materialized AS SELECT 1::integer AS answer WITH NO DATA;
        SET LOCAL standard_conforming_strings=off;"#).await.unwrap();
    if version >= 180000 {
        reader.batch_execute(r#"ALTER TABLE "Discovery.'\分析"."Items.Set" ADD COLUMN virtual_value integer GENERATED ALWAYS AS (seed+2) VIRTUAL"#).await.unwrap();
    }
    let schema = "Discovery.'\\分析";
    let names = [
        name(schema, "Items.Set"),
        name(schema, "empty"),
        name(schema, "Items.Set"),
        name(schema, "parts"),
        name(schema, "part0"),
        name(schema, "view0"),
        name(schema, "materialized"),
        name(schema, "pair"),
        name(schema, "Items.Set_pkey"),
        name(schema, "Items.Set_id_seq"),
    ];
    let observation = read(&reader, &names).await;
    let ordinary = read_native_named_relations(&reader, &names).await.unwrap();
    assert_eq!(observation.named_catalog(), &ordinary);
    assert_eq!(ordinary.relation_oids()[0], ordinary.relation_oids()[2]);
    let columns = &ordinary.catalog().relations[&ordinary.relation_oids()[0]].columns;
    assert_eq!(columns[1].attribute_number, 3);
    assert_eq!(
        ordinary
            .catalog()
            .types
            .values()
            .filter(|t| t.domain.is_some())
            .count(),
        2
    );
    reader.batch_execute("ROLLBACK").await.unwrap();
    close(reader, driver).await;
}

#[tokio::test]
async fn private_savepoint_and_temporary_facts_have_native_ownership() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = connect(None).await;
    let (other, other_driver) = connect(None).await;
    reader.batch_execute("CREATE TABLE discovery_private(id integer); BEGIN; SAVEPOINT child; ALTER TABLE discovery_private ALTER COLUMN id TYPE bigint; CREATE TEMP TABLE discovery_temp(n numeric(7,3))").await.unwrap();
    let temporary: String = reader.query_one("SELECT nspname::text FROM pg_catalog.pg_namespace WHERE oid=pg_catalog.pg_my_temp_schema()", &[]).await.unwrap().get(0);
    let names = [
        name("public", "discovery_private"),
        name(&temporary, "discovery_temp"),
    ];
    let changed = read(&reader, &names).await;
    assert_eq!(
        changed.named_catalog(),
        &read_native_named_relations(&reader, &names).await.unwrap()
    );
    assert_eq!(
        changed.named_catalog().catalog().relations[&changed.named_catalog().relation_oids()[0]]
            .columns[0]
            .declared_type_oid,
        20
    );
    reader
        .batch_execute("ROLLBACK TO child; RELEASE child")
        .await
        .unwrap();
    let recovered = read(&reader, &[names[0]]).await;
    assert!(recovered.stamp().local_generation() > changed.stamp().local_generation());
    assert_eq!(recovered.stamp().generation(), changed.stamp().generation());
    assert_eq!(
        recovered.named_catalog().catalog().relations
            [&recovered.named_catalog().relation_oids()[0]]
            .columns[0]
            .declared_type_oid,
        23
    );
    reader
        .batch_execute("ALTER TABLE discovery_private ADD COLUMN committed integer; COMMIT; BEGIN")
        .await
        .unwrap();
    let committed = read(&reader, &[names[0]]).await;
    assert_eq!(committed.stamp().backend_id(), changed.stamp().backend_id());
    assert!(committed.stamp().generation() > changed.stamp().generation());
    assert_eq!(
        committed.named_catalog().catalog().relations
            [&committed.named_catalog().relation_oids()[0]]
            .columns
            .len(),
        2
    );
    reader
        .batch_execute("ALTER TABLE discovery_private ADD COLUMN aborted integer; ROLLBACK; BEGIN")
        .await
        .unwrap();
    let rolled_back = read(&reader, &[names[0]]).await;
    assert!(rolled_back.stamp().local_generation() > committed.stamp().local_generation());
    reader
        .batch_execute("COMMIT; DROP TABLE discovery_private")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(other, other_driver).await;
}

#[tokio::test]
async fn prepared_catalog_heap_lock_waits_outside_the_global_fence() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = connect(None).await;
    let (writer, writer_driver) = connect(None).await;
    let (observer, observer_driver) = connect(None).await;
    let reader_pid = pid(&reader).await;
    writer
        .batch_execute("CREATE TABLE discovery_catalog_wait(id integer)")
        .await
        .unwrap();
    let waiting = observer.prepare("SELECT EXISTS(SELECT FROM pg_catalog.pg_locks WHERE pid=$1 AND relation=2615 AND mode='AccessShareLock' AND NOT granted)").await.unwrap();
    let fences = observer.prepare(FENCES).await.unwrap();
    observer.query_one(&waiting, &[&reader_pid]).await.unwrap();
    observer.query_one(&fences, &[&reader_pid]).await.unwrap();
    for outcome in ["COMMIT", "ROLLBACK"] {
        reader.batch_execute("BEGIN READ ONLY").await.unwrap();
        writer.batch_execute("BEGIN; LOCK TABLE pg_catalog.pg_namespace IN ACCESS EXCLUSIVE MODE; PREPARE TRANSACTION 'darmok_catalog_heap_lock'").await.unwrap();
        let names = [name("public", "discovery_catalog_wait")];
        let mut reading = Box::pin(read(&reader, &names));
        tokio::select! {
            result = &mut reading => panic!("expected native catalog AS wait: {result:?}"),
            () = async {
                tokio::time::timeout(Duration::from_secs(20), async {
                    loop {
                        if observer.query_one(&waiting, &[&reader_pid]).await.unwrap().get::<_, bool>(0) { break; }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }).await.expect("catalog heap wait was not observed");
            } => {}
        }
        assert_eq!(
            observer
                .query_one(&fences, &[&reader_pid])
                .await
                .unwrap()
                .get::<_, i64>(0),
            0
        );
        tokio::time::timeout(
            Duration::from_secs(20),
            writer.batch_execute(&format!("{outcome} PREPARED 'darmok_catalog_heap_lock'")),
        )
        .await
        .unwrap()
        .unwrap();
        let observation = tokio::time::timeout(Duration::from_secs(20), &mut reading)
            .await
            .unwrap();
        assert_eq!(observation.named_catalog().catalog().relations.len(), 1);
        drop(reading);
        reader.batch_execute("COMMIT").await.unwrap();
        no_fence(&observer, reader_pid).await;
    }
    writer
        .batch_execute("DROP TABLE discovery_catalog_wait")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn prepared_user_ddl_is_observed_before_and_after_native_completion() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = connect(None).await;
    let (writer, writer_driver) = connect(None).await;
    writer.batch_execute("CREATE TABLE discovery_prepared(id integer); BEGIN; ALTER TABLE discovery_prepared ADD COLUMN later bigint; PREPARE TRANSACTION 'darmok_discovery_ddl'").await.unwrap();
    reader
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .unwrap();
    let names = [name("public", "discovery_prepared")];
    let before = read(&reader, &names).await;
    assert_eq!(
        before.named_catalog().catalog().relations[&before.named_catalog().relation_oids()[0]]
            .columns
            .len(),
        1
    );
    tokio::time::timeout(
        Duration::from_secs(20),
        writer.batch_execute("COMMIT PREPARED 'darmok_discovery_ddl'"),
    )
    .await
    .unwrap()
    .unwrap();
    let after = read(&reader, &names).await;
    assert!(after.stamp().generation() > before.stamp().generation());
    assert_eq!(
        after.named_catalog().catalog().relations[&after.named_catalog().relation_oids()[0]]
            .columns
            .len(),
        2
    );
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("DROP TABLE discovery_prepared")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
}

#[tokio::test]
async fn guc_request_restore_errors_limits_and_removed_exports_are_explicit() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = connect(None).await;
    let (observer, observer_driver) = connect(None).await;
    let reader_pid = pid(&reader).await;
    reader
        .batch_execute("CREATE TABLE discovery_guc(id integer); BEGIN")
        .await
        .unwrap();
    let names = [name("public", "discovery_guc")];
    let before = read(&reader, &names).await;
    reader.batch_execute("SAVEPOINT child; SET LOCAL darmok_server.catalog_request_v1='[]'; ROLLBACK TO child; RELEASE child").await.unwrap();
    let restored = batch(
        &reader,
        SHOW,
        &names,
        &["SHOW"],
        TransactionState::Transaction,
    )
    .await;
    assert_eq!(before, restored);
    for json in ["null", "{}", "[[]]", "[[1,2]]", "not-json"] {
        reader.batch_execute("SAVEPOINT bad").await.unwrap();
        let sql = format!("SET LOCAL darmok_server.catalog_request_v1='{json}'; {SHOW}");
        let mut events = reader.simple_query_events(&sql).unwrap();
        let mut error = None;
        let mut ready = None;
        let mut rows = 0;
        while let Some(event) = events.next().await {
            match event.unwrap() {
                SimpleQueryEvent::BackendError(value) => error = Some(value),
                SimpleQueryEvent::ReadyForQuery(value) => ready = Some(value),
                SimpleQueryEvent::Row(_) => rows += 1,
                _ => {}
            }
        }
        assert!(error.is_some());
        assert_eq!(rows, 0);
        assert_eq!(ready, Some(TransactionState::FailedTransaction));
        no_fence(&observer, reader_pid).await;
        reader
            .batch_execute("ROLLBACK TO bad; RELEASE bad")
            .await
            .unwrap();
        assert_eq!(
            batch(
                &reader,
                SHOW,
                &names,
                &["SHOW"],
                TransactionState::Transaction
            )
            .await,
            before
        );
    }
    let overlong = "x".repeat(64);
    for (requested, missing_index) in [
        (vec![name("public", "missing_discovery")], Some(0)),
        (vec![names[0], name("public", &overlong)], Some(1)),
        (
            vec![
                name("public", "missing_discovery"),
                name("missing_schema", "missing"),
            ],
            Some(0),
        ),
        (
            vec![
                names[0],
                name("missing_schema", "missing"),
                name("public", "missing_discovery"),
            ],
            Some(1),
        ),
        (vec![names[0]; 4097], None),
    ] {
        reader.batch_execute("SAVEPOINT missing").await.unwrap();
        let error = reader.simple_query(&request(&requested)).await.unwrap_err();
        assert!(matches!(
            error.code(),
            Some(&SqlState::UNDEFINED_TABLE | &SqlState::PROGRAM_LIMIT_EXCEEDED)
        ));
        if let Some(index) = missing_index {
            assert_eq!(
                error.as_db_error().unwrap().message(),
                format!("catalog relation at request index {index} does not exist")
            );
        }
        no_fence(&observer, reader_pid).await;
        reader
            .batch_execute("ROLLBACK TO missing; RELEASE missing")
            .await
            .unwrap();
    }
    reader.batch_execute("COMMIT").await.unwrap();
    let header = batch(&reader, SHOW, &[], &["SHOW"], TransactionState::Idle).await;
    assert_eq!(header.stamp().backend_id(), before.stamp().backend_id());
    for sql in [
        "SET darmok_server.catalog_request_v2='[]'",
        "SELECT darmok_server.begin_catalog_lease()",
        "SELECT darmok_server.check_catalog_lease(1,1)",
        "SELECT darmok_server.end_catalog_lease(1,1)",
    ] {
        assert!(reader.batch_execute(sql).await.is_err());
        no_fence(&observer, reader_pid).await;
    }
    reader.batch_execute("BEGIN; SHOW darmok_server.catalog_request_v1; PREPARE TRANSACTION 'darmok_after_discovery'").await.unwrap();
    reader
        .batch_execute("COMMIT PREPARED 'darmok_after_discovery'")
        .await
        .unwrap();
    reader
        .batch_execute("DROP TABLE discovery_guc")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(observer, observer_driver).await;
}

#[tokio::test]
async fn describe_does_not_read_and_portal_resume_is_historical() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, reader_driver) = connect(None).await;
    let (writer, writer_driver) = connect(None).await;
    let reader_pid = pid(&reader).await;
    writer.batch_execute("CREATE TABLE discovery_describe(value integer); INSERT INTO discovery_describe VALUES(1)").await.unwrap();
    reader
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .unwrap();
    let statement = reader.prepare(SHOW).await.unwrap();
    let portal = reader
        .bind_described_builtin(&statement, &[])
        .await
        .unwrap();
    assert_eq!(portal.ready_state(), TransactionState::Transaction);
    no_fence(&writer, reader_pid).await;
    writer
        .batch_execute("UPDATE discovery_describe SET value=2")
        .await
        .unwrap();
    let mut events = reader.query_portal_events(&portal, 1).unwrap();
    let mut first = None;
    let mut suspended = false;
    while let Some(event) = events.next().await {
        match event.unwrap() {
            QueryEvent::Row(row) => {
                first = Some(decode_catalog_observation(row.get::<_, &str>(0), &[]).unwrap())
            }
            QueryEvent::PortalSuspended => suspended = true,
            QueryEvent::ReadyForQuery(state) => assert_eq!(state, TransactionState::Transaction),
            other => panic!("unexpected SHOW portal event: {other:?}"),
        }
    }
    assert!(suspended);
    no_fence(&writer, reader_pid).await;
    assert_eq!(
        reader
            .query_one("SELECT value FROM discovery_describe", &[])
            .await
            .unwrap()
            .get::<_, i32>(0),
        2
    );
    writer
        .batch_execute("CREATE TABLE discovery_portal(id integer)")
        .await
        .unwrap();
    let later = batch(&writer, SHOW, &[], &["SHOW"], TransactionState::Idle).await;
    assert!(later.stamp().generation() > first.unwrap().stamp().generation());
    let mut resumed = reader.query_portal_events(&portal, 0).unwrap();
    let mut tag = None;
    while let Some(event) = resumed.next().await {
        match event.unwrap() {
            QueryEvent::CommandComplete(value) => tag = Some(value),
            QueryEvent::ReadyForQuery(state) => assert_eq!(state, TransactionState::Transaction),
            other => panic!("historical SHOW must not rediscover: {other:?}"),
        }
    }
    assert_eq!(tag.as_deref(), Some("SHOW"));
    no_fence(&writer, reader_pid).await;
    drop(portal);
    drop(statement);
    reader.batch_execute("COMMIT").await.unwrap();
    writer
        .batch_execute("DROP TABLE discovery_portal, discovery_describe")
        .await
        .unwrap();
    close(reader, reader_driver).await;
    close(writer, writer_driver).await;
}

#[tokio::test]
async fn default_two_phase_disabled_setting_is_supported_without_requiring_it() {
    let _serial = TEST_SERIAL.lock().await;
    let url = std::env::var("DARMOK_TEST_NO_TWO_PHASE_DATABASE_URL")
        .expect("required independent native default-2PC profile");
    let (reader, driver) = connect(Some(url)).await;
    assert_eq!(
        reader
            .query_one(
                "SELECT current_setting('max_prepared_transactions')::integer",
                &[]
            )
            .await
            .unwrap()
            .get::<_, i32>(0),
        0
    );
    reader.batch_execute("BEGIN READ ONLY").await.unwrap();
    let result = read(&reader, &[]).await;
    assert!(result.named_catalog().catalog().relations.is_empty());
    reader.batch_execute("COMMIT").await.unwrap();
    close(reader, driver).await;
}

#[tokio::test]
async fn per_database_installation_is_required_even_when_the_module_is_preloaded() {
    let _serial = TEST_SERIAL.lock().await;
    let url = std::env::var("DARMOK_TEST_DATABASE_URL").unwrap();
    let mut config: tokio_postgres::Config = url.parse().unwrap();
    config.dbname("postgres");
    let (reader, connection) = config.connect(NoTls).await.unwrap();
    let driver = tokio::spawn(connection);
    assert_eq!(
        reader
            .query_one(
                "SELECT count(*) FROM pg_catalog.pg_extension WHERE extname='darmok_server'",
                &[]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let error = reader.simple_query(SHOW).await.unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::UNDEFINED_OBJECT));
    close(reader, driver).await;
}

#[tokio::test]
async fn bounded_sequential_one_shot_cost_reports_catalog_size() {
    let _serial = TEST_SERIAL.lock().await;
    let (reader, driver) = connect(None).await;
    reader
        .batch_execute("CREATE TABLE discovery_cost(id integer); BEGIN READ ONLY")
        .await
        .unwrap();
    let sizes = reader.query_one("SELECT (SELECT count(*) FROM pg_catalog.pg_namespace), (SELECT count(*) FROM pg_catalog.pg_class), (SELECT count(*) FROM pg_catalog.pg_attribute), (SELECT count(*) FROM pg_catalog.pg_type)", &[]).await.unwrap();
    let names = [name("public", "discovery_cost")];
    for _ in 0..32 {
        read(&reader, &names).await;
    }
    let mut times = Vec::with_capacity(128);
    for _ in 0..128 {
        let start = Instant::now();
        read(&reader, &names).await;
        times.push(start.elapsed().as_nanos());
    }
    times.sort_unstable();
    eprintln!(
        "catalog_discovery_cost requests=128 warmup=32 round_trips_per_request=1 namespace_rows={} class_rows={} attribute_rows={} type_rows={} p50_ns={} p95_ns={} max_ns={}",
        sizes.get::<_, i64>(0),
        sizes.get::<_, i64>(1),
        sizes.get::<_, i64>(2),
        sizes.get::<_, i64>(3),
        times[64],
        times[121],
        times[127]
    );
    reader
        .batch_execute("COMMIT; DROP TABLE discovery_cost")
        .await
        .unwrap();
    close(reader, driver).await;
}
