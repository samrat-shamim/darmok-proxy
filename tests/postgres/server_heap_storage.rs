//! Private catalog-declared storage, not application SQL or full semantic closure.
use futures_util::{FutureExt, StreamExt};
use serde_json::{Value, json};
use std::{collections::BTreeMap, panic::AssertUnwindSafe, time::Duration};
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
        "WITH heaps AS (SELECT oid,0::oid AS parent_oid,0 AS phase FROM pg_catalog.pg_class WHERE oid=$1 UNION ALL SELECT t.oid,r.oid,2 FROM pg_catalog.pg_class r JOIN pg_catalog.pg_class t ON t.oid=r.reltoastrelid WHERE r.oid=$1), nodes AS (SELECT oid,parent_oid,phase FROM heaps UNION ALL SELECT i.indexrelid,h.oid,h.phase+1 FROM heaps h JOIN pg_catalog.pg_index i ON i.indrelid=h.oid WHERE i.indislive) SELECT pg_catalog.json_build_object('oid',c.oid::bigint,'schema_oid',n.oid::bigint,'schema',n.nspname,'name',c.relname,'kind',pg_catalog.ascii(c.relkind::text),'persistence',pg_catalog.ascii(c.relpersistence::text),'am',c.relam::bigint,'toast_oid',c.reltoastrelid::bigint,'parent_oid',x.parent_oid::bigint,'shared',c.relisshared,'is_partition',c.relispartition,'has_indexes',c.relhasindex,'has_subclasses',c.relhassubclass,'live',COALESCE(i.indislive,false),'ready',COALESCE(i.indisready,false),'valid',COALESCE(i.indisvalid,false),'check_xmin',COALESCE(i.indcheckxmin,false),'tablespace_oid',c.reltablespace::bigint,'stored_file_number',c.relfilenode::bigint,'row_type_oid',c.reltype::bigint,'declared_attribute_count',c.relnatts,'file_tablespace_oid',COALESCE(NULLIF(c.reltablespace,0),(SELECT dattablespace FROM pg_catalog.pg_database WHERE datname=current_database()))::bigint,'file_database_oid',(CASE WHEN c.relisshared THEN 0::oid ELSE (SELECT oid FROM pg_catalog.pg_database WHERE datname=current_database()) END)::bigint,'file_number',pg_catalog.pg_relation_filenode(c.oid::regclass)::bigint,'file_proc_number',CASE WHEN c.relpersistence='t' THEN substring(pg_catalog.pg_relation_filepath(c.oid::regclass) FROM '/t([0-9]+)_')::integer ELSE -1 END)::text FROM nodes x JOIN pg_catalog.pg_class c ON c.oid=x.oid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace LEFT JOIN pg_catalog.pg_index i ON i.indexrelid=c.oid ORDER BY x.phase,c.oid", &[&root],
    )).await.unwrap().unwrap();
    rows.into_iter()
        .map(|row| serde_json::from_str(&row.get::<_, String>(0)).unwrap())
        .collect()
}

struct ColumnOracle {
    roots: BTreeMap<u32, Value>,
    attributes: Vec<Value>,
    types: Vec<Value>,
    scanned: Value,
}

