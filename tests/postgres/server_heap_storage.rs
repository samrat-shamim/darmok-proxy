//! Private catalog-declared storage, not application SQL or full semantic closure.
use futures_util::{FutureExt, StreamExt};
use serde_json::{Value, json};
use std::{panic::AssertUnwindSafe, time::Duration};
use tokio_postgres::{Client, NoTls, SimpleQueryEvent, Statement, error::SqlState};

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const DEADLINE: Duration = Duration::from_secs(20);
const CASE_DEADLINE: Duration = Duration::from_secs(60);

async fn client() -> (
    Client,
    tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>,
) {
    let url =
        std::env::var("DARMOK_TEST_DATABASE_URL").expect("native heap storage profile required");
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
        .expect("bounded native storage SQL did not complete")
}

async fn command(client: &Client, value: &str) -> Result<(), tokio_postgres::Error> {
    sql(
        client,
        &format!(
            "SET darmok_catalog_probe.command='{}'",
            value.replace('\'', "''")
        ),
    )
    .await
}

fn storage_command(roots: &Value, retain: bool) -> String {
    format!(
        "storage_{}:{roots}",
        if retain { "retain" } else { "release" }
    )
}

async fn status(client: &Client) -> Value {
    tokio::time::timeout(DEADLINE, async {
        let mut events = client
            .simple_query_events("SHOW darmok_catalog_probe.storage_status")
            .unwrap();
        let mut result = None;
        let mut descriptions = 0;
        let mut completions = 0;
        let mut ready = false;
        while let Some(event) = events.next().await {
            match event.unwrap() {
                SimpleQueryEvent::RowDescription(columns) => {
                    assert_eq!(columns.len(), 1);
                    assert_eq!(columns[0].name(), "darmok_catalog_probe.storage_status");
                    assert_eq!((columns[0].type_oid(), columns[0].format()), (25, 0));
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
                other => panic!("unexpected native storage status: {other:?}"),
            }
        }
        assert_eq!((descriptions, completions, ready), (1, 1, true));
        result.unwrap()
    })
    .await
    .expect("bounded native storage status did not complete")
}

async fn capture(client: &Client, roots: &Value, retain: bool) -> Value {
    command(client, &storage_command(roots, retain))
        .await
        .unwrap();
    status(client).await
}

fn check_scope(state: &Value, established: bool, retained: bool) {
    assert_eq!(state["calls"], 1);
    assert_eq!(state["retained"], retained);
    for name in ["before_snapshot", "after_snapshot"] {
        assert_eq!(state[name], established, "{name}: {state}");
    }
    let metadata = &state["metadata"];
    assert_eq!(metadata["first_snapshot_set"], established);
    assert_eq!(metadata["physical_owned"], true);
    assert_eq!(metadata["metadata_owned"], true);
    assert!((1..=16).contains(&metadata["attempts"].as_u64().unwrap()));
}

async fn oid(client: &Client, schema: &str, name: &str) -> u32 {
    tokio::time::timeout(DEADLINE, client.query_one(
        "SELECT c.oid FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=$2", &[&schema, &name],
    )).await.unwrap().unwrap().get(0)
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

async fn oracle(client: &Client, root: u32) -> Vec<Value> {
    let rows = tokio::time::timeout(DEADLINE, client.query(
        "WITH heaps AS (SELECT oid,0::oid AS parent_oid,0 AS phase FROM pg_catalog.pg_class WHERE oid=$1 UNION ALL SELECT t.oid,r.oid,2 FROM pg_catalog.pg_class r JOIN pg_catalog.pg_class t ON t.oid=r.reltoastrelid WHERE r.oid=$1), nodes AS (SELECT oid,parent_oid,phase FROM heaps UNION ALL SELECT i.indexrelid,h.oid,h.phase+1 FROM heaps h JOIN pg_catalog.pg_index i ON i.indrelid=h.oid WHERE i.indislive) SELECT pg_catalog.json_build_object('oid',c.oid::bigint,'schema_oid',n.oid::bigint,'schema',n.nspname,'name',c.relname,'kind',pg_catalog.ascii(c.relkind::text),'persistence',pg_catalog.ascii(c.relpersistence::text),'am',c.relam::bigint,'toast_oid',c.reltoastrelid::bigint,'parent_oid',x.parent_oid::bigint,'shared',c.relisshared,'is_partition',c.relispartition,'has_indexes',c.relhasindex,'has_subclasses',c.relhassubclass,'live',COALESCE(i.indislive,false),'ready',COALESCE(i.indisready,false),'valid',COALESCE(i.indisvalid,false),'check_xmin',COALESCE(i.indcheckxmin,false),'tablespace_oid',c.reltablespace::bigint,'stored_file_number',c.relfilenode::bigint,'file_tablespace_oid',COALESCE(NULLIF(c.reltablespace,0),(SELECT dattablespace FROM pg_catalog.pg_database WHERE datname=current_database()))::bigint,'file_database_oid',(CASE WHEN c.relisshared THEN 0::oid ELSE (SELECT oid FROM pg_catalog.pg_database WHERE datname=current_database()) END)::bigint,'file_number',pg_catalog.pg_relation_filenode(c.oid::regclass)::bigint,'file_proc_number',CASE WHEN c.relpersistence='t' THEN substring(pg_catalog.pg_relation_filepath(c.oid::regclass) FROM '/t([0-9]+)_')::integer ELSE -1 END)::text FROM nodes x JOIN pg_catalog.pg_class c ON c.oid=x.oid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace LEFT JOIN pg_catalog.pg_index i ON i.indexrelid=c.oid ORDER BY x.phase,c.oid", &[&root],
    )).await.unwrap().unwrap();
    rows.into_iter()
        .map(|row| serde_json::from_str(&row.get::<_, String>(0)).unwrap())
        .collect()
}

fn check_graph(state: &Value, expected: &[Value], root_mask: u64) {
    let metadata = &state["metadata"];
    let facts = metadata["facts"].as_array().unwrap();
    assert_eq!(facts.len(), expected.len());
    let mut references = Vec::new();
    for (actual, expected) in facts.iter().zip(expected) {
        for (key, value) in expected.as_object().unwrap() {
            assert_eq!(&actual[key], value, "{key}: {actual}");
        }
        let toast = expected["kind"] == u64::from(b't')
            || facts.iter().any(|parent| {
                parent["oid"] == expected["parent_oid"] && parent["kind"] == u64::from(b't')
            });
        let mask = if toast {
            2 | (root_mask & 8)
        } else {
            root_mask
        };
        assert_eq!(actual["mode_mask"], mask);
        for mode in 1..=3 {
            if mask & (1 << mode) != 0 {
                references.push(json!([actual["oid"], mode]));
            }
        }
    }
    assert_eq!(metadata["references"], json!(references));
}

struct Observer {
    locks: Statement,
    coordination: Statement,
    indexes: Statement,
}

impl Observer {
    async fn new(client: &Client) -> Self {
        tokio::time::timeout(DEADLINE, async {
            sql(client, "SET plan_cache_mode='force_generic_plan'").await.unwrap();
            let locks = client.prepare("SELECT relation,COALESCE(database,0),mode,granted FROM pg_catalog.pg_locks WHERE locktype='relation' AND pid IS NOT DISTINCT FROM $1 AND relation=ANY($2) ORDER BY relation,mode").await.unwrap();
            let coordination = client.prepare("SELECT objsubid::integer,mode,granted FROM pg_catalog.pg_locks WHERE locktype='object' AND COALESCE(database,0)=0 AND classid=3079 AND objid=0 AND objsubid IN (17485,17486,17487) AND pid=$1").await.unwrap();
            let indexes = client.prepare("SELECT i.indisready,i.indisvalid,i.indislive,c.oid FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class c ON i.indexrelid=c.oid JOIN pg_catalog.pg_namespace n ON c.relnamespace=n.oid WHERE n.nspname=$1 AND c.relname=$2").await.unwrap();
            for _ in 0..4 {
                client.query(&locks, &[&Some(-1_i32), &vec![0_u32]]).await.unwrap();
                client.query(&coordination, &[&-1_i32]).await.unwrap();
                client.query(&indexes, &[&"", &""]).await.unwrap();
            }
            Self { locks, coordination, indexes }
        }).await.expect("bounded native storage observer setup did not complete")
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

    async fn wait_physical(&self, client: &Client, pid: i32, oid: u32) {
        tokio::time::timeout(DEADLINE, async {
            loop {
                if self
                    .modes(client, Some(pid), &[oid])
                    .await
                    .iter()
                    .any(|row| !row.3)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("native storage physical wait was not observed");
    }

    async fn wait_index(&self, client: &Client, schema: &str, name: &str, ready: bool) -> u32 {
        tokio::time::timeout(DEADLINE, async {
            loop {
                for row in client
                    .query(&self.indexes, &[&schema, &name])
                    .await
                    .unwrap()
                {
                    if row.get::<_, bool>(0) == ready
                        && !row.get::<_, bool>(1)
                        && row.get::<_, bool>(2)
                    {
                        return row.get(3);
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("ordinary concurrent index phase was not observed")
    }
}

fn fact_oids(state: &Value) -> Vec<u32> {
    state["metadata"]["facts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|fact| u32::try_from(fact["oid"].as_u64().unwrap()).unwrap())
        .collect()
}

fn expected_locks(state: &Value) -> Vec<(u32, u32, String, bool)> {
    let facts = state["metadata"]["facts"].as_array().unwrap();
    let mut result: Vec<_> = state["metadata"]["references"]
        .as_array()
        .unwrap()
        .iter()
        .map(|reference| {
            let oid = u32::try_from(reference[0].as_u64().unwrap()).unwrap();
            let fact = facts.iter().find(|fact| fact["oid"] == oid).unwrap();
            let database = u32::try_from(fact["file_database_oid"].as_u64().unwrap()).unwrap();
            let mode = match reference[1].as_u64().unwrap() {
                1 => "AccessShareLock",
                2 => "RowShareLock",
                3 => "RowExclusiveLock",
                other => panic!("unexpected mode {other}"),
            };
            (oid, database, mode.to_owned(), true)
        })
        .collect();
    result.sort();
    result
}

fn finish(outcome: Result<Result<(), tokio::time::error::Elapsed>, Box<dyn std::any::Any + Send>>) {
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("native storage case expired: {error}"),
        Err(error) => std::panic::resume_unwind(error),
    }
}

async fn finish_targets(client: &Client, gids: &[&str]) {
    for gid in gids {
        match sql(client, &format!("ROLLBACK PREPARED '{gid}'")).await {
            Ok(()) => {}
            Err(error) if error.code() == Some(&SqlState::UNDEFINED_OBJECT) => {}
            Err(error) => panic!("normal native storage prepared cleanup failed: {error}"),
        }
    }
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[tokio::test]
async fn literal_bindings_full_graph_exact_modes_and_borrowed_counts() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "heap_storage_exact_字";
    let first = "a'\" . λ";
    let second = format!("{}x", "é".repeat(31));
    assert_eq!(second.len(), 63);
    let qschema = quoted(schema);
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer, &format!("CREATE SCHEMA {qschema}; CREATE TABLE {qschema}.{}(id integer PRIMARY KEY,body text); CREATE INDEX extra ON {qschema}.{}(body); CREATE UNLOGGED TABLE {qschema}.{}(id integer,body text)",quoted(first),quoted(first),quoted(&second))).await.unwrap();
        let a = oid(&observer,schema,first).await;
        let b = oid(&observer,schema,&second).await;
        assert!(a < b);
        let mut expected = oracle(&observer,a).await;
        expected.extend(oracle(&observer,b).await);
        let roots = json!([[schema,second,3],[schema,first,1],[schema,first,3],[schema,second,1],[schema,first,2],[schema,second,2],[schema,first,1]]);
        for established in [false,true] {
            sql(&reader, if established { "BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT 1" } else { "BEGIN" }).await.unwrap();
            let state = capture(&reader,&roots,false).await;
            check_scope(&state,established,false);
            check_graph(&state,&expected,14);
            assert_eq!(state["metadata"]["roots"],json!([[b,3],[a,1],[a,3],[b,1],[a,2],[b,2],[a,1]]));
            let oids = fact_oids(&state);
            assert!(probe.modes(&observer,Some(backend),&oids).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            let raw = state["metadata"]["references"].as_array().unwrap().iter()
                .map(|r| format!("{}/{}",r[0],r[1])).collect::<Vec<_>>().join(",");
            command(&reader,&format!("relation_borrow:{raw}")).await.unwrap();
            let again = capture(&reader,&roots,false).await;
            check_scope(&again,established,false);
            check_graph(&again,&expected,14);
            assert_eq!(probe.modes(&observer,Some(backend),&oids).await,expected_locks(&state));
            command(&reader,"relation_unborrow").await.unwrap();
            assert!(probe.modes(&observer,Some(backend),&oids).await.is_empty());
            let retained = capture(&reader,&roots,true).await;
            check_scope(&retained,established,true);
            assert_eq!(probe.modes(&observer,Some(backend),&oids).await,expected_locks(&state));
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"COMMIT").await.unwrap();
            assert!(probe.modes(&observer,Some(backend),&oids).await.is_empty());
        }
    })).catch_unwind().await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        &format!("DROP SCHEMA IF EXISTS {qschema} CASCADE"),
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn completed_storage_retention_follows_native_children_and_prepared_outcomes() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let gids = [
        "heap_storage_retained_commit",
        "heap_storage_retained_abort",
    ];
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE,async {
        sql(&observer,"CREATE SCHEMA heap_storage_retain; CREATE TABLE heap_storage_retain.t(id integer PRIMARY KEY,body text)").await.unwrap();
        let root = oid(&observer,"heap_storage_retain","t").await;
        let expected = oracle(&observer,root).await;
        let database = expected[0]["file_database_oid"].as_u64().unwrap() as u32;
        let roots = json!([["heap_storage_retain","t",1],["heap_storage_retain","t",3]]);
        sql(&reader,"BEGIN").await.unwrap();
        command(&reader,&format!("relation_borrow:{root}/1")).await.unwrap();
        sql(&reader,"SAVEPOINT child").await.unwrap();
        let state = capture(&reader,&roots,true).await;
        check_scope(&state,false,true);
        check_graph(&state,&expected,10);
        let oids = fact_oids(&state);
        sql(&reader,"ROLLBACK TO child; RELEASE child").await.unwrap();
        assert_eq!(probe.modes(&observer,Some(backend),&oids).await,vec![(root,database,"AccessShareLock".to_owned(),true)]);
        sql(&reader,"SAVEPOINT child").await.unwrap();
        capture(&reader,&roots,true).await;
        sql(&reader,"RELEASE child").await.unwrap();
        command(&reader,"relation_unborrow").await.unwrap();
        assert_eq!(probe.modes(&observer,Some(backend),&oids).await,expected_locks(&state));
        probe.no_coordination(&observer,backend).await;
        sql(&reader,"COMMIT").await.unwrap();
        assert!(probe.modes(&observer,Some(backend),&oids).await.is_empty());
        for (gid,ending) in gids.iter().zip(["COMMIT","ROLLBACK"]) {
            sql(&reader,"BEGIN").await.unwrap();
            let retained = capture(&reader,&roots,true).await;
            check_scope(&retained,false,true);
            sql(&reader,&format!("PREPARE TRANSACTION '{gid}'")).await.unwrap();
            assert!(probe.modes(&observer,Some(backend),&oids).await.is_empty());
            assert_eq!(probe.modes(&observer,None,&oids).await,expected_locks(&state));
            probe.no_coordination(&observer,backend).await;
            sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
            assert!(probe.modes(&observer,None,&oids).await.is_empty());
        }
        sql(&reader,"BEGIN").await.unwrap();
        capture(&reader,&roots,true).await;
        sql(&reader,"ROLLBACK").await.unwrap();
        assert!(probe.modes(&observer,Some(backend),&oids).await.is_empty());
    })).catch_unwind().await;
    finish_targets(&observer, &gids).await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        "DROP SCHEMA IF EXISTS heap_storage_retain CASCADE",
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn shared_mapped_and_actual_own_temporary_storage_identity() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let (foreign, fd) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE,async {
        for name in ["pg_class","pg_database"] {
            let root = oid(&observer,"pg_catalog",name).await;
            let expected = oracle(&observer,root).await;
            assert_eq!(expected[0]["stored_file_number"],0);
            assert_ne!(expected[0]["file_number"],0);
            sql(&reader,"BEGIN").await.unwrap();
            let state = capture(&reader,&json!([["pg_catalog",name,1]]),true).await;
            check_scope(&state,false,true);
            check_graph(&state,&expected,2);
            let oids = fact_oids(&state);
            assert_eq!(probe.modes(&observer,Some(backend),&oids).await,expected_locks(&state));
            if name == "pg_database" {
                assert!(state["metadata"]["facts"].as_array().unwrap().iter().all(|fact| fact["shared"] == true && fact["file_database_oid"] == 0 && fact["file_tablespace_oid"] == 1664));
            }
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"ROLLBACK").await.unwrap();
            assert!(probe.modes(&observer,Some(backend),&oids).await.is_empty());
        }
        sql(&reader,"CREATE TEMP TABLE heap_storage_temp(id integer PRIMARY KEY,body text)").await.unwrap();
        sql(&foreign,"CREATE TEMP TABLE heap_storage_foreign(id integer,body text)").await.unwrap();
        let temp_schema: String = reader.query_one("SELECT nspname::text FROM pg_catalog.pg_namespace WHERE oid=pg_catalog.pg_my_temp_schema()",&[]).await.unwrap().get(0);
        let foreign_schema: String = foreign.query_one("SELECT nspname::text FROM pg_catalog.pg_namespace WHERE oid=pg_catalog.pg_my_temp_schema()",&[]).await.unwrap().get(0);
        assert_ne!(temp_schema,foreign_schema);
        let root = oid(&reader,&temp_schema,"heap_storage_temp").await;
        let expected = oracle(&reader,root).await;
        sql(&reader,"BEGIN").await.unwrap();
        let state = capture(&reader,&json!([[temp_schema,"heap_storage_temp",3]]),true).await;
        check_scope(&state,false,true);
        check_graph(&state,&expected,8);
        let oids = fact_oids(&state);
        let proc_number = state["metadata"]["facts"][0]["file_proc_number"].as_i64().unwrap();
        assert!(proc_number >= 0);
        assert!(state["metadata"]["facts"].as_array().unwrap().iter().all(|fact| fact["file_proc_number"] == proc_number && fact["persistence"] == u64::from(b't')));
        assert_eq!(probe.modes(&observer,Some(backend),&oids).await,expected_locks(&state));
        sql(&reader,"SAVEPOINT foreign_temp").await.unwrap();
        let error = command(&reader,&storage_command(&json!([[foreign_schema,"heap_storage_foreign",1]]),false)).await.unwrap_err();
        assert_eq!(error.code(),Some(&SqlState::FEATURE_NOT_SUPPORTED));
        sql(&reader,"ROLLBACK TO foreign_temp; RELEASE foreign_temp").await.unwrap();
        let again = capture(&reader,&json!([[temp_schema,"heap_storage_temp",1]]),false).await;
        check_scope(&again,false,false);
        check_graph(&again,&expected,2);
        assert_eq!(probe.modes(&observer,Some(backend),&oids).await,expected_locks(&state));
        probe.no_coordination(&observer,backend).await;
        sql(&reader,"ROLLBACK").await.unwrap();
        assert!(probe.modes(&observer,Some(backend),&oids).await.is_empty());
        sql(&reader,"DROP TABLE heap_storage_temp").await.unwrap();
        sql(&foreign,"DROP TABLE heap_storage_foreign").await.unwrap();
    })).catch_unwind().await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&foreign, "ROLLBACK").await.unwrap();
    close(reader, rd).await;
    close(foreign, fd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn explicit_unsupported_boundaries_abort_the_child_and_allow_fresh_admission() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE,async {
        sql(&observer,"CREATE SCHEMA heap_storage_limits; CREATE TABLE heap_storage_limits.good(id integer); CREATE TABLE heap_storage_limits.parent(id integer); CREATE TABLE heap_storage_limits.child() INHERITS(heap_storage_limits.parent); CREATE VIEW heap_storage_limits.v AS SELECT id FROM heap_storage_limits.good; CREATE MATERIALIZED VIEW heap_storage_limits.m AS SELECT id FROM heap_storage_limits.good; CREATE SEQUENCE heap_storage_limits.s; CREATE TABLE heap_storage_limits.p(id integer) PARTITION BY RANGE(id); CREATE TABLE heap_storage_limits.leaf PARTITION OF heap_storage_limits.p FOR VALUES FROM(0) TO(10); CREATE TABLE heap_storage_limits.hash_table(id integer); CREATE INDEX hash_index ON heap_storage_limits.hash_table USING hash(id)").await.unwrap();
        let root = oid(&observer,"heap_storage_limits","good").await;
        let expected = oracle(&observer,root).await;
        let parent = oid(&observer,"heap_storage_limits","parent").await;
        let parent_expected = oracle(&observer,parent).await;
        let all: Vec<u32> = observer.query("SELECT oid FROM pg_catalog.pg_class WHERE relnamespace=(SELECT oid FROM pg_catalog.pg_namespace WHERE nspname='heap_storage_limits')",&[]).await.unwrap().into_iter().map(|row| row.get(0)).collect();
        sql(&reader,"BEGIN").await.unwrap();
        let parent_state = capture(&reader,&json!([["heap_storage_limits","parent",1]]),false).await;
        check_scope(&parent_state,false,false);
        check_graph(&parent_state,&parent_expected,2);
        assert_eq!(parent_state["metadata"]["facts"].as_array().unwrap().len(),1);
        assert_eq!(parent_state["metadata"]["facts"][0]["has_subclasses"],true);
        for name in ["v","m","s","p","leaf","hash_table"] {
            sql(&reader,"SAVEPOINT boundary").await.unwrap();
            let error = command(&reader,&storage_command(&json!([["heap_storage_limits",name,1]]),false)).await.unwrap_err();
            assert_eq!(error.code(),Some(&SqlState::FEATURE_NOT_SUPPORTED),"{name}: {error}");
            sql(&reader,"ROLLBACK TO boundary; RELEASE boundary").await.unwrap();
            let fresh = capture(&reader,&json!([["heap_storage_limits","good",1]]),false).await;
            check_scope(&fresh,false,false);
            check_graph(&fresh,&expected,2);
            assert!(probe.modes(&observer,Some(backend),&all).await.is_empty());
            probe.no_coordination(&observer,backend).await;
        }
        for roots in [json!([["heap_storage_limits","good",4]]),json!([["heap_storage_limits","x".repeat(64),1]])] {
            sql(&reader,"SAVEPOINT boundary").await.unwrap();
            let error = command(&reader,&storage_command(&roots,false)).await.unwrap_err();
            assert_eq!(error.code(),Some(&SqlState::INVALID_PARAMETER_VALUE));
            sql(&reader,"ROLLBACK TO boundary; RELEASE boundary").await.unwrap();
            check_scope(&capture(&reader,&json!([["heap_storage_limits","good",1]]),false).await,false,false);
        }
        sql(&reader,"COMMIT").await.unwrap();
    })).catch_unwind().await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        "DROP SCHEMA IF EXISTS heap_storage_limits CASCADE",
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn prepared_literal_replacement_waits_without_fences_and_rechecks_current_storage() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (writer, wd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let gids = ["heap_storage_replace_commit", "heap_storage_replace_abort"];
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE,async {
        sql(&observer,"CREATE SCHEMA heap_storage_wait; CREATE TABLE heap_storage_wait.t(id integer PRIMARY KEY,body text)").await.unwrap();
        for established in [false,true] {
            for (gid,ending) in gids.iter().zip(["COMMIT","ROLLBACK"]) {
                let old = oid(&observer,"heap_storage_wait","t").await;
                sql(&reader,if established { "BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT 1" } else { "BEGIN" }).await.unwrap();
                sql(&writer,&format!("BEGIN; DROP TABLE heap_storage_wait.t; CREATE TABLE heap_storage_wait.t(id integer PRIMARY KEY,body text,changed integer); CREATE INDEX extra ON heap_storage_wait.t(body); PREPARE TRANSACTION '{gid}'")).await.unwrap();
                let request = storage_command(&json!([["heap_storage_wait","t",1]]),false);
                let waiting = command(&reader,&request);
                tokio::pin!(waiting);
                tokio::select! {
                    result = &mut waiting => panic!("native storage physical acquisition unexpectedly completed: {result:?}"),
                    () = probe.wait_physical(&observer,backend,old) => {},
                }
                probe.no_coordination(&observer,backend).await;
                // The first fact observation has ended all its own catalog
                // descriptor increments before this native prepared wait.
                assert!(probe.modes(&observer,Some(backend),&[1259,2610,2615]).await.is_empty());
                sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
                waiting.await.unwrap();
                let current = oid(&observer,"heap_storage_wait","t").await;
                if ending == "COMMIT" { assert_ne!(current,old); } else { assert_eq!(current,old); }
                let expected = oracle(&observer,current).await;
                let state = status(&reader).await;
                check_scope(&state,established,false);
                check_graph(&state,&expected,2);
                assert_eq!(state["metadata"]["roots"],json!([[current,1]]));
                assert!(state["metadata"]["attempts"].as_u64().unwrap() >= 2);
                let mut oids = fact_oids(&state);
                oids.push(old);
                assert!(probe.modes(&observer,Some(backend),&oids).await.is_empty());
                probe.no_coordination(&observer,backend).await;
                sql(&reader,"COMMIT").await.unwrap();
            }
        }
    })).catch_unwind().await;
    finish_targets(&observer, &gids).await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&writer, "ROLLBACK").await.unwrap();
    sql(&observer, "DROP SCHEMA IF EXISTS heap_storage_wait CASCADE")
        .await
        .unwrap();
    close(reader, rd).await;
    close(writer, wd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn all_live_indexes_include_ordinary_not_ready_and_ready_invalid_build_phases() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (builder, bd) = client().await;
    let (holder, hd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let gid = "heap_storage_index_writer";
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE,async {
        sql(&observer,"CREATE SCHEMA heap_storage_live; CREATE TABLE heap_storage_live.t(id integer PRIMARY KEY,body text); INSERT INTO heap_storage_live.t VALUES(1,'a')").await.unwrap();
        let root = oid(&observer,"heap_storage_live","t").await;
        for ready in [false,true] {
            let name = if ready { "ready_invalid" } else { "not_ready" };
            if ready {
                // An ordinary older RR data reader keeps the final validation
                // phase waiting after indisready has been committed.
                sql(&holder,"BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT body FROM heap_storage_live.t").await.unwrap();
            } else {
                // Ordinary native data-only 2PC holds RX before the initial
                // build wait. No failed index, catalog corruption or signal.
                sql(&holder,&format!("BEGIN; INSERT INTO heap_storage_live.t VALUES(2,'b'); PREPARE TRANSACTION '{gid}'")).await.unwrap();
            }
            let create = format!("CREATE INDEX CONCURRENTLY {name} ON heap_storage_live.t(body)");
            let building = sql(&builder,&create);
            tokio::pin!(building);
            let index = tokio::select! {
                result = &mut building => panic!("ordinary concurrent index completed before its controlled native phase: {result:?}"),
                index = probe.wait_index(&observer,"heap_storage_live",name,ready) => index,
            };
            let expected = oracle(&observer,root).await;
            sql(&reader,"BEGIN").await.unwrap();
            let state = capture(&reader,&json!([["heap_storage_live","t",1]]),false).await;
            check_scope(&state,false,false);
            check_graph(&state,&expected,2);
            let fact = state["metadata"]["facts"].as_array().unwrap().iter().find(|fact| fact["oid"] == index).unwrap();
            assert_eq!(fact["live"],true);
            assert_eq!(fact["ready"],ready);
            assert_eq!(fact["valid"],false);
            assert!(state["metadata"]["references"].as_array().unwrap().contains(&json!([index,1])));
            assert!(probe.modes(&observer,Some(backend),&fact_oids(&state)).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"COMMIT").await.unwrap();
            if ready { sql(&holder,"COMMIT").await.unwrap(); }
            else { sql(&observer,&format!("COMMIT PREPARED '{gid}'")).await.unwrap(); }
            building.await.unwrap();
            let completed: bool = observer.query_one("SELECT indisready AND indisvalid AND indislive FROM pg_catalog.pg_index WHERE indexrelid=$1",&[&index]).await.unwrap().get(0);
            assert!(completed);
        }
    })).catch_unwind().await;
    // Normal native completion releases any controlled prerequisite before
    // the builder's queued ROLLBACK; dropping an await never cancels SQL.
    finish_targets(&observer, &[gid]).await;
    sql(&holder, "ROLLBACK").await.unwrap();
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&builder, "ROLLBACK").await.unwrap();
    sql(&observer, "DROP SCHEMA IF EXISTS heap_storage_live CASCADE")
        .await
        .unwrap();
    close(reader, rd).await;
    close(holder, hd).await;
    close(builder, bd).await;
    close(observer, od).await;
    finish(outcome);
}