async fn column_oracle(client: &Client, roots: &[u32]) -> ColumnOracle {
    tokio::time::timeout(DEADLINE, async {
        let root_rows = client.query(
            "SELECT oid,pg_catalog.json_build_object('oid',oid::bigint,'row_type_oid',reltype::bigint,'declared_attribute_count',relnatts)::text FROM pg_catalog.pg_class WHERE oid=ANY($1) ORDER BY oid", &[&roots],
        ).await.unwrap();
        let roots = root_rows.into_iter().map(|row| {
            (row.get::<_, u32>(0), serde_json::from_str(&row.get::<_, String>(1)).unwrap())
        }).collect::<BTreeMap<_, Value>>();
        let oids = roots.keys().copied().collect::<Vec<_>>();
        let attributes = client.query(
            "SELECT pg_catalog.json_build_object('relation_oid',attrelid::bigint,'number',attnum,'name',attname,'type_oid',atttypid::bigint,'length',attlen,'typmod',atttypmod,'dimensions',attndims,'by_value',attbyval,'alignment',pg_catalog.ascii(attalign::text),'storage',pg_catalog.ascii(attstorage::text),'compression',CASE WHEN attcompression::text='' THEN 0 ELSE pg_catalog.ascii(attcompression::text) END,'not_null_declared',attnotnull,'has_default',atthasdef,'has_missing',atthasmissing,'identity',CASE WHEN attidentity::text='' THEN 0 ELSE pg_catalog.ascii(attidentity::text) END,'generated',CASE WHEN attgenerated::text='' THEN 0 ELSE pg_catalog.ascii(attgenerated::text) END,'dropped',attisdropped,'local',attislocal,'inheritance_count',attinhcount,'collation_oid',attcollation::bigint)::text FROM pg_catalog.pg_attribute WHERE attrelid=ANY($1) AND attnum>0 ORDER BY attrelid,attnum", &[&oids],
        ).await.unwrap().into_iter().map(|row| serde_json::from_str(&row.get::<_, String>(0)).unwrap()).collect();
        let types = client.query(
            "SELECT pg_catalog.json_build_object('oid',t.oid::bigint,'schema_oid',t.typnamespace::bigint,'schema',n.nspname,'name',t.typname,'length',t.typlen,'by_value',t.typbyval,'kind',pg_catalog.ascii(t.typtype::text),'category',pg_catalog.ascii(t.typcategory::text),'preferred',t.typispreferred,'defined',t.typisdefined,'delimiter',pg_catalog.ascii(t.typdelim::text),'relation_oid',t.typrelid::bigint,'subscript_oid',t.typsubscript::oid::bigint,'element_oid',t.typelem::bigint,'array_oid',t.typarray::bigint,'input_oid',t.typinput::oid::bigint,'output_oid',t.typoutput::oid::bigint,'receive_oid',t.typreceive::oid::bigint,'send_oid',t.typsend::oid::bigint,'typmod_input_oid',t.typmodin::oid::bigint,'typmod_output_oid',t.typmodout::oid::bigint,'analyze_oid',t.typanalyze::oid::bigint,'alignment',pg_catalog.ascii(t.typalign::text),'storage',pg_catalog.ascii(t.typstorage::text),'not_null_declared',t.typnotnull,'base_type_oid',t.typbasetype::bigint,'typmod',t.typtypmod,'dimensions',t.typndims,'collation_oid',t.typcollation::bigint)::text FROM pg_catalog.pg_type t JOIN pg_catalog.pg_namespace n ON n.oid=t.typnamespace WHERE t.oid IN(SELECT atttypid FROM pg_catalog.pg_attribute WHERE attrelid=ANY($1) AND attnum>0 AND NOT attisdropped AND atttypid<>0) ORDER BY t.oid", &[&oids],
        ).await.unwrap().into_iter().map(|row| serde_json::from_str(&row.get::<_, String>(0)).unwrap()).collect();
        let scanned = serde_json::from_str(&client.query_one(
            "SELECT pg_catalog.json_build_object('namespace_rows',(SELECT count(*) FROM pg_catalog.pg_namespace),'relation_rows',(SELECT count(*) FROM pg_catalog.pg_class),'index_rows',(SELECT count(*) FROM pg_catalog.pg_index),'attribute_rows',(SELECT count(*) FROM pg_catalog.pg_attribute),'type_rows',(SELECT count(*) FROM pg_catalog.pg_type))::text", &[],
        ).await.unwrap().get::<_, String>(0)).unwrap();
        ColumnOracle { roots, attributes, types, scanned }
    }).await.expect("bounded independent column/type oracle did not complete")
}

async fn create_missing_oracles(client: &Client, schema: &str) {
    let schema = quoted(schema);
    sql(client, &format!(
        "CREATE FUNCTION {schema}.image(anyarray) RETURNS bytea AS '$libdir/darmok_catalog_probe','darmok_test_missing_array_image' LANGUAGE C STRICT; CREATE FUNCTION {schema}.physical_natts(regclass) RETURNS integer[] AS '$libdir/darmok_catalog_probe','darmok_test_heap_natts' LANGUAGE C STRICT"
    )).await.unwrap();
}

async fn missing_oracle(client: &Client, schema: &str, roots: &[u32]) -> Vec<Value> {
    let schema = quoted(schema);
    tokio::time::timeout(DEADLINE, client.query(&format!(
        "WITH observed AS MATERIALIZED (SELECT attrelid,attnum,atttypid,pg_catalog.pg_column_size(attmissingval) stored_bytes,pg_catalog.pg_column_compression(attmissingval) compression,{schema}.image(attmissingval) image,pg_catalog.array_ndims(attmissingval) ndim,pg_catalog.array_length(attmissingval,1) dim,pg_catalog.array_lower(attmissingval,1) lbound FROM pg_catalog.pg_attribute WHERE attrelid=ANY($1) AND attnum>0 AND NOT attisdropped AND atthasmissing) SELECT pg_catalog.json_build_object('relation_oid',attrelid::bigint,'number',attnum,'type_oid',atttypid::bigint,'carrier_kind',CASE WHEN compression='pglz' THEN 'p' WHEN compression='lz4' THEN 'l' WHEN compression IS NULL AND stored_bytes=pg_catalog.octet_length(image)-3 THEN 's' WHEN compression IS NULL AND stored_bytes=pg_catalog.octet_length(image) THEN 'u' ELSE 'unexpected-native-form' END,'stored_bytes',stored_bytes,'image_bytes',pg_catalog.octet_length(image),'image',pg_catalog.encode(image,'hex'))::text,ndim,dim,lbound FROM observed ORDER BY attrelid,attnum"
    ), &[&roots])).await.expect("bounded independent missing image oracle did not complete").unwrap()
        .into_iter().map(|row|{
            for column in 1..=3 { assert_eq!(row.get::<_,Option<i32>>(column),Some(1)); }
            serde_json::from_str(&row.get::<_,String>(0)).unwrap()
        }).collect()
}

fn check_missing(state: &Value, expected: &[Value]) {
    assert_eq!(state["metadata"]["missing"], json!(expected));
    assert_eq!(state["metadata"]["missing_count"], expected.len());
}

fn check_columns(state: &Value, expected: &ColumnOracle) {
    let metadata = &state["metadata"];
    assert_eq!(metadata["attributes"], json!(expected.attributes));
    assert_eq!(metadata["types"], json!(expected.types));
    let root_facts = metadata["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|binding| {
            let oid = u32::try_from(binding[0].as_u64().unwrap()).unwrap();
            let mut fact = expected.roots[&oid].clone();
            let offset: u64 = expected
                .roots
                .range(..oid)
                .map(|(_, root)| root["declared_attribute_count"].as_u64().unwrap())
                .sum();
            fact["attribute_offset"] = json!(offset);
            fact
        })
        .collect::<Vec<_>>();
    assert_eq!(metadata["root_facts"], json!(root_facts));
    let missing = metadata["missing"].as_array().unwrap();
    let declarations = expected
        .attributes
        .iter()
        .filter(|fact| fact["has_missing"] == true)
        .collect::<Vec<_>>();
    assert_eq!(metadata["missing_count"], missing.len());
    assert_eq!(missing.len(), declarations.len());
    let mut image_bytes = 0;
    let mut stored_bytes = 0;
    for (image, declaration) in missing.iter().zip(declarations) {
        for field in ["relation_oid", "number", "type_oid"] {
            assert_eq!(image[field], declaration[field]);
        }
        assert_eq!(declaration["dropped"], false);
        let bytes = image["image_bytes"].as_u64().unwrap();
        assert_eq!(image["image"].as_str().unwrap().len() as u64, bytes * 2);
        assert!(bytes >= 24);
        assert!(matches!(
            image["carrier_kind"].as_str().unwrap(),
            "s" | "u" | "p" | "l"
        ));
        image_bytes += bytes;
        stored_bytes += image["stored_bytes"].as_u64().unwrap();
    }
    assert_eq!(metadata["missing_image_bytes"], image_bytes);
    for phase in ["initial_cost", "final_cost"] {
        for (name, count) in expected.scanned.as_object().unwrap() {
            assert_eq!(&metadata[phase][name], count, "{phase}/{name}: {metadata}");
        }
        let allocated = metadata[phase]["allocated_bytes"].as_u64().unwrap();
        let arrays = metadata["attribute_array_bytes"].as_u64().unwrap()
            + metadata["type_array_bytes"].as_u64().unwrap();
        assert_eq!(metadata[phase]["missing_carrier_bytes"], stored_bytes);
        assert!(allocated > 0 && allocated >= arrays);
    }
    for (name, count) in [
        ("attribute_array_bytes", expected.attributes.len()),
        ("type_array_bytes", expected.types.len()),
        ("missing_array_bytes", missing.len()),
    ] {
        let bytes = metadata[name].as_u64().unwrap();
        if count == 0 {
            assert_eq!(bytes, 0);
        } else {
            assert!(bytes > count as u64 && bytes.is_multiple_of(count as u64));
        }
    }
}

fn check_graph(state: &Value, expected: &[Value], root_mask: u64) {
    let root_masks = expected
        .iter()
        .filter(|fact| fact["parent_oid"] == 0)
        .map(|fact| {
            (
                u32::try_from(fact["oid"].as_u64().unwrap()).unwrap(),
                root_mask,
            )
        })
        .collect();
    check_graph_modes(state, expected, &root_masks);
}

fn check_graph_modes(state: &Value, expected_facts: &[Value], root_masks: &BTreeMap<u32, u64>) {
    let metadata = &state["metadata"];
    let facts = metadata["facts"].as_array().unwrap();
    assert_eq!(facts.len(), expected_facts.len());
    let mut references = Vec::new();
    for (actual, expected) in facts.iter().zip(expected_facts) {
        for (key, value) in expected.as_object().unwrap() {
            assert_eq!(&actual[key], value, "{key}: {actual}");
        }
        let toast = expected["kind"] == u64::from(b't')
            || expected_facts.iter().any(|parent| {
                parent["oid"] == expected["parent_oid"] && parent["kind"] == u64::from(b't')
            });
        let mut ancestor = expected;
        while ancestor["parent_oid"] != 0 {
            ancestor = expected_facts
                .iter()
                .find(|fact| fact["oid"] == ancestor["parent_oid"])
                .unwrap();
        }
        let root = u32::try_from(ancestor["oid"].as_u64().unwrap()).unwrap();
        let root_mask = root_masks[&root];
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
async fn selected_positive_slots_types_and_column_overrides_are_exact_declarations() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "attribute_facts_字";
    let qschema = quoted(schema);
    let long_column = format!("{}x", "é".repeat(31));
    let renamed_column = "a'\" . λ";
    assert_eq!(long_column.len(), 63);
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,&format!("CREATE SCHEMA {qschema}; CREATE TYPE {qschema}.gone AS ENUM ('removed'); CREATE TYPE {qschema}.int4 AS ENUM ('different'); CREATE DOMAIN {qschema}.dom AS varchar(9) NOT NULL; CREATE TYPE {qschema}.pair AS (n integer); CREATE TABLE {qschema}.t({} integer,removed {qschema}.gone,txt text COLLATE \"C\",short varchar(7),amount numeric(6,2),arr integer[][],dm {qschema}.dom,fake {qschema}.int4,pair {qschema}.pair,nullable integer,same integer,id integer GENERATED ALWAYS AS IDENTITY,calc integer GENERATED ALWAYS AS(same+1) STORED); CREATE TABLE {qschema}.empty(); INSERT INTO {qschema}.t(dm,same) VALUES('ok',1); ALTER TABLE {qschema}.t ALTER COLUMN txt SET STORAGE EXTERNAL; ALTER TABLE {qschema}.t ALTER COLUMN txt SET COMPRESSION pglz; ALTER TABLE {qschema}.t ADD COLUMN missing integer DEFAULT 7",quoted(&long_column))).await.unwrap();
        let root = oid(&observer,schema,"t").await;
        let empty = oid(&observer,schema,"empty").await;
        let before_drop = column_oracle(&observer,&[root,empty]).await;
        let removed_type = u32::try_from(before_drop.attributes[1]["type_oid"].as_u64().unwrap()).unwrap();
        sql(&observer,&format!("ALTER TABLE {qschema}.t DROP COLUMN removed; DROP TYPE {qschema}.gone; ALTER TABLE {qschema}.t RENAME COLUMN nullable TO {}", quoted(renamed_column))).await.unwrap();
        let removed: i64 = observer.query_one("SELECT count(*) FROM pg_catalog.pg_type WHERE oid=$1",&[&removed_type]).await.unwrap().get(0);
        assert_eq!(removed,0);
        let expected = column_oracle(&observer,&[root,empty]).await;
        assert_eq!(expected.attributes.len(),14);
        let dropped = &expected.attributes[1];
        assert_eq!(dropped["number"],2);
        assert_eq!(dropped["dropped"],true);
        assert_eq!(dropped["type_oid"],0);
        for field in ["length","by_value","alignment"] {
            assert_eq!(dropped[field],before_drop.attributes[1][field]);
        }
        assert_eq!(expected.attributes[0]["name"],long_column);
        assert_eq!(expected.attributes[9]["name"],renamed_column);
        let text = &expected.attributes[2];
        let text_type = expected.types.iter().find(|fact| fact["oid"]==text["type_oid"]).unwrap();
        assert_eq!(text["storage"],u64::from(b'e'));
        assert_eq!(text_type["storage"],u64::from(b'x'));
        assert_eq!(text["compression"],u64::from(b'p'));
        assert_ne!(text["collation_oid"],text_type["collation_oid"]);
        assert_eq!(expected.attributes[3]["typmod"],11);
        assert_eq!(expected.attributes[5]["dimensions"],2);
        let fake = expected.types.iter().find(|fact| fact["name"]=="int4" && fact["schema"]==schema).unwrap();
        assert_ne!(fake["oid"],23);
        assert_eq!(fake["kind"],u64::from(b'e'));
        assert!(expected.types.iter().any(|fact| fact["kind"]==u64::from(b'd') && fact["base_type_oid"]==1043));
        assert!(expected.types.iter().any(|fact| fact["kind"]==u64::from(b'c') && fact["relation_oid"]!=0));
        assert!(expected.types.iter().any(|fact| fact["element_oid"]==23));
        assert!(expected.types.iter().all(|fact| fact["oid"]!=removed_type));
        assert_eq!(expected.attributes[11]["identity"],u64::from(b'a'));
        assert_eq!(expected.attributes[11]["not_null_declared"],true);
        assert_eq!(expected.attributes[12]["generated"],u64::from(b's'));
        assert_eq!(expected.attributes[13]["has_default"],true);
        assert_eq!(expected.attributes[13]["has_missing"],true);
        let roots = json!([[schema,"t",3],[schema,"empty",1],[schema,"t",1],[schema,"t",2],[schema,"empty",2]]);
        for established in [false,true] {
            sql(&reader,if established { "BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT 1" } else { "BEGIN" }).await.unwrap();
            let first = capture(&reader,&roots,false).await;
            check_scope(&first,established,false);
            check_columns(&first,&expected);
            let warm = capture(&reader,&roots,false).await;
            check_scope(&warm,established,false);
            check_columns(&warm,&expected);
            assert_eq!(first["metadata"]["root_facts"][0],first["metadata"]["root_facts"][2]);
            assert!(probe.modes(&observer,Some(backend),&fact_oids(&first)).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"COMMIT").await.unwrap();
        }
        let integer_columns = (0..32).map(|n|format!("n{n} integer")).collect::<Vec<_>>().join(",");
        sql(&observer,&format!("CREATE TABLE {qschema}.unselected({integer_columns})")).await.unwrap();
        let after_unselected = column_oracle(&observer,&[root,empty]).await;
        assert_eq!(after_unselected.attributes,expected.attributes);
        assert_eq!(after_unselected.types,expected.types);
        assert!(after_unselected.scanned["attribute_rows"].as_u64().unwrap() >= expected.scanned["attribute_rows"].as_u64().unwrap()+32);
        sql(&reader,"BEGIN").await.unwrap();
        let filtered = capture(&reader,&roots,false).await;
        check_scope(&filtered,false,false);
        check_columns(&filtered,&after_unselected);
        sql(&reader,"COMMIT").await.unwrap();
        sql(&reader,"BEGIN").await.unwrap();
        let empty_only = capture(&reader,&json!([[schema,"empty",1]]),false).await;
        check_scope(&empty_only,false,false);
        check_columns(&empty_only,&column_oracle(&observer,&[empty]).await);
        assert_eq!(empty_only["metadata"]["attributes"],json!([]));
        assert_eq!(empty_only["metadata"]["types"],json!([]));
        sql(&reader,"ROLLBACK").await.unwrap();
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
async fn missing_images_preserve_native_storage_forms_and_present_nulls() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "missing_images_字";
    let qschema = quoted(schema);
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,&format!("CREATE SCHEMA {qschema}; CREATE DOMAIN {qschema}.dom AS integer; CREATE TYPE {qschema}.label AS ENUM ('stored'); CREATE TYPE {qschema}.pair AS (n integer,s text); CREATE TABLE {qschema}.t(id integer); INSERT INTO {qschema}.t VALUES(1); CREATE TABLE {qschema}.parent(id integer); CREATE TABLE {qschema}.child() INHERITS({qschema}.parent); INSERT INTO {qschema}.parent VALUES(1); INSERT INTO {qschema}.child VALUES(2); SET default_toast_compression='pglz'; ALTER TABLE {qschema}.t ADD COLUMN nullable integer DEFAULT NULL, ADD COLUMN z integer DEFAULT 0, ADD COLUMN b boolean DEFAULT false, ADD COLUMN tiny text DEFAULT 'yes', ADD COLUMN plain text DEFAULT repeat('xy',100), ADD COLUMN p text DEFAULT repeat('pq',8192); SET default_toast_compression='lz4'; ALTER TABLE {qschema}.t ADD COLUMN l text DEFAULT repeat('uv',8192), ADD COLUMN dm {qschema}.dom DEFAULT 7, ADD COLUMN en {qschema}.label DEFAULT 'stored', ADD COLUMN arr integer[] DEFAULT ARRAY[1,NULL,3], ADD COLUMN pair {qschema}.pair DEFAULT ROW(7,'pair')::{qschema}.pair, ADD COLUMN empty text DEFAULT '', ADD COLUMN gone integer DEFAULT 19; ALTER TABLE {qschema}.t ALTER COLUMN p SET COMPRESSION lz4, ALTER COLUMN l SET COMPRESSION pglz; ALTER TABLE {qschema}.parent ADD COLUMN inherited integer DEFAULT 31; INSERT INTO {qschema}.t(id,z,b,tiny,plain,p,l,dm,en,arr,pair,empty,gone) VALUES(2,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL)")).await.unwrap();
        create_missing_oracles(&observer,schema).await;
        let root = oid(&observer,schema,"t").await;
        let parent = oid(&observer,schema,"parent").await;
        let child = oid(&observer,schema,"child").await;
        let oids = [root,parent,child];
        let mut ordered = oids;
        ordered.sort_unstable();
        let mut graph = Vec::new();
        for oid in ordered { graph.extend(oracle(&observer,oid).await); }
        let masks = BTreeMap::from([(root,14),(parent,2),(child,2)]);
        let roots = json!([[schema,"t",3],[schema,"child",1],[schema,"parent",1],[schema,"t",1],[schema,"t",2]]);
        let mut previous: Option<Vec<Value>> = None;
        for phase in 0..3 {
            if phase==1 {
                let changes = ["z","b","tiny","plain","p","l","dm","en","arr","pair","empty","gone"].iter()
                    .map(|name|format!("ALTER COLUMN {} DROP DEFAULT",quoted(name))).collect::<Vec<_>>().join(",");
                sql(&observer,&format!("ALTER TABLE {qschema}.t {changes}")).await.unwrap();
            } else if phase==2 {
                sql(&observer,&format!("ALTER TABLE {qschema}.t DROP COLUMN gone")).await.unwrap();
            }
            let columns = column_oracle(&observer,&oids).await;
            let images = missing_oracle(&observer,schema,&oids).await;
            assert_eq!(images.len(),if phase==2 {13} else {14});
            let forms = images.iter().map(|image|image["carrier_kind"].as_str().unwrap()).collect::<std::collections::BTreeSet<_>>();
            assert_eq!(forms,std::collections::BTreeSet::from(["s","u","p","l"]));
            let named = |name: &str| columns.attributes.iter().find(|fact|fact["relation_oid"]==root && fact["name"]==name).unwrap();
            let image_for = |name: &str| images.iter().find(|image|image["relation_oid"]==root && image["number"]==named(name)["number"]).unwrap();
            assert_eq!(named("nullable")["has_missing"],false);
            assert_eq!(image_for("z")["carrier_kind"],"s");
            assert_eq!(image_for("b")["carrier_kind"],"s");
            assert_eq!(image_for("tiny")["carrier_kind"],"s");
            assert_eq!(image_for("plain")["carrier_kind"],"u");
            assert_eq!(image_for("p")["carrier_kind"],"p");
            assert_eq!(image_for("l")["carrier_kind"],"l");
            assert_eq!(named("p")["compression"],u64::from(b'l'));
            assert_eq!(named("l")["compression"],u64::from(b'p'));
            for name in ["dm","en","arr","pair"] {
                assert_eq!(image_for(name)["type_oid"],named(name)["type_oid"]);
            }
            for relation in [parent,child] {
                assert!(images.iter().any(|image|image["relation_oid"]==relation && image["number"]==2 && image["type_oid"]==23));
            }
            if let Some(prior) = previous.take() {
                let gone_number = 14;
                let expected = if phase==2 { prior.into_iter().filter(|image: &Value| !(image["relation_oid"]==root && image["number"]==gone_number)).collect() } else {prior};
                assert_eq!(images,expected,"default/drop history changed native image identity or bytes");
            }
            previous = Some(images.clone());
            if phase==2 {
                let dropped = columns.attributes.iter().find(|fact|fact["relation_oid"]==root && fact["number"]==14).unwrap();
                assert_eq!(dropped["dropped"],true);
                assert_eq!(dropped["has_missing"],false);
                assert_eq!(dropped["type_oid"],0);
            } else {
                assert_eq!(named("gone")["has_default"],phase==0);
            }
            // Independent ordinary native heap scan positively distinguishes
            // the old absent slots from a new physically present NULL tuple.
            let mut natts: Vec<i32> = observer.query_one(&format!("SELECT {qschema}.physical_natts($1::oid::regclass)"),&[&root]).await.unwrap().get(0);
            natts.sort_unstable();
            assert_eq!(natts,vec![1,14]);
            let values = observer.query(&format!("SELECT pg_catalog.json_build_object('id',id,'nullable',nullable,'z',z,'b',b,'tiny',tiny,'plain',plain,'p_ok',p=repeat('pq',8192),'l_ok',l=repeat('uv',8192),'dm',dm::integer,'en',en::text,'arr',arr,'pair',pair::text,'empty',empty,'all_missing_null',z IS NULL AND b IS NULL AND tiny IS NULL AND plain IS NULL AND p IS NULL AND l IS NULL AND dm IS NULL AND en IS NULL AND arr IS NULL AND pair IS NULL AND empty IS NULL)::text FROM {qschema}.t ORDER BY id"),&[]).await.unwrap().into_iter()
                .map(|row|serde_json::from_str::<Value>(&row.get::<_,String>(0)).unwrap()).collect::<Vec<_>>();
            assert_eq!(values,vec![
                json!({"id":1,"nullable":null,"z":0,"b":false,"tiny":"yes","plain":"xy".repeat(100),"p_ok":true,"l_ok":true,"dm":7,"en":"stored","arr":[1,null,3],"pair":"(7,pair)","empty":"","all_missing_null":false}),
                json!({"id":2,"nullable":null,"z":null,"b":null,"tiny":null,"plain":null,"p_ok":null,"l_ok":null,"dm":null,"en":null,"arr":null,"pair":null,"empty":null,"all_missing_null":true})
            ]);
            for established in [false,true] {
                sql(&reader,if established { "BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT 1" } else { "BEGIN" }).await.unwrap();
                for _ in 0..2 {
                    let state = capture(&reader,&roots,false).await;
                    check_scope(&state,established,false);
                    check_columns(&state,&columns);
                    check_missing(&state,&images);
                    check_graph_modes(&state,&graph,&masks);
                    assert_eq!(state["metadata"]["root_facts"][0],state["metadata"]["root_facts"][3]);
                    assert_eq!(state["metadata"]["root_facts"][0],state["metadata"]["root_facts"][4]);
                    assert!(probe.modes(&observer,Some(backend),&fact_oids(&state)).await.is_empty());
                    probe.no_coordination(&observer,backend).await;
                }
                sql(&reader,"COMMIT").await.unwrap();
            }
        }
    })).catch_unwind().await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        &format!("DROP SCHEMA IF EXISTS {qschema} CASCADE; RESET default_toast_compression"),
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn prepared_column_and_type_changes_refresh_after_both_normal_outcomes() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (writer, wd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let gids = ["attribute_facts_commit", "attribute_facts_abort"];
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,"CREATE SCHEMA attribute_facts_wait; CREATE TYPE attribute_facts_wait.e AS ENUM ('value'); CREATE TABLE attribute_facts_wait.t(old_name attribute_facts_wait.e,n integer); INSERT INTO attribute_facts_wait.t VALUES('value',1); ALTER TABLE attribute_facts_wait.t ADD COLUMN stable integer DEFAULT 5").await.unwrap();
        create_missing_oracles(&observer,"attribute_facts_wait").await;
        let root = oid(&observer,"attribute_facts_wait","t").await;
        let type_oid: u32 = observer.query_one("SELECT atttypid FROM pg_catalog.pg_attribute WHERE attrelid=$1 AND attnum=1",&[&root]).await.unwrap().get(0);
        let roots = json!([["attribute_facts_wait","t",1],["attribute_facts_wait","t",3]]);
        let mut cycle = 0;
        for established in [false,true] {
            for (gid,ending) in gids.iter().zip(["COMMIT","ROLLBACK"]) {
                let before = column_oracle(&observer,&[root]).await;
                let before_images = missing_oracle(&observer,"attribute_facts_wait",&[root]).await;
                let current_column = before.attributes[0]["name"].as_str().unwrap();
                let current_type = before.types.iter().find(|fact|fact["oid"]==type_oid).unwrap()["name"].as_str().unwrap();
                let next_column = format!("renamed_{cycle}");
                let next_type = format!("type_{cycle}");
                let next_added = format!("added_{cycle}");
                let next_value = format!("v_{cycle}");
                cycle += 1;
                sql(&reader,if established { "BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT 1" } else { "BEGIN" }).await.unwrap();
                sql(&writer,&format!("BEGIN; ALTER TABLE attribute_facts_wait.t RENAME COLUMN {} TO {}; ALTER TABLE attribute_facts_wait.t ADD COLUMN {} varchar(11) DEFAULT '{next_value}'; ALTER TYPE attribute_facts_wait.{} RENAME TO {}; PREPARE TRANSACTION '{gid}'",quoted(current_column),quoted(&next_column),quoted(&next_added),quoted(current_type),quoted(&next_type))).await.unwrap();
                let request = storage_command(&roots,false);
                let waiting = command(&reader,&request);
                tokio::pin!(waiting);
                tokio::select! {
                    result = &mut waiting => panic!("prepared attribute acquisition completed early: {result:?}"),
                    () = probe.wait_physical(&observer,backend,root) => {},
                }
                probe.no_coordination(&observer,backend).await;
                assert!(probe.modes(&observer,Some(backend),&[1247,1249,1259,2610,2615]).await.is_empty());
                sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
                waiting.await.unwrap();
                let expected = column_oracle(&observer,&[root]).await;
                let images = missing_oracle(&observer,"attribute_facts_wait",&[root]).await;
                let state = status(&reader).await;
                check_scope(&state,established,false);
                check_columns(&state,&expected);
                check_missing(&state,&images);
                check_graph(&state,&oracle(&observer,root).await,10);
                assert_eq!(expected.roots[&root]["row_type_oid"],before.roots[&root]["row_type_oid"]);
                if ending=="COMMIT" {
                    assert_eq!(expected.attributes.len(),before.attributes.len()+1);
                    assert_eq!(expected.attributes[0]["name"],next_column);
                    assert_eq!(images.len(),before_images.len()+1);
                    let value: String = observer.query_one(&format!("SELECT {} FROM attribute_facts_wait.t",quoted(&next_added)),&[]).await.unwrap().get(0);
                    assert_eq!(value,next_value);
                    assert_eq!(expected.types.iter().find(|fact|fact["oid"]==type_oid).unwrap()["name"],next_type);
                } else {
                    // Rollback need not publish a target relcache SI message.
                    // Compare restored current declarations, not a guessed retry.
                    assert_eq!(expected.attributes,before.attributes);
                    assert_eq!(expected.types,before.types);
                    assert_eq!(images,before_images);
                }
                assert!(probe.modes(&observer,Some(backend),&fact_oids(&state)).await.is_empty());
                probe.no_coordination(&observer,backend).await;
                sql(&reader,"COMMIT").await.unwrap();
            }
        }
    })).catch_unwind().await;
    finish_targets(&observer, &gids).await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&writer, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        "DROP SCHEMA IF EXISTS attribute_facts_wait CASCADE",
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(writer, wd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn native_null_and_generation_bits_remain_declarations() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        let version: i32 = observer.query_one("SELECT current_setting('server_version_num')::integer",&[]).await.unwrap().get(0);
        assert!((170000..190000).contains(&version));
        sql(&observer,"CREATE SCHEMA attribute_facts_null; CREATE TABLE attribute_facts_null.t(required integer NOT NULL,n integer); INSERT INTO attribute_facts_null.t VALUES(1,NULL)").await.unwrap();
        let root = oid(&observer,"attribute_facts_null","t").await;
        if version>=180000 {
            // Pinned REL_18_6 ConstraintElem accepts NOT NULL ColId, with no
            // parentheses, and records skip_validation from NOT VALID.
            sql(&observer,"ALTER TABLE attribute_facts_null.t ADD CONSTRAINT n_declared NOT NULL n NOT VALID; ALTER TABLE attribute_facts_null.t ADD COLUMN virtual_value integer GENERATED ALWAYS AS(COALESCE(n,0)+1) VIRTUAL").await.unwrap();
            let validated: bool = observer.query_one("SELECT convalidated FROM pg_catalog.pg_constraint WHERE conrelid=$1 AND conname='n_declared' AND contype='n'",&[&root]).await.unwrap().get(0);
            assert!(!validated);
        }
        let nulls: i64 = observer.query_one("SELECT count(*) FROM attribute_facts_null.t WHERE n IS NULL",&[]).await.unwrap().get(0);
        assert_eq!(nulls,1);
        let expected = column_oracle(&observer,&[root]).await;
        assert_eq!(expected.attributes[0]["not_null_declared"],true);
        assert_eq!(expected.attributes[1]["not_null_declared"],version>=180000);
        if version>=180000 {
            assert_eq!(expected.attributes.len(),3);
            assert_eq!(expected.attributes[2]["generated"],u64::from(b'v'));
            assert_eq!(expected.attributes[2]["has_default"],true);
        } else { assert_eq!(expected.attributes.len(),2); }
        sql(&reader,"BEGIN").await.unwrap();
        let state = capture(&reader,&json!([["attribute_facts_null","t",1]]),false).await;
        check_scope(&state,false,false);
        check_columns(&state,&expected);
        sql(&reader,"ROLLBACK").await.unwrap();
    })).catch_unwind().await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        "DROP SCHEMA IF EXISTS attribute_facts_null CASCADE",
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
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
        let columns = column_oracle(&observer, &[a, b]).await;
        let roots = json!([[schema,second,3],[schema,first,1],[schema,first,3],[schema,second,1],[schema,first,2],[schema,second,2],[schema,first,1]]);
        for established in [false,true] {
            sql(&reader, if established { "BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT 1" } else { "BEGIN" }).await.unwrap();
            let state = capture(&reader,&roots,false).await;
            check_scope(&state,established,false);
            check_graph(&state,&expected,14);
            check_columns(&state, &columns);
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
            check_columns(&again, &columns);
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
        sql(&observer,"CREATE SCHEMA heap_storage_retain; CREATE TABLE heap_storage_retain.t(id integer PRIMARY KEY,body text); INSERT INTO heap_storage_retain.t VALUES(1,'body'); ALTER TABLE heap_storage_retain.t ADD COLUMN missing text DEFAULT 'retained'").await.unwrap();
        create_missing_oracles(&observer,"heap_storage_retain").await;
        let root = oid(&observer,"heap_storage_retain","t").await;
        let expected = oracle(&observer,root).await;
        let columns = column_oracle(&observer, &[root]).await;
        let images = missing_oracle(&observer,"heap_storage_retain",&[root]).await;
        assert_eq!(images.len(),1);
        let database = expected[0]["file_database_oid"].as_u64().unwrap() as u32;
        let roots = json!([["heap_storage_retain","t",1],["heap_storage_retain","t",3]]);
        sql(&reader,"BEGIN").await.unwrap();
        command(&reader,&format!("relation_borrow:{root}/1")).await.unwrap();
        sql(&reader,"SAVEPOINT child").await.unwrap();
        let state = capture(&reader,&roots,true).await;
        check_scope(&state,false,true);
        check_graph(&state,&expected,10);
        check_columns(&state, &columns);
        check_missing(&state,&images);
        let oids = fact_oids(&state);
        sql(&reader,"ROLLBACK TO child; RELEASE child").await.unwrap();
        assert_eq!(probe.modes(&observer,Some(backend),&oids).await,vec![(root,database,"AccessShareLock".to_owned(),true)]);
        sql(&reader,"SAVEPOINT child").await.unwrap();
        let child = capture(&reader,&roots,true).await;
        check_columns(&child,&columns);
        check_missing(&child,&images);
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
            check_columns(&retained,&columns);
            check_missing(&retained,&images);
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
            let columns = column_oracle(&observer, &[root]).await;
            assert_eq!(expected[0]["stored_file_number"],0);
            assert_ne!(expected[0]["file_number"],0);
            sql(&reader,"BEGIN").await.unwrap();
            let state = capture(&reader,&json!([["pg_catalog",name,1]]),true).await;
            check_scope(&state,false,true);
            check_graph(&state,&expected,2);
            check_columns(&state, &columns);
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
        let columns = column_oracle(&reader, &[root]).await;
        sql(&reader,"BEGIN").await.unwrap();
        let state = capture(&reader,&json!([[temp_schema,"heap_storage_temp",3]]),true).await;
        check_scope(&state,false,true);
        check_graph(&state,&expected,8);
        check_columns(&state, &columns);
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
        let parent_columns = column_oracle(&observer, &[parent]).await;
        let child = oid(&observer,"heap_storage_limits","child").await;
        let child_expected = oracle(&observer,child).await;
        let child_columns = column_oracle(&observer, &[child]).await;
        assert_eq!(child_columns.attributes.len(),1);
        assert_eq!(child_columns.attributes[0]["local"],false);
        assert!(child_columns.attributes[0]["inheritance_count"].as_i64().unwrap() > 0);
        let all: Vec<u32> = observer.query("SELECT oid FROM pg_catalog.pg_class WHERE relnamespace=(SELECT oid FROM pg_catalog.pg_namespace WHERE nspname='heap_storage_limits')",&[]).await.unwrap().into_iter().map(|row| row.get(0)).collect();
        // Inheritance is a copied column declaration. An exact child binding
        // neither expands its parent nor selects a data snapshot during copying.
        for established in [false,true] {
            sql(&reader,if established { "BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT 1" } else { "BEGIN" }).await.unwrap();
            let child_state = capture(&reader,&json!([["heap_storage_limits","child",1]]),false).await;
            check_scope(&child_state,established,false);
            check_graph(&child_state,&child_expected,2);
            check_columns(&child_state,&child_columns);
            assert_eq!(child_state["metadata"]["roots"],json!([[child,1]]));
            assert_eq!(fact_oids(&child_state),vec![child]);
            assert!(probe.modes(&observer,Some(backend),&all).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"COMMIT").await.unwrap();
        }
        sql(&reader,"BEGIN").await.unwrap();
        let parent_state = capture(&reader,&json!([["heap_storage_limits","parent",1]]),false).await;
        check_scope(&parent_state,false,false);
        check_graph(&parent_state,&parent_expected,2);
        check_columns(&parent_state, &parent_columns);
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
                assert!(probe.modes(&observer,Some(backend),&[1247,1249,1259,2610,2615]).await.is_empty());
                sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
                waiting.await.unwrap();
                let current = oid(&observer,"heap_storage_wait","t").await;
                if ending == "COMMIT" { assert_ne!(current,old); } else { assert_eq!(current,old); }
                let expected = oracle(&observer,current).await;
                let columns = column_oracle(&observer, &[current]).await;
                let state = status(&reader).await;
                check_scope(&state,established,false);
                check_graph(&state,&expected,2);
            check_columns(&state, &columns);
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
            let columns = column_oracle(&observer, &[root]).await;
            sql(&reader,"BEGIN").await.unwrap();
            let state = capture(&reader,&json!([["heap_storage_live","t",1]]),false).await;
            check_scope(&state,false,false);
            check_graph(&state,&expected,2);
            check_columns(&state, &columns);
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
