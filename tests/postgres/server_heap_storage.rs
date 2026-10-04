//! Private catalog-declared storage, not application SQL or full semantic closure.
use futures_util::{FutureExt, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    future::Future,
    panic::AssertUnwindSafe,
    time::{Duration, Instant},
};
use tokio_postgres::{Client, NoTls, SimpleQueryEvent, Statement, error::SqlState};

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const DEADLINE: Duration = Duration::from_secs(20);
const CASE_DEADLINE: Duration = Duration::from_secs(60);

struct OracleTiming {
    phase: &'static str,
    started: Instant,
    outcome: &'static str,
}

impl Drop for OracleTiming {
    fn drop(&mut self) {
        eprintln!(
            "oracle phase={} outcome={} elapsed={:.3}s",
            self.phase,
            self.outcome,
            self.started.elapsed().as_secs_f64()
        );
    }
}

async fn oracle_read<T>(phase: &'static str, operation: impl Future<Output = T>) -> T {
    eprintln!("oracle phase={phase} started");
    let mut timing = OracleTiming {
        phase,
        started: Instant::now(),
        outcome: "interrupted",
    };
    let result = tokio::time::timeout(DEADLINE, operation).await;
    timing.outcome = if result.is_ok() {
        "completed"
    } else {
        "timeout"
    };
    result.unwrap_or_else(|error| panic!("bounded oracle phase {phase} expired: {error}"))
}

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

async fn physical_oracles(client: &Client, roots: &[u32]) -> BTreeMap<u32, Vec<Value>> {
    let rows = oracle_read("physical graphs", client.query(
        "WITH heaps AS (SELECT oid AS root_oid,oid,0::oid AS parent_oid,0 AS phase FROM pg_catalog.pg_class WHERE oid=ANY($1) UNION ALL SELECT r.oid,t.oid,r.oid,2 FROM pg_catalog.pg_class r JOIN pg_catalog.pg_class t ON t.oid=r.reltoastrelid WHERE r.oid=ANY($1)), nodes AS (SELECT root_oid,oid,parent_oid,phase FROM heaps UNION ALL SELECT h.root_oid,i.indexrelid,h.oid,h.phase+1 FROM heaps h JOIN pg_catalog.pg_index i ON i.indrelid=h.oid WHERE i.indislive) SELECT x.root_oid,pg_catalog.json_build_object('oid',c.oid::bigint,'schema_oid',n.oid::bigint,'schema',n.nspname,'name',c.relname,'kind',pg_catalog.ascii(c.relkind::text),'persistence',pg_catalog.ascii(c.relpersistence::text),'am',c.relam::bigint,'toast_oid',c.reltoastrelid::bigint,'parent_oid',x.parent_oid::bigint,'shared',c.relisshared,'is_partition',c.relispartition,'has_indexes',c.relhasindex,'has_subclasses',c.relhassubclass,'live',COALESCE(i.indislive,false),'ready',COALESCE(i.indisready,false),'valid',COALESCE(i.indisvalid,false),'check_xmin',COALESCE(i.indcheckxmin,false),'tablespace_oid',c.reltablespace::bigint,'stored_file_number',c.relfilenode::bigint,'row_type_oid',c.reltype::bigint,'declared_attribute_count',c.relnatts,'declared_check_count',c.relchecks,'rules_hint',c.relhasrules,'triggers_hint',c.relhastriggers,'file_tablespace_oid',COALESCE(NULLIF(c.reltablespace,0),(SELECT dattablespace FROM pg_catalog.pg_database WHERE datname=current_database()))::bigint,'file_database_oid',(CASE WHEN c.relisshared THEN 0::oid ELSE (SELECT oid FROM pg_catalog.pg_database WHERE datname=current_database()) END)::bigint,'file_number',pg_catalog.pg_relation_filenode(c.oid::regclass)::bigint,'file_proc_number',CASE WHEN c.relpersistence='t' THEN substring(pg_catalog.pg_relation_filepath(c.oid::regclass) FROM '/t([0-9]+)_')::integer ELSE -1 END)::text FROM nodes x JOIN pg_catalog.pg_class c ON c.oid=x.oid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace LEFT JOIN pg_catalog.pg_index i ON i.indexrelid=c.oid ORDER BY x.root_oid,x.phase,c.oid", &[&roots],
    )).await.unwrap();
    let mut graphs = BTreeMap::<u32, Vec<Value>>::new();
    for row in rows {
        graphs
            .entry(row.get(0))
            .or_default()
            .push(serde_json::from_str(&row.get::<_, String>(1)).unwrap());
    }
    graphs
}

async fn oracle(client: &Client, root: u32) -> Vec<Value> {
    physical_oracles(client, &[root])
        .await
        .remove(&root)
        .unwrap()
}

struct OracleSelection {
    roots: Vec<u32>,
    types: Vec<u32>,
    composites: Vec<u32>,
}

struct ColumnOracle {
    selection: OracleSelection,
    roots: BTreeMap<u32, Value>,
    composites: Vec<Value>,
    attributes: Vec<Value>,
    types: Vec<Value>,
    scanned: Value,
}

// One independent selection is shared by all projections in a metadata phase.
// Callers create a fresh selection after each DDL/native prepared completion.
async fn oracle_selection(client: &Client, roots: &[u32]) -> OracleSelection {
    let rows = oracle_read("type/composite selection", client.query(
        "WITH RECURSIVE selected(oid) AS ((SELECT reltype FROM pg_catalog.pg_class WHERE oid=ANY($1) AND reltype<>0 UNION SELECT atttypid FROM pg_catalog.pg_attribute WHERE attrelid=ANY($1) AND attnum>0 AND NOT attisdropped AND atttypid<>0) UNION SELECT edge.oid FROM selected s JOIN pg_catalog.pg_type t ON t.oid=s.oid CROSS JOIN LATERAL (SELECT oid FROM (VALUES(t.typbasetype),(t.typelem),(t.typarray)) AS declared(oid) UNION SELECT a.atttypid FROM pg_catalog.pg_attribute a WHERE t.typtype='c' AND a.attrelid=t.typrelid AND a.attnum>0 AND NOT a.attisdropped) AS edge(oid) WHERE edge.oid<>0) SELECT s.oid,CASE WHEN t.typtype='c' THEN t.typrelid ELSE 0::oid END FROM selected s JOIN pg_catalog.pg_type t ON t.oid=s.oid ORDER BY s.oid", &[&roots],
    )).await.unwrap();
    let mut types = Vec::new();
    let mut composites = Vec::new();
    for row in rows {
        types.push(row.get(0));
        let composite: u32 = row.get(1);
        if composite != 0 {
            composites.push(composite);
        }
    }
    composites.sort_unstable();
    assert!(composites.windows(2).all(|pair| pair[0] != pair[1]));
    let mut roots = roots.to_vec();
    roots.sort_unstable();
    roots.dedup();
    OracleSelection {
        roots,
        types,
        composites,
    }
}

async fn column_oracle(client: &Client, roots: &[u32]) -> ColumnOracle {
    oracle_read("column metadata", async {
        let root_rows = client.query(
            "SELECT oid,pg_catalog.json_build_object('oid',oid::bigint,'row_type_oid',reltype::bigint,'declared_attribute_count',relnatts)::text FROM pg_catalog.pg_class WHERE oid=ANY($1) ORDER BY oid", &[&roots],
        ).await.unwrap();
        let roots = root_rows.into_iter().map(|row| {
            (row.get::<_, u32>(0), serde_json::from_str(&row.get::<_, String>(1)).unwrap())
        }).collect::<BTreeMap<_, Value>>();
        let oids = roots.keys().copied().collect::<Vec<_>>();
        let selection = oracle_selection(client,&oids).await;
        let type_oids = &selection.types;
        let composites: Vec<Value> = client.query(
            "SELECT pg_catalog.json_build_object('type_oid',t.oid::bigint,'relation_oid',c.oid::bigint,'schema_oid',c.relnamespace::bigint,'schema',n.nspname,'name',c.relname,'kind',pg_catalog.ascii(c.relkind::text),'persistence',pg_catalog.ascii(c.relpersistence::text),'am',c.relam::bigint,'is_partition',c.relispartition,'declared_attribute_count',c.relnatts,'attribute_offset',COALESCE(sum(c.relnatts) OVER (ORDER BY c.oid ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING),0))::text FROM pg_catalog.pg_type t JOIN pg_catalog.pg_class c ON c.oid=t.typrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE t.oid=ANY($1) AND t.typtype='c' ORDER BY c.oid", &[&type_oids],
        ).await.unwrap().into_iter().map(|row| serde_json::from_str(&row.get::<_, String>(0)).unwrap()).collect();
        let composite_oids = &selection.composites;
        assert_eq!(*composite_oids, composites.iter().map(|fact|u32::try_from(fact["relation_oid"].as_u64().unwrap()).unwrap()).collect::<Vec<_>>());
        let attributes = client.query(
            "SELECT pg_catalog.json_build_object('relation_oid',attrelid::bigint,'number',attnum,'name',attname,'type_oid',atttypid::bigint,'length',attlen,'typmod',atttypmod,'dimensions',attndims,'by_value',attbyval,'alignment',pg_catalog.ascii(attalign::text),'storage',pg_catalog.ascii(attstorage::text),'compression',CASE WHEN attcompression::text='' THEN 0 ELSE pg_catalog.ascii(attcompression::text) END,'not_null_declared',attnotnull,'has_default',atthasdef,'has_missing',atthasmissing,'identity',CASE WHEN attidentity::text='' THEN 0 ELSE pg_catalog.ascii(attidentity::text) END,'generated',CASE WHEN attgenerated::text='' THEN 0 ELSE pg_catalog.ascii(attgenerated::text) END,'dropped',attisdropped,'local',attislocal,'inheritance_count',attinhcount,'collation_oid',attcollation::bigint)::text FROM pg_catalog.pg_attribute WHERE attrelid=ANY($1) AND attnum>0 ORDER BY attrelid,attnum", &[&composite_oids],
        ).await.unwrap().into_iter().map(|row| serde_json::from_str(&row.get::<_, String>(0)).unwrap()).collect();
        let types = client.query(
            "SELECT pg_catalog.json_build_object('oid',t.oid::bigint,'schema_oid',t.typnamespace::bigint,'schema',n.nspname,'name',t.typname,'length',t.typlen,'by_value',t.typbyval,'kind',pg_catalog.ascii(t.typtype::text),'category',pg_catalog.ascii(t.typcategory::text),'preferred',t.typispreferred,'defined',t.typisdefined,'delimiter',pg_catalog.ascii(t.typdelim::text),'relation_oid',t.typrelid::bigint,'subscript_oid',t.typsubscript::oid::bigint,'element_oid',t.typelem::bigint,'array_oid',t.typarray::bigint,'input_oid',t.typinput::oid::bigint,'output_oid',t.typoutput::oid::bigint,'receive_oid',t.typreceive::oid::bigint,'send_oid',t.typsend::oid::bigint,'typmod_input_oid',t.typmodin::oid::bigint,'typmod_output_oid',t.typmodout::oid::bigint,'analyze_oid',t.typanalyze::oid::bigint,'alignment',pg_catalog.ascii(t.typalign::text),'storage',pg_catalog.ascii(t.typstorage::text),'not_null_declared',t.typnotnull,'base_type_oid',t.typbasetype::bigint,'typmod',t.typtypmod,'dimensions',t.typndims,'collation_oid',t.typcollation::bigint)::text FROM pg_catalog.pg_type t JOIN pg_catalog.pg_namespace n ON n.oid=t.typnamespace WHERE t.oid=ANY($1) ORDER BY t.oid", &[&type_oids],
        ).await.unwrap().into_iter().map(|row| serde_json::from_str(&row.get::<_, String>(0)).unwrap()).collect();
        let scanned = serde_json::from_str(&client.query_one(
            "SELECT pg_catalog.json_build_object('namespace_rows',(SELECT count(*) FROM pg_catalog.pg_namespace),'relation_rows',(SELECT count(*) FROM pg_catalog.pg_class),'options_rows',(SELECT count(*) FROM pg_catalog.pg_class),'index_rows',(SELECT count(*) FROM pg_catalog.pg_index),'attribute_rows',(SELECT count(*) FROM pg_catalog.pg_attribute),'attribute_payload_rows',(SELECT count(*) FROM pg_catalog.pg_attribute),'type_rows',(SELECT count(*) FROM pg_catalog.pg_type),'type_payload_rows',(SELECT count(*) FROM pg_catalog.pg_type),'attrdef_rows',(SELECT count(*) FROM pg_catalog.pg_attrdef))::text", &[],
        ).await.unwrap().get::<_, String>(0)).unwrap();
        ColumnOracle { selection, roots, composites, attributes, types, scanned }
    }).await
}

async fn create_missing_oracles(client: &Client, schema: &str) {
    let quoted_schema = quoted(schema);
    sql(client, &format!(
        "CREATE FUNCTION {quoted_schema}.image(anyarray) RETURNS bytea AS '$libdir/darmok_catalog_probe','darmok_test_missing_array_image' LANGUAGE C STRICT; CREATE FUNCTION {quoted_schema}.physical_natts(regclass) RETURNS integer[] AS '$libdir/darmok_catalog_probe','darmok_test_heap_natts' LANGUAGE C STRICT"
    )).await.unwrap();
    create_payload_oracles(client, schema).await;
}

async fn missing_oracle(client: &Client, schema: &str, selection: &OracleSelection) -> Vec<Value> {
    let schema = quoted(schema);
    let composite_oids = &selection.composites;
    oracle_read("missing images", client.query(&format!(
        "WITH observed AS MATERIALIZED (SELECT attrelid,attnum,atttypid,pg_catalog.pg_column_size(attmissingval) stored_bytes,pg_catalog.pg_column_compression(attmissingval) compression,{schema}.image(attmissingval) image,pg_catalog.array_ndims(attmissingval) ndim,pg_catalog.array_length(attmissingval,1) dim,pg_catalog.array_lower(attmissingval,1) lbound FROM pg_catalog.pg_attribute WHERE attrelid=ANY($1) AND attnum>0 AND NOT attisdropped AND atthasmissing) SELECT pg_catalog.json_build_object('relation_oid',attrelid::bigint,'number',attnum,'type_oid',atttypid::bigint,'carrier_kind',CASE WHEN compression='pglz' THEN 'p' WHEN compression='lz4' THEN 'l' WHEN compression IS NULL AND stored_bytes=pg_catalog.octet_length(image)-3 THEN 's' WHEN compression IS NULL AND stored_bytes=pg_catalog.octet_length(image) THEN 'u' ELSE 'unexpected-native-form' END,'stored_bytes',stored_bytes,'image_bytes',pg_catalog.octet_length(image),'image',pg_catalog.encode(image,'hex'))::text,ndim,dim,lbound FROM observed ORDER BY attrelid,attnum"
    ), &[&composite_oids])).await.unwrap()
        .into_iter().map(|row|{
            for column in 1..=3 { assert_eq!(row.get::<_,Option<i32>>(column),Some(1)); }
            serde_json::from_str(&row.get::<_,String>(0)).unwrap()
        }).collect()
}

fn check_missing(state: &Value, expected: &[Value]) {
    assert_eq!(state["metadata"]["missing"], json!(expected));
    assert_eq!(state["metadata"]["missing_count"], expected.len());
}

async fn create_payload_oracles(client: &Client, schema: &str) {
    let schema = quoted(schema);
    for argument in ["pg_catalog.pg_node_tree", "text", "text[]"] {
        for (name, result, symbol) in [
            ("payload_image", "bytea", "darmok_test_varlena_image"),
            ("payload_carrier", "text", "darmok_test_varlena_carrier"),
        ] {
            sql(client, &format!("CREATE FUNCTION {schema}.{name}({argument}) RETURNS {result} AS '$libdir/darmok_catalog_probe','{symbol}' LANGUAGE C STRICT")).await.unwrap();
        }
    }
}

fn payload_expected(
    catalog: u32,
    row: u32,
    relation: u32,
    number: i16,
    field: i16,
    shape: Option<String>,
    image: Option<Vec<u8>>,
) -> Value {
    let mut value = if let Some(shape) = shape {
        serde_json::from_str::<Value>(&shape).unwrap()
    } else {
        assert!(image.is_none());
        json!({"present":false,"carrier_kind":"","compression_kind":0,"toast_oid":0,"value_oid":0,"carrier_bytes":0,"stored_bytes":0})
    };
    let bytes = image.as_ref().map_or(0, Vec::len);
    if value["present"] == true {
        assert!(bytes >= 4);
    }
    value["image_bytes"] = json!(bytes);
    value["image"] = json!(image.map(|image| {
        image
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    }));
    json!({"catalog_oid":catalog,"row_oid":row,"relation_oid":relation,"number":number,"field_number":field,"value":value})
}

async fn payload_oracle(client: &Client, schema: &str, selection: &OracleSelection) -> Vec<Value> {
    let schema = quoted(schema);
    oracle_read("catalog payloads", async {
        let mut payloads = Vec::new();
        let type_oids = &selection.types;
        let composite_oids = &selection.composites;
        let roots = &selection.roots;
        for row in client.query(&format!(
            "SELECT 'pg_catalog.pg_attrdef'::regclass::oid,d.oid,d.adrelid,d.adnum,a.attnum,{schema}.payload_carrier(d.adbin),{schema}.payload_image(d.adbin) FROM pg_catalog.pg_attrdef d CROSS JOIN pg_catalog.pg_attribute a WHERE d.adrelid=ANY($1) AND a.attrelid='pg_catalog.pg_attrdef'::regclass AND a.attname='adbin' ORDER BY d.oid"
        ), &[&composite_oids]).await.unwrap() {
            payloads.push(payload_expected(row.get(0),row.get(1),row.get(2),row.get(3),row.get(4),row.get(5),row.get(6)));
        }
        for field in ["typdefaultbin", "typdefault"] {
            for row in client.query(&format!(
                "SELECT 'pg_catalog.pg_type'::regclass::oid,t.oid,a.attnum,{schema}.payload_carrier(t.{field}),{schema}.payload_image(t.{field}) FROM pg_catalog.pg_type t CROSS JOIN pg_catalog.pg_attribute a WHERE t.oid=ANY($1) AND a.attrelid='pg_catalog.pg_type'::regclass AND a.attname=$2 ORDER BY t.oid"
            ), &[&type_oids,&field]).await.unwrap() {
                payloads.push(payload_expected(row.get(0),row.get(1),0,0,row.get(2),row.get(3),row.get(4)));
            }
        }
        for row in client.query(&format!(
            "WITH roots AS (SELECT unnest($1::oid[]) oid UNION SELECT unnest(ARRAY['pg_catalog.pg_namespace'::regclass::oid,'pg_catalog.pg_class'::regclass::oid,'pg_catalog.pg_index'::regclass::oid,'pg_catalog.pg_attribute'::regclass::oid,'pg_catalog.pg_type'::regclass::oid,'pg_catalog.pg_attrdef'::regclass::oid])), heaps AS (SELECT c.oid FROM pg_catalog.pg_class c JOIN roots r ON r.oid=c.oid UNION SELECT c.reltoastrelid FROM pg_catalog.pg_class c JOIN roots r ON r.oid=c.oid WHERE c.reltoastrelid<>0), nodes AS (SELECT oid FROM heaps UNION SELECT i.indexrelid FROM pg_catalog.pg_index i JOIN heaps h ON h.oid=i.indrelid WHERE i.indislive) SELECT 'pg_catalog.pg_class'::regclass::oid,c.oid,a.attnum,{schema}.payload_carrier(c.reloptions),{schema}.payload_image(c.reloptions) FROM nodes x JOIN pg_catalog.pg_class c ON c.oid=x.oid CROSS JOIN pg_catalog.pg_attribute a WHERE a.attrelid='pg_catalog.pg_class'::regclass AND a.attname='reloptions' ORDER BY c.oid"
        ), &[&roots]).await.unwrap() {
            payloads.push(payload_expected(row.get(0),row.get(1),0,0,row.get(2),row.get(3),row.get(4)));
        }
        payloads.sort_by_key(|fact|(fact["catalog_oid"].as_u64().unwrap(),fact["row_oid"].as_u64().unwrap(),fact["field_number"].as_u64().unwrap()));
        payloads
    }).await
}

fn options_payload(payloads: &[Value], relation: u32) -> &Value {
    payloads
        .iter()
        .find(|fact| fact["catalog_oid"] == 1259 && fact["row_oid"] == relation)
        .expect("selected graph relation must have its own options source")
}

fn type_payloads(state: &Value, type_oid: u32) -> Vec<Value> {
    state["metadata"]["payloads"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|fact| fact["catalog_oid"] == 1247 && fact["row_oid"] == type_oid)
        .cloned()
        .collect()
}

fn type_fact(state: &Value, type_oid: u32) -> &Value {
    state["metadata"]["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fact| fact["oid"] == type_oid)
        .expect("selected native type must be present")
}

fn check_payloads(state: &Value, expected: &[Value]) {
    assert_eq!(state["metadata"]["payloads"], json!(expected));
    assert_eq!(state["metadata"]["payload_count"], expected.len());
    let mut carrier_bytes = 0_u64;
    let mut stored_bytes = 0_u64;
    let mut image_bytes = 0_u64;
    let mut heaps = std::collections::BTreeSet::new();
    let mut groups = std::collections::BTreeSet::new();
    for fact in expected {
        let value = &fact["value"];
        carrier_bytes += value["carrier_bytes"].as_u64().unwrap();
        stored_bytes += value["stored_bytes"].as_u64().unwrap();
        image_bytes += value["image_bytes"].as_u64().unwrap();
        if value["carrier_kind"] == "e" {
            heaps.insert(value["toast_oid"].as_u64().unwrap());
            groups.insert((
                value["toast_oid"].as_u64().unwrap(),
                value["value_oid"].as_u64().unwrap(),
            ));
        }
    }
    for phase in ["initial_cost", "source_cost", "final_cost"] {
        assert_eq!(
            state["metadata"][phase]["payload_carrier_bytes"],
            carrier_bytes
        );
        assert!(
            state["metadata"][phase]["requested_copy_bytes"]
                .as_u64()
                .unwrap()
                <= 64 * 1024 * 1024
        );
    }
    assert_eq!(state["metadata"]["payload_image_bytes"], image_bytes);
    assert_eq!(state["metadata"]["payload_stored_bytes"], stored_bytes);
    assert_eq!(state["metadata"]["toast_heaps"], heaps.len());
    let rows = state["metadata"]["toast_rows"].as_u64().unwrap();
    let chunks = state["metadata"]["selected_chunks"].as_u64().unwrap();
    assert!(rows >= chunks && chunks >= groups.len() as u64);
    if heaps.is_empty() {
        assert_eq!((rows, chunks), (0, 0));
    }
}

fn check_columns(state: &Value, expected: &ColumnOracle) {
    let metadata = &state["metadata"];
    assert_eq!(metadata["attributes"], json!(expected.attributes));
    assert_eq!(metadata["composites"], json!(expected.composites));
    assert_eq!(metadata["types"], json!(expected.types));
    let root_facts = metadata["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|binding| {
            let oid = u32::try_from(binding[0].as_u64().unwrap()).unwrap();
            let mut fact = expected.roots[&oid].clone();
            let composite = expected
                .composites
                .iter()
                .find(|fact| fact["relation_oid"] == oid)
                .unwrap();
            fact["attribute_offset"] = composite["attribute_offset"].clone();
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
    for phase in ["initial_cost", "source_cost", "final_cost"] {
        for (name, count) in expected.scanned.as_object().unwrap() {
            assert_eq!(&metadata[phase][name], count, "{phase}/{name}: {metadata}");
        }
        let allocated = metadata[phase]["allocated_bytes"].as_u64().unwrap();
        let arrays = metadata["attribute_array_bytes"].as_u64().unwrap()
            + metadata["composite_array_bytes"].as_u64().unwrap()
            + metadata["type_array_bytes"].as_u64().unwrap();
        assert_eq!(metadata[phase]["missing_carrier_bytes"], stored_bytes);
        assert!(allocated > 0 && allocated >= arrays);
    }
    for (name, count) in [
        ("attribute_array_bytes", expected.attributes.len()),
        ("composite_array_bytes", expected.composites.len()),
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

async fn catalog_graphs(client: &Client) -> BTreeMap<u32, Vec<Value>> {
    let names = vec![
        "pg_namespace",
        "pg_class",
        "pg_index",
        "pg_attribute",
        "pg_type",
        "pg_attrdef",
    ];
    let roots: Vec<u32> = oracle_read("catalog graph root names", client.query(
        "SELECT c.oid FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='pg_catalog' AND c.relname::text=ANY($1) ORDER BY c.oid",
        &[&names],
    )).await.unwrap().into_iter().map(|row|row.get(0)).collect();
    assert_eq!(roots.len(), 6);
    let graphs = physical_oracles(client, &roots).await;
    assert_eq!(graphs.len(), 6);
    graphs
}

async fn check_graph(client: &Client, state: &Value, expected: &[Value], root_mask: u64) {
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
    check_graph_modes(client, state, expected, &root_masks).await;
}

async fn check_graph_modes(
    client: &Client,
    state: &Value,
    application_facts: &[Value],
    root_masks: &BTreeMap<u32, u64>,
) {
    let catalogs = catalog_graphs(client).await;
    let mut applications = BTreeMap::<u32, Vec<Value>>::new();
    let mut root = 0;
    for fact in application_facts {
        if fact["parent_oid"] == 0 {
            root = u32::try_from(fact["oid"].as_u64().unwrap()).unwrap();
        }
        assert_ne!(root, 0);
        applications.entry(root).or_default().push(fact.clone());
    }
    let roots = catalogs
        .keys()
        .chain(applications.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut expected_facts = Vec::<Value>::new();
    let mut positions = BTreeMap::<u32, usize>::new();
    let mut uses = Vec::new();
    for root in roots {
        for (catalog, graph, root_mask) in [
            (
                false,
                applications.get(&root),
                root_masks.get(&root).copied().unwrap_or(0),
            ),
            (true, catalogs.get(&root), 2),
        ] {
            let Some(graph) = graph else {
                continue;
            };
            for fact in graph {
                let oid = u32::try_from(fact["oid"].as_u64().unwrap()).unwrap();
                let toast = fact["kind"] == u64::from(b't')
                    || graph.iter().any(|parent| {
                        parent["oid"] == fact["parent_oid"] && parent["kind"] == u64::from(b't')
                    });
                let mask = if toast {
                    2 | (root_mask & 8)
                } else {
                    root_mask
                };
                let position = if let Some(position) = positions.get(&oid) {
                    for (name, value) in fact.as_object().unwrap() {
                        assert_eq!(
                            &expected_facts[*position][name], value,
                            "shared graph fact {oid}/{name}"
                        );
                    }
                    *position
                } else {
                    if fact["parent_oid"] != 0 {
                        assert!(
                            positions.contains_key(&(fact["parent_oid"].as_u64().unwrap() as u32))
                        );
                    }
                    let position = expected_facts.len();
                    positions.insert(oid, position);
                    let mut copied = fact.clone();
                    copied["mode_mask"] = json!(0);
                    expected_facts.push(copied);
                    position
                };
                let previous = expected_facts[position]["mode_mask"].as_u64().unwrap();
                expected_facts[position]["mode_mask"] = json!(previous | mask);
                uses.push(
                    json!({"root_oid":root,"relation_oid":oid,"mode_mask":mask,"catalog":catalog}),
                );
            }
        }
    }
    let metadata = &state["metadata"];
    assert_eq!(metadata["facts"], json!(expected_facts));
    assert_eq!(metadata["uses"], json!(uses));
    let references = expected_facts
        .iter()
        .flat_map(|fact| {
            (1..=3)
                .filter(move |&mode| fact["mode_mask"].as_u64().unwrap() & (1 << mode) != 0)
                .map(move |mode| json!([fact["oid"], mode]))
        })
        .collect::<Vec<_>>();
    assert_eq!(metadata["references"], json!(references));
    assert!(references.len() <= 4096);
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

    async fn check_prepared_storage_wait(&self, client: &Client, pid: i32, established: bool) {
        self.no_coordination(client, pid).await;
        // Combined physical acquisition owns catalog AS before an application
        // root wait. A's registered/ephemeral snapshot must already be gone;
        // an entered RR data snapshot is preserved independently.
        let catalogs = vec![1247_u32, 1249, 1259, 2604, 2610, 2615];
        let locks = self.modes(client, Some(pid), &catalogs).await;
        assert_eq!(locks.iter().map(|row| row.0).collect::<Vec<_>>(), catalogs);
        assert!(locks.iter().all(|row| row.2 == "AccessShareLock" && row.3));
        self.check_data_horizon(client, pid, established).await;
    }

    async fn check_data_horizon(&self, client: &Client, pid: i32, established: bool) {
        let horizon: Option<String> = tokio::time::timeout(
            DEADLINE,
            client.query_one(
                "SELECT backend_xmin::text FROM pg_catalog.pg_stat_activity WHERE pid=$1",
                &[&pid],
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .get(0);
        assert_eq!(
            horizon.is_some(),
            established,
            "prepared wait retained an unexpected snapshot horizon"
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

    async fn wait_semantic(&self, client: &Client, pid: i32) {
        tokio::time::timeout(DEADLINE, async {
            loop {
                let rows = client.query(&self.coordination, &[&pid]).await.unwrap();
                if rows.iter().any(|row| {
                    row.get::<_, i32>(0) == 17487
                        && row.get::<_, String>(1) == "ShareLock"
                        && !row.get::<_, bool>(2)
                }) {
                    // C waits for S with neither raw nor publication locks.
                    assert_eq!(rows.len(), 1);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("native storage semantic wait was not observed");
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

fn storage_fact(state: &Value, oid: u32) -> &Value {
    state["metadata"]["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fact| fact["oid"] == oid)
        .unwrap()
}

fn application_fact_oids(state: &Value) -> Vec<u32> {
    state["metadata"]["uses"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|fact| fact["catalog"] == false)
        .map(|fact| u32::try_from(fact["relation_oid"].as_u64().unwrap()).unwrap())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
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

fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn payload_entropy(bytes: usize, seed: u64) -> String {
    let alphabet = (33_u8..=126)
        .filter(|byte| !matches!(*byte, b'\'' | b'\\'))
        .collect::<Vec<_>>();
    let mut state = seed;
    (0..bytes)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            char::from(alphabet[(state % alphabet.len() as u64) as usize])
        })
        .collect()
}

#[tokio::test]
async fn owned_defaults_cover_actual_native_carriers_and_independent_type_presence() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "payload_facts_字";
    let qschema = quoted(schema);
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE,async {
        let version: i32 = observer.query_one("SELECT current_setting('server_version_num')::integer",&[]).await.unwrap().get(0);
        assert!((170000..190000).contains(&version));
        let entropy = payload_entropy(32768,0x2359ac4ef12389ab);
        let repeated = payload_entropy(16384,0x5630ba981ef5cd29).repeat(3);
        sql(&observer,&format!("CREATE SCHEMA {qschema}; SET default_toast_compression='lz4'")).await.unwrap();
        // Native base types allow independent text-only defaults. Their fixture
        // input/output functions are internal text ABI aliases; copying never
        // calls these providers or substitutes their effective default.
        for (name,default) in [("raw",Some(entropy.as_str())),("empty",Some("")),("absent",None)] {
            let default = default.map(|value|format!(", DEFAULT={}",literal(value))).unwrap_or_default();
            sql(&observer,&format!("CREATE TYPE {qschema}.{name}; CREATE FUNCTION {qschema}.{name}_in(cstring) RETURNS {qschema}.{name} AS 'textin' LANGUAGE internal IMMUTABLE STRICT; CREATE FUNCTION {qschema}.{name}_out({qschema}.{name}) RETURNS cstring AS 'textout' LANGUAGE internal IMMUTABLE STRICT; CREATE TYPE {qschema}.{name}(INPUT={qschema}.{name}_in,OUTPUT={qschema}.{name}_out,INTERNALLENGTH=variable,ALIGNMENT=int4,STORAGE=extended{default})")).await.unwrap();
        }
        sql(&observer,&format!("SET default_toast_compression='pglz'; CREATE DOMAIN {qschema}.dp AS text DEFAULT {}; CREATE DOMAIN {qschema}.inherited AS {qschema}.dp; SET default_toast_compression='lz4'; CREATE DOMAIN {qschema}.dl AS text DEFAULT {}; CREATE DOMAIN {qschema}.inline_l AS text DEFAULT {}; CREATE DOMAIN {qschema}.unselected_type AS text DEFAULT {}",literal(&repeated),literal(&repeated),literal(&"z".repeat(3000)),literal(&entropy))).await.unwrap();
        sql(&observer,&format!("CREATE TABLE {qschema}.t(id integer,nullable text DEFAULT NULL,inline_p text,ext_p text,ext_q text,ext_l text,dp {qschema}.dp,dl {qschema}.dl,il {qschema}.inline_l,raw {qschema}.raw,empty {qschema}.empty,absent {qschema}.absent,inherited {qschema}.inherited,calc integer GENERATED ALWAYS AS(id+1) STORED); CREATE TABLE {qschema}.other(id integer,dp {qschema}.dp); INSERT INTO {qschema}.other VALUES(1,NULL); ALTER TABLE {qschema}.other ADD COLUMN missing integer DEFAULT 7; SET default_toast_compression='pglz'; ALTER TABLE {qschema}.t ALTER COLUMN inline_p SET DEFAULT {}; ALTER TABLE {qschema}.t ALTER COLUMN ext_p SET DEFAULT {}; ALTER TABLE {qschema}.t ALTER COLUMN ext_q SET DEFAULT {}; SET default_toast_compression='lz4'; ALTER TABLE {qschema}.t ALTER COLUMN ext_l SET DEFAULT {}; CREATE TABLE {qschema}.unselected(body text DEFAULT {}); ALTER TABLE {qschema}.t ALTER COLUMN ext_p SET COMPRESSION lz4,ALTER COLUMN ext_l SET COMPRESSION pglz",literal(&"abc".repeat(1000)),literal(&repeated),literal(&repeated),literal(&repeated),literal(&entropy))).await.unwrap();
        create_missing_oracles(&observer,schema).await;
        let root = oid(&observer,schema,"t").await;
        let other = oid(&observer,schema,"other").await;
        let roots = json!([[schema,"t",3],[schema,"other",1],[schema,"t",1],[schema,"t",2]]);
        let direct_oids = [root,other];
        let mut previous_missing = None;
        let mut inherited_source = None;
        for phase in 0..3 {
            if phase==1 {
                sql(&observer,&format!("ALTER TABLE {qschema}.t ALTER COLUMN nullable DROP DEFAULT,ALTER COLUMN ext_q DROP DEFAULT,ALTER COLUMN ext_p SET DEFAULT 'replacement'; ALTER DOMAIN {qschema}.dp SET DEFAULT 'new domain'; ALTER TABLE {qschema}.other ALTER COLUMN missing DROP DEFAULT")).await.unwrap();
            } else if phase==2 {
                sql(&observer,&format!("ALTER TABLE {qschema}.t RENAME COLUMN inline_p TO renamed; ALTER TABLE {qschema}.t DROP COLUMN ext_l; ALTER DOMAIN {qschema}.dl DROP DEFAULT")).await.unwrap();
            }
            let columns = column_oracle(&observer,&direct_oids).await;
            let payloads = payload_oracle(&observer,schema,&columns.selection).await;
            let inherited_oid = &columns.types.iter().find(|fact|fact["schema"]==schema && fact["name"]=="inherited").unwrap()["oid"];
            let inherited_records = payloads.iter().filter(|fact|fact["catalog_oid"]==1247 && &fact["row_oid"]==inherited_oid).cloned().collect::<Vec<_>>();
            assert_eq!(inherited_records.as_slice(),inherited_source.get_or_insert_with(||inherited_records.clone()).as_slice(),"a parent default change replaced the child's directly stored source");
            let missing = missing_oracle(&observer,schema,&columns.selection).await;
            assert_eq!(missing.len(),1);
            if let Some(previous) = previous_missing.replace(missing.clone()) { assert_eq!(missing,previous); }
            if phase==0 {
                let column_value = |name: &str| {
                    let number = &columns.attributes.iter().find(|fact|fact["relation_oid"]==root && fact["name"]==name).unwrap()["number"];
                    &payloads.iter().find(|fact|fact["relation_oid"]==root && &fact["number"]==number).unwrap()["value"]
                };
                assert_eq!(column_value("ext_p")["carrier_kind"],"e");
                assert_eq!(column_value("ext_p")["compression_kind"],u64::from(b'p'));
                assert_eq!(column_value("ext_l")["carrier_kind"],"e");
                assert_eq!(column_value("ext_l")["compression_kind"],u64::from(b'l'));
                assert_eq!(column_value("inline_p")["carrier_kind"],"p");
                assert_eq!(column_value("ext_p")["image"],column_value("ext_q")["image"]);
                let type_values = |name: &str| {
                    let type_oid = &columns.types.iter().find(|fact|fact["schema"]==schema && fact["name"]==name).unwrap()["oid"];
                    payloads.iter().filter(|fact|fact["catalog_oid"]==1247 && &fact["row_oid"]==type_oid).map(|fact|&fact["value"]).collect::<Vec<_>>()
                };
                let raw = type_values("raw");
                assert_eq!(raw.len(),2);
                assert_eq!(raw[0]["present"],false);
                assert_eq!(raw[1]["carrier_kind"],"e");
                assert_eq!(raw[1]["compression_kind"],0);
                let empty = type_values("empty");
                assert_eq!(empty[0]["present"],false);
                assert_eq!(empty[1]["present"],true);
                assert_eq!(empty[1]["carrier_kind"],"s");
                assert_eq!(empty[1]["image_bytes"],4);
                let absent = type_values("absent");
                assert_eq!(absent.len(),2);
                assert!(absent.iter().all(|value|value["present"]==false));
                assert!(type_values("inherited").iter().all(|value|value["present"]==true));
                for name in ["dp","dl"] {
                    assert!(type_values(name).iter().all(|value|value["present"]==true && value["carrier_kind"]=="e"));
                }
                // PGLZ cannot reuse this distant text repetition. The binary
                // node representation still compresses; each field's actual
                // carrier must determine its decoder independently.
                assert_eq!(type_values("dp")[0]["compression_kind"],u64::from(b'p'));
                assert_eq!(type_values("dp")[1]["compression_kind"],0);
                assert!(type_values("dl").iter().all(|value|value["compression_kind"]==u64::from(b'l')));
                assert!(type_values("inline_l").iter().all(|value|value["carrier_kind"]=="l"));
            }
            let mut graph = oracle(&observer,root).await;
            graph.extend(oracle(&observer,other).await);
            let masks = BTreeMap::from([(root,14),(other,2)]);
            for established in [false,true] {
                sql(&reader,if established {"BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT 1"} else {"BEGIN ISOLATION LEVEL REPEATABLE READ"}).await.unwrap();
                for pass in 0..2 {
                    let start = std::time::Instant::now();
                    let state = capture(&reader,&roots,false).await;
                    let elapsed = start.elapsed();
                    check_scope(&state,established,false);
                    check_columns(&state,&columns);
                    check_missing(&state,&missing);
                    check_payloads(&state,&payloads);
                    check_graph_modes(&observer,&state,&graph,&masks).await;
                    assert_eq!(state["metadata"]["root_facts"][0],state["metadata"]["root_facts"][2]);
                    assert!(state["metadata"]["toast_rows"].as_u64().unwrap()>state["metadata"]["selected_chunks"].as_u64().unwrap());
                    assert!(probe.modes(&observer,Some(backend),&fact_oids(&state)).await.is_empty());
                    probe.no_coordination(&observer,backend).await;
                    println!("native_payload_cost_sample {}",json!({"server_version":version,"phase":phase,"established_rr":established,"pass":pass,"sql_and_show_elapsed_ms":elapsed.as_secs_f64()*1000.0,"initial":state["metadata"]["initial_cost"],"source":state["metadata"]["source_cost"],"final":state["metadata"]["final_cost"],"toast_heaps":state["metadata"]["toast_heaps"],"toast_rows":state["metadata"]["toast_rows"],"selected_chunks":state["metadata"]["selected_chunks"],"payload_image_bytes":state["metadata"]["payload_image_bytes"],"references":state["metadata"]["references"].as_array().unwrap().len(),"attempts":state["metadata"]["attempts"]}));
                }
                sql(&reader,"ROLLBACK").await.unwrap();
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
        let before_root = before_drop.attributes.iter().filter(|fact|fact["relation_oid"]==root).collect::<Vec<_>>();
        let removed_type = u32::try_from(before_root[1]["type_oid"].as_u64().unwrap()).unwrap();
        sql(&observer,&format!("ALTER TABLE {qschema}.t DROP COLUMN removed; DROP TYPE {qschema}.gone; ALTER TABLE {qschema}.t RENAME COLUMN nullable TO {}", quoted(renamed_column))).await.unwrap();
        let removed: i64 = observer.query_one("SELECT count(*) FROM pg_catalog.pg_type WHERE oid=$1",&[&removed_type]).await.unwrap().get(0);
        assert_eq!(removed,0);
        let expected = column_oracle(&observer,&[root,empty]).await;
        let root_attributes = expected.attributes.iter().filter(|fact|fact["relation_oid"]==root).collect::<Vec<_>>();
        assert_eq!(root_attributes.len(),14);
        assert_eq!(expected.attributes.len(),15);
        let dropped = &root_attributes[1];
        assert_eq!(dropped["number"],2);
        assert_eq!(dropped["dropped"],true);
        assert_eq!(dropped["type_oid"],0);
        for field in ["length","by_value","alignment"] {
            assert_eq!(dropped[field],before_root[1][field]);
        }
        assert_eq!(root_attributes[0]["name"],long_column);
        assert_eq!(root_attributes[9]["name"],renamed_column);
        let text = &root_attributes[2];
        let text_type = expected.types.iter().find(|fact| fact["oid"]==text["type_oid"]).unwrap();
        assert_eq!(text["storage"],u64::from(b'e'));
        assert_eq!(text_type["storage"],u64::from(b'x'));
        assert_eq!(text["compression"],u64::from(b'p'));
        assert_ne!(text["collation_oid"],text_type["collation_oid"]);
        assert_eq!(root_attributes[3]["typmod"],11);
        assert_eq!(root_attributes[5]["dimensions"],2);
        let fake = expected.types.iter().find(|fact| fact["name"]=="int4" && fact["schema"]==schema).unwrap();
        assert_ne!(fake["oid"],23);
        assert_eq!(fake["kind"],u64::from(b'e'));
        assert!(expected.types.iter().any(|fact| fact["kind"]==u64::from(b'd') && fact["base_type_oid"]==1043));
        assert!(expected.types.iter().any(|fact| fact["kind"]==u64::from(b'c') && fact["relation_oid"]!=0));
        assert!(expected.types.iter().any(|fact| fact["element_oid"]==23));
        assert!(expected.types.iter().all(|fact| fact["oid"]!=removed_type));
        assert_eq!(root_attributes[11]["identity"],u64::from(b'a'));
        assert_eq!(root_attributes[11]["not_null_declared"],true);
        assert_eq!(root_attributes[12]["generated"],u64::from(b's'));
        assert_eq!(root_attributes[13]["has_default"],true);
        assert_eq!(root_attributes[13]["has_missing"],true);
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
        let empty_types = empty_only["metadata"]["types"].as_array().unwrap();
        assert_eq!(empty_types.len(),2);
        let rowtype = &empty_only["metadata"]["root_facts"][0]["row_type_oid"];
        let row = empty_types.iter().find(|fact| &fact["oid"]==rowtype).unwrap();
        assert_eq!(row["kind"],u64::from(b'c'));
        assert_eq!(row["relation_oid"],empty);
        let companion = empty_types.iter().find(|fact|fact["oid"]==row["array_oid"]).unwrap();
        assert_eq!(&companion["element_oid"],rowtype);
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
            let images = missing_oracle(&observer,schema,&columns.selection).await;
            let payloads = payload_oracle(&observer,schema,&columns.selection).await;
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
                    check_payloads(&state,&payloads);
                    check_graph_modes(&observer,&state,&graph,&masks).await;
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
        sql(&observer,"CREATE SCHEMA attribute_facts_wait; CREATE TYPE attribute_facts_wait.e AS ENUM ('value'); CREATE DOMAIN attribute_facts_wait.d AS text DEFAULT 'initial'; CREATE TABLE attribute_facts_wait.t(old_name attribute_facts_wait.e,n integer,dm attribute_facts_wait.d); INSERT INTO attribute_facts_wait.t(old_name,n) VALUES('value',1); ALTER TABLE attribute_facts_wait.t ADD COLUMN stable integer DEFAULT 5").await.unwrap();
        create_missing_oracles(&observer,"attribute_facts_wait").await;
        let root = oid(&observer,"attribute_facts_wait","t").await;
        let type_oid: u32 = observer.query_one("SELECT atttypid FROM pg_catalog.pg_attribute WHERE attrelid=$1 AND attnum=1",&[&root]).await.unwrap().get(0);
        let roots = json!([["attribute_facts_wait","t",1],["attribute_facts_wait","t",3]]);
        let mut cycle = 0;
        for established in [false,true] {
            for (gid,ending) in gids.iter().zip(["COMMIT","ROLLBACK"]) {
                let before = column_oracle(&observer,&[root]).await;
                let before_images = missing_oracle(&observer,"attribute_facts_wait",&before.selection).await;
                let before_payloads = payload_oracle(&observer,"attribute_facts_wait",&before.selection).await;
                let current_column = before.attributes[0]["name"].as_str().unwrap();
                let current_type = before.types.iter().find(|fact|fact["oid"]==type_oid).unwrap()["name"].as_str().unwrap();
                let next_column = format!("renamed_{cycle}");
                let next_type = format!("type_{cycle}");
                let next_added = format!("added_{cycle}");
                let next_value = format!("v_{cycle}");
                cycle += 1;
                sql(&reader,if established { "BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT 1" } else { "BEGIN" }).await.unwrap();
                sql(&writer,&format!("BEGIN; ALTER TABLE attribute_facts_wait.t RENAME COLUMN {} TO {}; ALTER TABLE attribute_facts_wait.t ADD COLUMN {} varchar(11) DEFAULT '{next_value}'; ALTER TYPE attribute_facts_wait.{} RENAME TO {}; ALTER DOMAIN attribute_facts_wait.d SET DEFAULT '{next_value}'; ALTER TABLE attribute_facts_wait.t ALTER COLUMN stable SET DEFAULT {cycle}; PREPARE TRANSACTION '{gid}'",quoted(current_column),quoted(&next_column),quoted(&next_added),quoted(current_type),quoted(&next_type))).await.unwrap();
                let request = storage_command(&roots,false);
                let waiting = command(&reader,&request);
                tokio::pin!(waiting);
                tokio::select! {
                    result = &mut waiting => panic!("prepared attribute acquisition completed early: {result:?}"),
                    () = probe.wait_physical(&observer,backend,root) => {},
                }
                probe.check_prepared_storage_wait(&observer,backend,established).await;
                sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
                waiting.await.unwrap();
                let expected = column_oracle(&observer,&[root]).await;
                let images = missing_oracle(&observer,"attribute_facts_wait",&expected.selection).await;
                let payloads = payload_oracle(&observer,"attribute_facts_wait",&expected.selection).await;
                let state = status(&reader).await;
                check_scope(&state,established,false);
                check_columns(&state,&expected);
                check_missing(&state,&images);
                check_payloads(&state,&payloads);
                check_graph(&observer,&state,&oracle(&observer,root).await,10).await;
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
                    assert_eq!(payloads,before_payloads);
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
        create_payload_oracles(&observer,"attribute_facts_null").await;
        let expected = column_oracle(&observer,&[root]).await;
        let payloads = payload_oracle(&observer,"attribute_facts_null",&expected.selection).await;
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
        check_payloads(&state,&payloads);
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
            check_graph(&observer,&state,&expected,14).await;
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
            check_graph(&observer,&again,&expected,14).await;
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
        let images = missing_oracle(&observer,"heap_storage_retain",&columns.selection).await;
        assert_eq!(images.len(),1);
        let database = expected[0]["file_database_oid"].as_u64().unwrap() as u32;
        let roots = json!([["heap_storage_retain","t",1],["heap_storage_retain","t",3]]);
        sql(&reader,"BEGIN").await.unwrap();
        command(&reader,&format!("relation_borrow:{root}/1")).await.unwrap();
        sql(&reader,"SAVEPOINT child").await.unwrap();
        let state = capture(&reader,&roots,true).await;
        check_scope(&state,false,true);
        check_graph(&observer,&state,&expected,10).await;
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
            let roots = if name=="pg_class" { json!([["pg_catalog",name,3],["pg_catalog",name,1]]) } else { json!([["pg_catalog",name,1]]) };
            let state = capture(&reader,&roots,true).await;
            check_scope(&state,false,true);
            check_graph(&observer,&state,&expected,if name=="pg_class" {10} else {2}).await;
            check_columns(&state, &columns);
            let oids = fact_oids(&state);
            assert_eq!(probe.modes(&observer,Some(backend),&oids).await,expected_locks(&state));
            if name == "pg_database" {
                assert!(state["metadata"]["facts"].as_array().unwrap().iter().filter(|fact|application_fact_oids(&state).contains(&(fact["oid"].as_u64().unwrap() as u32))).all(|fact| fact["shared"] == true && fact["file_database_oid"] == 0 && fact["file_tablespace_oid"] == 1664));
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
        check_graph(&observer,&state,&expected,8).await;
        check_columns(&state, &columns);
        let oids = fact_oids(&state);
        let proc_number = state["metadata"]["facts"].as_array().unwrap().iter().find(|fact|fact["oid"]==root).unwrap()["file_proc_number"].as_i64().unwrap();
        assert!(proc_number >= 0);
        assert!(state["metadata"]["facts"].as_array().unwrap().iter().filter(|fact|application_fact_oids(&state).contains(&(fact["oid"].as_u64().unwrap() as u32))).all(|fact| fact["file_proc_number"] == proc_number && fact["persistence"] == u64::from(b't')));
        assert_eq!(probe.modes(&observer,Some(backend),&oids).await,expected_locks(&state));
        sql(&reader,"SAVEPOINT foreign_temp").await.unwrap();
        let error = command(&reader,&storage_command(&json!([[foreign_schema,"heap_storage_foreign",1]]),false)).await.unwrap_err();
        assert_eq!(error.code(),Some(&SqlState::FEATURE_NOT_SUPPORTED));
        sql(&reader,"ROLLBACK TO foreign_temp; RELEASE foreign_temp").await.unwrap();
        let again = capture(&reader,&json!([[temp_schema,"heap_storage_temp",1]]),false).await;
        check_scope(&again,false,false);
        check_graph(&observer,&again,&expected,2).await;
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
            check_graph(&observer,&child_state,&child_expected,2).await;
            check_columns(&child_state,&child_columns);
            assert_eq!(child_state["metadata"]["roots"],json!([[child,1]]));
            assert_eq!(application_fact_oids(&child_state),vec![child]);
            assert!(probe.modes(&observer,Some(backend),&all).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"COMMIT").await.unwrap();
        }
        sql(&reader,"BEGIN").await.unwrap();
        let parent_state = capture(&reader,&json!([["heap_storage_limits","parent",1]]),false).await;
        check_scope(&parent_state,false,false);
        check_graph(&observer,&parent_state,&parent_expected,2).await;
        check_columns(&parent_state, &parent_columns);
        assert_eq!(application_fact_oids(&parent_state),vec![parent]);
        assert_eq!(parent_state["metadata"]["facts"].as_array().unwrap().iter().find(|fact|fact["oid"]==parent).unwrap()["has_subclasses"],true);
        for name in ["v","m","s","p","leaf","hash_table"] {
            sql(&reader,"SAVEPOINT boundary").await.unwrap();
            let error = command(&reader,&storage_command(&json!([["heap_storage_limits",name,1]]),false)).await.unwrap_err();
            assert_eq!(error.code(),Some(&SqlState::FEATURE_NOT_SUPPORTED),"{name}: {error}");
            sql(&reader,"ROLLBACK TO boundary; RELEASE boundary").await.unwrap();
            let fresh = capture(&reader,&json!([["heap_storage_limits","good",1]]),false).await;
            check_scope(&fresh,false,false);
            check_graph(&observer,&fresh,&expected,2).await;
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
                probe.check_prepared_storage_wait(&observer,backend,established).await;
                sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
                waiting.await.unwrap();
                let current = oid(&observer,"heap_storage_wait","t").await;
                if ending == "COMMIT" { assert_ne!(current,old); } else { assert_eq!(current,old); }
                let expected = oracle(&observer,current).await;
                let columns = column_oracle(&observer, &[current]).await;
                let state = status(&reader).await;
                check_scope(&state,established,false);
                check_graph(&observer,&state,&expected,2).await;
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
            check_graph(&observer,&state,&expected,2).await;
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

#[tokio::test]
async fn graph_options_keep_exact_images_across_changes_and_reset() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "storage_reloptions";
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,"CREATE SCHEMA storage_reloptions; CREATE TABLE storage_reloptions.t(id integer,body text) WITH(fillfactor=70,toast.autovacuum_enabled=false); CREATE INDEX t_id ON storage_reloptions.t(id) WITH(fillfactor=80,deduplicate_items=off); CREATE TABLE storage_reloptions.absent(id integer); CREATE TABLE storage_reloptions.unrelated(id integer) WITH(fillfactor=55); INSERT INTO storage_reloptions.t VALUES(1,'value')").await.unwrap();
        create_payload_oracles(&observer,schema).await;
        let root = oid(&observer,schema,"t").await;
        let absent = oid(&observer,schema,"absent").await;
        let unrelated = oid(&observer,schema,"unrelated").await;
        let index = oid(&observer,schema,"t_id").await;
        let toast: u32 = observer.query_one("SELECT reltoastrelid FROM pg_catalog.pg_class WHERE oid=$1",&[&root]).await.unwrap().get(0);
        assert_ne!(toast,0);
        let roots = json!([[schema,"t",1],[schema,"t",3],[schema,"absent",1]]);
        for established in [false,true] {
            sql(&observer,"ALTER TABLE storage_reloptions.t SET(fillfactor=70,toast.autovacuum_enabled=false); ALTER INDEX storage_reloptions.t_id SET(fillfactor=80,deduplicate_items=off)").await.unwrap();
            sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
            if established { sql(&reader,"SELECT body FROM storage_reloptions.t").await.unwrap(); }
            let initial_columns = column_oracle(&observer,&[root,absent]).await;
            let initial_expected = payload_oracle(&observer,schema,&initial_columns.selection).await;
            let selected = initial_expected.iter().filter(|fact|fact["catalog_oid"]==1259).map(|fact|u32::try_from(fact["row_oid"].as_u64().unwrap()).unwrap()).collect::<Vec<_>>();
            let prior_modes = probe.modes(&observer,Some(backend),&selected).await;
            let initial = capture(&reader,&roots,false).await;
            check_scope(&initial,established,false);
            check_payloads(&initial,&initial_expected);
            check_columns(&initial,&initial_columns);
            let options = initial["metadata"]["payloads"].as_array().unwrap().iter().filter(|fact|fact["catalog_oid"]==1259).collect::<Vec<_>>();
            assert_eq!(options.len(),fact_oids(&initial).len());
            assert!(!options.iter().any(|fact|fact["row_oid"]==unrelated));
            for node in [root,index,toast] { assert_eq!(options_payload(&initial_expected,node)["value"]["present"],true); }
            assert_eq!(options_payload(&initial_expected,absent)["value"]["present"],false);
            sql(&observer,"ALTER TABLE storage_reloptions.t SET(fillfactor=75,toast.autovacuum_enabled=true); ALTER INDEX storage_reloptions.t_id SET(fillfactor=85,deduplicate_items=on)").await.unwrap();
            let changed_selection = oracle_selection(&observer,&[root,absent]).await;
            let changed_expected = payload_oracle(&observer,schema,&changed_selection).await;
            let changed = capture(&reader,&roots,false).await;
            check_scope(&changed,established,false);
            check_payloads(&changed,&changed_expected);
            for node in [root,index,toast] { assert_ne!(options_payload(&initial_expected,node)["value"]["image"],options_payload(&changed_expected,node)["value"]["image"]); }
            assert_eq!(initial["metadata"]["payloads"],json!(initial_expected));
            sql(&observer,"ALTER TABLE storage_reloptions.t RESET(fillfactor,toast.autovacuum_enabled); ALTER INDEX storage_reloptions.t_id RESET(fillfactor,deduplicate_items)").await.unwrap();
            let reset_selection = oracle_selection(&observer,&[root,absent]).await;
            let reset_expected = payload_oracle(&observer,schema,&reset_selection).await;
            let reset = capture(&reader,&roots,false).await;
            check_scope(&reset,established,false);
            check_payloads(&reset,&reset_expected);
            for node in [root,index,toast,absent] { assert_eq!(options_payload(&reset_expected,node)["value"]["present"],false); }
            assert_eq!(changed["metadata"]["payloads"],json!(changed_expected));
            assert_eq!(probe.modes(&observer,Some(backend),&fact_oids(&reset)).await,prior_modes);
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"COMMIT").await.unwrap();
        }
    })).catch_unwind().await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        "DROP SCHEMA IF EXISTS storage_reloptions CASCADE",
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
async fn prepared_option_changes_preserve_read_views_through_both_outcomes() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (writer, wd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "storage_options_prepared";
    let gids = [
        "storage_options_first_commit",
        "storage_options_first_abort",
        "storage_options_rr_commit",
        "storage_options_rr_abort",
    ];
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,"CREATE SCHEMA storage_options_prepared; CREATE TABLE storage_options_prepared.t(id integer,body text) WITH(fillfactor=70,toast.autovacuum_enabled=false); CREATE INDEX t_id ON storage_options_prepared.t(id) WITH(fillfactor=80); INSERT INTO storage_options_prepared.t VALUES(1,'value')").await.unwrap();
        create_payload_oracles(&observer,schema).await;
        let root = oid(&observer,schema,"t").await;
        let roots = json!([[schema,"t",1],[schema,"t",1]]);
        let index = oid(&observer,schema,"t_id").await;
        for (case,gid) in gids.iter().enumerate() {
            let established = case>=2;
            let ending = if case%2==0 { "COMMIT" } else { "ROLLBACK" };
            sql(&observer,"ALTER TABLE storage_options_prepared.t SET(fillfactor=70,toast.autovacuum_enabled=false); ALTER INDEX storage_options_prepared.t_id SET(fillfactor=80)").await.unwrap();
            let before_selection = oracle_selection(&observer,&[root]).await;
            let before = payload_oracle(&observer,schema,&before_selection).await;
            sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
            if established { sql(&reader,"SELECT body FROM storage_options_prepared.t").await.unwrap(); }
            let selected = before.iter().filter(|fact|fact["catalog_oid"]==1259).map(|fact|u32::try_from(fact["row_oid"].as_u64().unwrap()).unwrap()).collect::<Vec<_>>();
            let prior_modes = probe.modes(&observer,Some(backend),&selected).await;
            let initial = capture(&reader,&roots,false).await;
            check_scope(&initial,established,false);
            check_payloads(&initial,&before);
            assert_eq!(probe.modes(&observer,Some(backend),&fact_oids(&initial)).await,prior_modes);
            probe.no_coordination(&observer,backend).await;
            sql(&writer,&format!("BEGIN; ALTER TABLE storage_options_prepared.t SET(fillfactor=75,toast.autovacuum_enabled=true); ALTER INDEX storage_options_prepared.t_id SET(fillfactor=85); PREPARE TRANSACTION '{gid}'")).await.unwrap();
            let targets = vec![root,index];
            let prepared_modes = observer.query("SELECT relation FROM pg_catalog.pg_locks WHERE locktype='relation' AND pid IS NULL AND granted AND mode='ShareUpdateExclusiveLock' AND relation=ANY($1) AND database=(SELECT oid FROM pg_catalog.pg_database WHERE datname=current_database()) ORDER BY relation",&[&targets]).await.unwrap().into_iter().map(|row|row.get::<_,u32>(0)).collect::<std::collections::BTreeSet<_>>();
            assert_eq!(prepared_modes,targets.into_iter().collect::<std::collections::BTreeSet<_>>());
            // Physical SUE coexists with AS, but prepared metadata retains
            // semantic RX. Capture must wait for S outside raw/publication.
            let semantic_modes = observer.query("SELECT mode FROM pg_catalog.pg_locks WHERE locktype='object' AND COALESCE(database,0)=0 AND classid=3079 AND objid=0 AND objsubid=17487 AND pid IS NULL AND granted ORDER BY mode",&[]).await.unwrap().into_iter().map(|row|row.get::<_,String>(0)).collect::<Vec<_>>();
            assert_eq!(semantic_modes,vec!["AccessShareLock","RowExclusiveLock"]);
            let request = storage_command(&roots,false);
            let waiting = command(&reader,&request);
            tokio::pin!(waiting);
            tokio::select! {
                result = &mut waiting => panic!("prepared options capture completed before native completion: {result:?}"),
                () = probe.wait_semantic(&observer,backend) => {},
            }
            probe.check_data_horizon(&observer,backend,established).await;
            sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
            waiting.await.unwrap();
            let after_selection = oracle_selection(&observer,&[root]).await;
            let after = payload_oracle(&observer,schema,&after_selection).await;
            let completed = status(&reader).await;
            check_scope(&completed,established,false);
            check_payloads(&completed,&after);
            if ending=="COMMIT" { assert_ne!(options_payload(&before,root)["value"]["image"],options_payload(&after,root)["value"]["image"]); }
            else { assert_eq!(after,before); }
            assert_eq!(initial["metadata"]["payloads"],json!(before));
            assert_eq!(probe.modes(&observer,Some(backend),&fact_oids(&completed)).await,prior_modes);
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"COMMIT").await.unwrap();
        }
    })).catch_unwind().await;
    finish_targets(&observer, &gids).await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&writer, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        "DROP SCHEMA IF EXISTS storage_options_prepared CASCADE",
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(writer, wd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
#[ignore = "requires the native PostgreSQL primary profile"]
async fn descriptor_branch_hints_and_check_counts_match_current_catalog_declarations() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let schema = "storage_descriptor_fields";
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,"CREATE SCHEMA storage_descriptor_fields; CREATE TABLE storage_descriptor_fields.sentinel(id integer); INSERT INTO storage_descriptor_fields.sentinel VALUES(1); CREATE TABLE storage_descriptor_fields.absent(id integer); CREATE TABLE storage_descriptor_fields.unrelated(id integer CHECK(id>0)); CREATE FUNCTION storage_descriptor_fields.pass() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'; CREATE RULE unrelated_update AS ON UPDATE TO storage_descriptor_fields.unrelated DO INSTEAD NOTHING; CREATE TRIGGER unrelated_insert BEFORE INSERT ON storage_descriptor_fields.unrelated FOR EACH ROW EXECUTE FUNCTION storage_descriptor_fields.pass()").await.unwrap();
        let absent = oid(&observer,schema,"absent").await;
        let unrelated = oid(&observer,schema,"unrelated").await;
        for established in [false,true] {
            let name = if established { "established" } else { "first" };
            let table = format!("{}.{}",quoted(schema),quoted(name));
            sql(&observer,&format!("CREATE TABLE {table}(id integer,body text); INSERT INTO {table} VALUES(1,'value')")).await.unwrap();
            let root = oid(&observer,schema,name).await;
            let roots = json!([[schema,name,1],[schema,name,1],[schema,"absent",1]]);
            let masks = BTreeMap::from([(root,2),(absent,2)]);
            sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
            if established { sql(&reader,"SELECT id FROM storage_descriptor_fields.sentinel").await.unwrap(); }
            let mut before = oracle(&observer,root).await;
            before.extend(oracle(&observer,absent).await);
            let initial = capture(&reader,&roots,false).await;
            check_scope(&initial,established,false);
            check_graph_modes(&observer,&initial,&before,&masks).await;
            assert_eq!(storage_fact(&initial,root)["declared_check_count"],0);
            assert_eq!(storage_fact(&initial,root)["rules_hint"],false);
            assert_eq!(storage_fact(&initial,root)["triggers_hint"],false);
            assert!(!application_fact_oids(&initial).contains(&unrelated));
            sql(&observer,&format!("CREATE RULE suppress_update AS ON UPDATE TO {table} DO INSTEAD NOTHING; CREATE TRIGGER pass_insert BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION storage_descriptor_fields.pass(); ALTER TABLE {table} ADD CONSTRAINT positive CHECK(id>0) NOT VALID; ALTER TABLE {table} ADD CONSTRAINT body_present CHECK(body IS NOT NULL)")).await.unwrap();
            let mut expected = oracle(&observer,root).await;
            expected.extend(oracle(&observer,absent).await);
            let declared = capture(&reader,&roots,false).await;
            check_scope(&declared,established,false);
            check_graph_modes(&observer,&declared,&expected,&masks).await;
            assert_eq!(storage_fact(&declared,root)["declared_check_count"],2);
            assert_eq!(storage_fact(&declared,root)["rules_hint"],true);
            assert_eq!(storage_fact(&declared,root)["triggers_hint"],true);
            assert_eq!(storage_fact(&declared,absent)["declared_check_count"],0);
            assert_eq!(storage_fact(&declared,absent)["rules_hint"],false);
            assert_eq!(storage_fact(&declared,absent)["triggers_hint"],false);
            sql(&observer,&format!("ALTER TABLE {table} VALIDATE CONSTRAINT positive; DROP RULE suppress_update ON {table}; DROP TRIGGER pass_insert ON {table}; ALTER TABLE {table} DROP CONSTRAINT body_present")).await.unwrap();
            let current = capture(&reader,&roots,false).await;
            check_scope(&current,established,false);
            let mut after_drop = oracle(&observer,root).await;
            after_drop.extend(oracle(&observer,absent).await);
            check_graph_modes(&observer,&current,&after_drop,&masks).await;
            assert_eq!(storage_fact(&current,root)["declared_check_count"],1);
            // DROP need not clear conservative hints; normal maintenance and
            // the independent oracle, rather than inferred absence, decide them.
            sql(&observer,&format!("ALTER TABLE {table} DROP CONSTRAINT positive")).await.unwrap();
            sql(&observer,&format!("VACUUM {table}")).await.unwrap();
            let maintained = capture(&reader,&roots,false).await;
            check_scope(&maintained,established,false);
            let mut after_maintenance = oracle(&observer,root).await;
            after_maintenance.extend(oracle(&observer,absent).await);
            check_graph_modes(&observer,&maintained,&after_maintenance,&masks).await;
            assert_eq!(storage_fact(&maintained,root)["declared_check_count"],0);
            assert_eq!(storage_fact(&initial,root)["declared_check_count"],0);
            assert_eq!(storage_fact(&initial,root)["rules_hint"],false);
            assert_eq!(storage_fact(&declared,root)["declared_check_count"],2);
            assert_eq!(storage_fact(&declared,root)["rules_hint"],true);
            assert_eq!(storage_fact(&declared,root)["triggers_hint"],true);
            sql(&reader,"COMMIT").await.unwrap();
        }
    })).catch_unwind().await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        "DROP SCHEMA IF EXISTS storage_descriptor_fields CASCADE",
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
#[ignore = "requires the native PostgreSQL primary profile with two-phase transactions"]
async fn prepared_descriptor_declarations_preserve_real_data_views_through_both_outcomes() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (writer, wd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "storage_descriptor_prepared";
    let gids = [
        "storage_descriptor_first_commit",
        "storage_descriptor_first_abort",
        "storage_descriptor_rr_commit",
        "storage_descriptor_rr_abort",
    ];
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,"CREATE SCHEMA storage_descriptor_prepared; CREATE TABLE storage_descriptor_prepared.sentinel(id integer); INSERT INTO storage_descriptor_prepared.sentinel VALUES(1); CREATE FUNCTION storage_descriptor_prepared.pass() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'").await.unwrap();
        for (case,gid) in gids.iter().enumerate() {
            let established = case>=2;
            let ending = if case%2==0 { "COMMIT" } else { "ROLLBACK" };
            let name = format!("target_{case}");
            let table = format!("{}.{}",quoted(schema),quoted(&name));
            sql(&observer,&format!("CREATE TABLE {table}(id integer,body text); INSERT INTO {table} VALUES(1,'value')")).await.unwrap();
            let root = oid(&observer,schema,&name).await;
            let roots = json!([[schema,name,1],[schema,name,1]]);
            let before = oracle(&observer,root).await;
            let before_data: i32 = observer.query_one("SELECT id FROM storage_descriptor_prepared.sentinel",&[]).await.unwrap().get(0);
            sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
            // A real unrelated data read establishes RR without holding target
            // AS through the writer's ordinary access-exclusive DDL.
            if established { sql(&reader,"SELECT id FROM storage_descriptor_prepared.sentinel").await.unwrap(); }
            let initial = capture(&reader,&roots,false).await;
            check_scope(&initial,established,false);
            check_graph(&observer,&initial,&before,2).await;
            sql(&writer,&format!("BEGIN; CREATE RULE suppress_update AS ON UPDATE TO {table} DO INSTEAD NOTHING; CREATE TRIGGER pass_insert BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION storage_descriptor_prepared.pass(); ALTER TABLE {table} ADD CONSTRAINT positive CHECK(id>0) NOT VALID; ALTER TABLE {table} ADD CONSTRAINT body_present CHECK(body IS NOT NULL); UPDATE storage_descriptor_prepared.sentinel SET id=id+1; PREPARE TRANSACTION '{gid}'")).await.unwrap();
            assert!(probe.modes(&observer,None,&[root]).await.iter().any(|row|row.2=="AccessExclusiveLock" && row.3));
            let request = storage_command(&roots,false);
            let waiting = command(&reader,&request);
            tokio::pin!(waiting);
            tokio::select! {
                result = &mut waiting => panic!("prepared descriptor acquisition completed before native completion: {result:?}"),
                () = probe.wait_physical(&observer,backend,root) => {},
            }
            probe.check_prepared_storage_wait(&observer,backend,established).await;
            sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
            waiting.await.unwrap();
            let expected = oracle(&observer,root).await;
            let completed = status(&reader).await;
            check_scope(&completed,established,false);
            check_graph(&observer,&completed,&expected,2).await;
            if ending=="COMMIT" {
                assert_eq!(storage_fact(&completed,root)["declared_check_count"],2);
                assert_eq!(storage_fact(&completed,root)["rules_hint"],true);
                assert_eq!(storage_fact(&completed,root)["triggers_hint"],true);
            } else { assert_eq!(expected,before); }
            assert_eq!(storage_fact(&initial,root)["declared_check_count"],0);
            assert_eq!(storage_fact(&initial,root)["rules_hint"],false);
            assert_eq!(storage_fact(&initial,root)["triggers_hint"],false);
            assert!(probe.modes(&observer,Some(backend),&fact_oids(&completed)).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            let observed_data: i32 = reader.query_one("SELECT id FROM storage_descriptor_prepared.sentinel",&[]).await.unwrap().get(0);
            let current_data: i32 = observer.query_one("SELECT id FROM storage_descriptor_prepared.sentinel",&[]).await.unwrap().get(0);
            assert_eq!(current_data,before_data+i32::from(ending=="COMMIT"));
            assert_eq!(observed_data,if established { before_data } else { current_data });
            sql(&reader,"COMMIT").await.unwrap();
        }
    })).catch_unwind().await;
    finish_targets(&observer, &gids).await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&writer, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        "DROP SCHEMA IF EXISTS storage_descriptor_prepared CASCADE",
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(writer, wd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
#[ignore = "requires the native PostgreSQL primary profile"]
async fn declared_type_links_own_ancestor_defaults_and_terminate_companion_cycles() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "storage_type_links";
    let qschema = quoted(schema);
    let large = (0..3000_u32)
        .map(|n| format!("{:08x}", n.wrapping_mul(2654435761)))
        .collect::<String>();
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,&format!("CREATE SCHEMA {qschema}; CREATE DOMAIN {qschema}.base AS integer DEFAULT 7; CREATE DOMAIN {qschema}.child AS {qschema}.base DEFAULT 11; CREATE DOMAIN {qschema}.values AS {qschema}.child[]; CREATE DOMAIN {qschema}.text_base AS text DEFAULT '{large}'; CREATE DOMAIN {qschema}.text_child AS {qschema}.text_base DEFAULT 'child'; CREATE DOMAIN {qschema}.unrelated AS text DEFAULT '{large}'; CREATE TABLE {qschema}.target(a {qschema}.child,b {qschema}.child[],c {qschema}.values,d {qschema}.text_child); CREATE TABLE {qschema}.empty(); CREATE TABLE {qschema}.sentinel(id integer); INSERT INTO {qschema}.sentinel VALUES(1)")).await.unwrap();
        create_payload_oracles(&observer,schema).await;
        let root = oid(&observer,schema,"target").await;
        let empty = oid(&observer,schema,"empty").await;
        let named: BTreeMap<String,u32> = observer.query(
            "SELECT typname::text,oid FROM pg_catalog.pg_type WHERE typnamespace=(SELECT oid FROM pg_catalog.pg_namespace WHERE nspname=$1)", &[&schema],
        ).await.unwrap().into_iter().map(|row|(row.get(0),row.get(1))).collect();
        let base = named["base"];
        let child = named["child"];
        let array_domain = named["values"];
        let text_base = named["text_base"];
        let text_child = named["text_child"];
        let unrelated = named["unrelated"];
        let roots = json!([[schema,"target",3],[schema,"empty",1],[schema,"target",1],[schema,"target",2]]);
        let masks = BTreeMap::from([(root,14),(empty,2)]);
        for established in [false,true] {
            sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
            if established { sql(&reader,&format!("SELECT id FROM {qschema}.sentinel")).await.unwrap(); }
            let columns = column_oracle(&observer,&[root,empty]).await;
            let payloads = payload_oracle(&observer,schema,&columns.selection).await;
            let mut graph = oracle(&observer,root).await;
            graph.extend(oracle(&observer,empty).await);
            let initial = capture(&reader,&roots,false).await;
            check_scope(&initial,established,false);
            check_columns(&initial,&columns);
            check_payloads(&initial,&payloads);
            check_graph_modes(&observer,&initial,&graph,&masks).await;
            assert_eq!(initial["metadata"]["root_facts"][0],initial["metadata"]["root_facts"][2]);
            assert_eq!(initial["metadata"]["root_facts"][0],initial["metadata"]["root_facts"][3]);
            assert_eq!(type_fact(&initial,child)["base_type_oid"],base);
            let companion = u32::try_from(type_fact(&initial,child)["array_oid"].as_u64().unwrap()).unwrap();
            assert_eq!(type_fact(&initial,companion)["element_oid"],child);
            assert_eq!(type_fact(&initial,array_domain)["base_type_oid"],companion);
            assert_eq!(type_fact(&initial,text_child)["base_type_oid"],text_base);
            assert!(!initial["metadata"]["types"].as_array().unwrap().iter().any(|fact|fact["oid"]==unrelated));
            assert!(type_payloads(&initial,unrelated).is_empty());
            for type_oid in [base,child,text_base,text_child] {
                let defaults = type_payloads(&initial,type_oid);
                assert_eq!(defaults.len(),2);
                assert!(defaults.iter().all(|fact|fact["value"]["present"]==true));
            }
            assert_ne!(type_payloads(&initial,base)[0]["value"]["image"],type_payloads(&initial,child)[0]["value"]["image"]);
            assert_ne!(type_payloads(&initial,text_base)[0]["value"]["image"],type_payloads(&initial,text_child)[0]["value"]["image"]);
            let before = type_payloads(&initial,base);
            let value = if established { 23 } else { 19 };
            sql(&observer,&format!("ALTER DOMAIN {qschema}.base SET DEFAULT {value}; ALTER DOMAIN {qschema}.unrelated SET DEFAULT '{large}{large}'")).await.unwrap();
            let current_columns = column_oracle(&observer,&[root,empty]).await;
            let current_payloads = payload_oracle(&observer,schema,&current_columns.selection).await;
            let current = capture(&reader,&roots,false).await;
            check_scope(&current,established,false);
            check_columns(&current,&current_columns);
            check_payloads(&current,&current_payloads);
            check_graph_modes(&observer,&current,&graph,&masks).await;
            assert_ne!(type_payloads(&current,base),before);
            assert_eq!(type_payloads(&initial,base),before);
            assert_eq!(type_payloads(&current,child),type_payloads(&initial,child));
            assert!(probe.modes(&observer,Some(backend),&fact_oids(&current)).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"COMMIT").await.unwrap();
        }
        // Own TEMP rowtype and its companion use the same actual namespace.
        // Compare after the first capture so the SQL oracle does not establish
        // the reader's data snapshot before neutral preparation.
        sql(&reader,&format!("CREATE TEMP TABLE own_type_links(v {qschema}.child[])")).await.unwrap();
        let temp_root: u32 = reader.query_one("SELECT 'pg_temp.own_type_links'::regclass::oid",&[]).await.unwrap().get(0);
        let temp_schema: String = reader.query_one("SELECT n.nspname::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE c.oid=$1",&[&temp_root]).await.unwrap().get(0);
        sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
        let temp = capture(&reader,&json!([[temp_schema,"own_type_links",1]]),true).await;
        check_scope(&temp,false,true);
        let temp_columns = column_oracle(&reader,&[temp_root]).await;
        check_columns(&temp,&temp_columns);
        check_payloads(&temp,&payload_oracle(&reader,schema,&temp_columns.selection).await);
        assert!(!probe.modes(&observer,Some(backend),&fact_oids(&temp)).await.is_empty());
        probe.no_coordination(&observer,backend).await;
        sql(&reader,"COMMIT; DROP TABLE pg_temp.own_type_links").await.unwrap();
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
#[ignore = "requires the native PostgreSQL primary profile with two-phase transactions"]
async fn prepared_type_link_changes_preserve_both_real_data_view_states() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (writer, wd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "storage_type_prepared";
    let qschema = quoted(schema);
    let gids = [
        "type_links_first_commit",
        "type_links_first_abort",
        "type_links_rr_commit",
        "type_links_rr_abort",
    ];
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,&format!("CREATE SCHEMA {qschema}; CREATE DOMAIN {qschema}.base AS integer DEFAULT 7; CREATE DOMAIN {qschema}.child AS {qschema}.base DEFAULT 11; CREATE TABLE {qschema}.sentinel(id integer); INSERT INTO {qschema}.sentinel VALUES(1)")).await.unwrap();
        create_payload_oracles(&observer,schema).await;
        let base: u32 = observer.query_one("SELECT t.oid FROM pg_catalog.pg_type t JOIN pg_catalog.pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname=$1 AND t.typname='base'",&[&schema]).await.unwrap().get(0);
        for (case,gid) in gids.iter().enumerate() {
            let established = case>=2;
            let ending = if case%2==0 { "COMMIT" } else { "ROLLBACK" };
            let name = format!("target_{case}");
            let table = format!("{qschema}.{}",quoted(&name));
            sql(&observer,&format!("CREATE TABLE {table}(v {qschema}.child)")).await.unwrap();
            let root = oid(&observer,schema,&name).await;
            let roots = json!([[schema,name,1],[schema,name,1]]);
            let before_columns = column_oracle(&observer,&[root]).await;
            let before_payloads = payload_oracle(&observer,schema,&before_columns.selection).await;
            let before_data: i32 = observer.query_one(&format!("SELECT id FROM {qschema}.sentinel"),&[]).await.unwrap().get(0);
            sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
            if established { sql(&reader,&format!("SELECT id FROM {qschema}.sentinel")).await.unwrap(); }
            let initial = capture(&reader,&roots,false).await;
            check_scope(&initial,established,false);
            check_columns(&initial,&before_columns);
            check_payloads(&initial,&before_payloads);
            let added = format!("{qschema}.{}",quoted(&format!("added_{case}")));
            sql(&writer,&format!("BEGIN; ALTER DOMAIN {qschema}.base SET DEFAULT {}; CREATE DOMAIN {added} AS {qschema}.child DEFAULT {}; ALTER TABLE {table} ADD COLUMN extra {added}[]; UPDATE {qschema}.sentinel SET id=id+1; PREPARE TRANSACTION '{gid}'",case+29,case+41)).await.unwrap();
            assert!(probe.modes(&observer,None,&[root]).await.iter().any(|row|row.2=="AccessExclusiveLock" && row.3));
            let request = storage_command(&roots,false);
            let waiting = command(&reader,&request);
            tokio::pin!(waiting);
            tokio::select! {
                result = &mut waiting => panic!("prepared type-link acquisition completed before native completion: {result:?}"),
                () = probe.wait_physical(&observer,backend,root) => {},
            }
            probe.check_prepared_storage_wait(&observer,backend,established).await;
            sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
            waiting.await.unwrap();
            let completed = status(&reader).await;
            check_scope(&completed,established,false);
            let current_columns = column_oracle(&observer,&[root]).await;
            let current_payloads = payload_oracle(&observer,schema,&current_columns.selection).await;
            check_columns(&completed,&current_columns);
            check_payloads(&completed,&current_payloads);
            check_graph(&observer,&completed,&oracle(&observer,root).await,2).await;
            if ending=="COMMIT" {
                assert!(current_columns.types.len()>before_columns.types.len());
                assert_ne!(type_payloads(&completed,base),type_payloads(&initial,base));
            } else {
                assert_eq!(current_columns.types,before_columns.types);
                assert_eq!(current_payloads,before_payloads);
            }
            assert_eq!(initial["metadata"]["types"],json!(before_columns.types));
            assert_eq!(initial["metadata"]["payloads"],json!(before_payloads));
            assert!(probe.modes(&observer,Some(backend),&fact_oids(&completed)).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            let observed_data: i32 = reader.query_one(&format!("SELECT id FROM {qschema}.sentinel"),&[]).await.unwrap().get(0);
            let current_data: i32 = observer.query_one(&format!("SELECT id FROM {qschema}.sentinel"),&[]).await.unwrap().get(0);
            assert_eq!(current_data,before_data+i32::from(ending=="COMMIT"));
            assert_eq!(observed_data,if established { before_data } else { current_data });
            sql(&reader,"COMMIT").await.unwrap();
        }
    })).catch_unwind().await;
    finish_targets(&observer, &gids).await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&writer, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        &format!("DROP SCHEMA IF EXISTS {qschema} CASCADE"),
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(writer, wd).await;
    close(observer, od).await;
    finish(outcome);
}

#[tokio::test]
#[ignore = "requires the native PostgreSQL primary profile"]
async fn composite_fields_own_nested_nonroot_declarations_and_images() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "storage_composite_fields";
    let qschema = quoted(schema);
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,&format!("CREATE SCHEMA {qschema}; CREATE DOMAIN {qschema}.base AS integer DEFAULT 7; CREATE DOMAIN {qschema}.child AS {qschema}.base DEFAULT 11; CREATE TYPE {qschema}.leaf AS (v {qschema}.child,gone text); ALTER TYPE {qschema}.leaf DROP ATTRIBUTE gone; CREATE DOMAIN {qschema}.leaves AS {qschema}.leaf[]; CREATE TABLE {qschema}.parent(id integer); CREATE TABLE {qschema}.stored(own_field {qschema}.child DEFAULT 13) INHERITS({qschema}.parent); INSERT INTO {qschema}.stored(id) VALUES(1); ALTER TABLE {qschema}.parent ADD COLUMN inherited text DEFAULT 'inherited'; ALTER TABLE {qschema}.stored ADD COLUMN missing {qschema}.child DEFAULT 19; CREATE TYPE {qschema}.middle AS (items {qschema}.leaves,shared {qschema}.leaf,stored {qschema}.stored); CREATE TYPE {qschema}.unrelated AS (unused jsonb); CREATE TABLE {qschema}.target(a {qschema}.middle,b {qschema}.leaf[],stored {qschema}.stored); CREATE TABLE {qschema}.empty(); CREATE TABLE {qschema}.sentinel(id integer); INSERT INTO {qschema}.sentinel VALUES(1)")).await.unwrap();
        create_missing_oracles(&observer,schema).await;
        let root = oid(&observer,schema,"target").await;
        let empty = oid(&observer,schema,"empty").await;
        let leaf = oid(&observer,schema,"leaf").await;
        let middle = oid(&observer,schema,"middle").await;
        let stored = oid(&observer,schema,"stored").await;
        let parent = oid(&observer,schema,"parent").await;
        let unrelated = oid(&observer,schema,"unrelated").await;
        let roots = json!([[schema,"target",3],[schema,"empty",1],[schema,"target",1],[schema,"target",2]]);
        let masks = BTreeMap::from([(root,14),(empty,2)]);
        let mut graph = oracle(&observer,root).await;
        graph.extend(oracle(&observer,empty).await);
        for established in [false,true] {
            sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
            if established { sql(&reader,&format!("SELECT id FROM {qschema}.sentinel")).await.unwrap(); }
            let columns = column_oracle(&observer,&[root,empty]).await;
            let missing = missing_oracle(&observer,schema,&columns.selection).await;
            let payloads = payload_oracle(&observer,schema,&columns.selection).await;
            let initial = capture(&reader,&roots,false).await;
            check_scope(&initial,established,false);
            check_columns(&initial,&columns);
            check_missing(&initial,&missing);
            check_payloads(&initial,&payloads);
            check_graph_modes(&observer,&initial,&graph,&masks).await;
            let actual = columns.composites.iter().map(|fact|u32::try_from(fact["relation_oid"].as_u64().unwrap()).unwrap()).collect::<std::collections::BTreeSet<_>>();
            assert_eq!(actual,std::collections::BTreeSet::from([root,empty,leaf,middle,stored]));
            assert!(!actual.contains(&parent) && !actual.contains(&unrelated));
            let leaf_fields = columns.attributes.iter().filter(|fact|fact["relation_oid"]==leaf).collect::<Vec<_>>();
            assert_eq!(leaf_fields.len(),2);
            assert_eq!(leaf_fields[1]["dropped"],true);
            assert_eq!(leaf_fields[1]["type_oid"],0);
            let stored_fields = columns.attributes.iter().filter(|fact|fact["relation_oid"]==stored).collect::<Vec<_>>();
            assert_eq!(stored_fields.len(),4);
            assert!(stored_fields.iter().any(|fact|fact["local"]==false && fact["inheritance_count"].as_i64().unwrap()>0));
            assert!(stored_fields.iter().any(|fact|fact["local"]==true && fact["has_default"]==true));
            assert!(missing.len()>=2 && missing.iter().all(|fact|fact["relation_oid"]==stored));
            assert!(payloads.iter().any(|fact|fact["catalog_oid"]==2604 && fact["relation_oid"]==stored && fact["value"]["present"]==true));
            assert!(columns.composites.iter().filter(|fact|fact["kind"]==u64::from(b'c')).all(|fact|fact["am"]==0));
            assert_eq!(initial["metadata"]["root_facts"][0],initial["metadata"]["root_facts"][2]);
            assert_eq!(initial["metadata"]["root_facts"][0],initial["metadata"]["root_facts"][3]);
            assert!(!fact_oids(&initial).iter().any(|oid|[leaf,middle,stored,parent,unrelated].contains(oid)));
            let field = if established { "latest" } else { "renamed" };
            let previous_field = if established { "renamed" } else { "v" };
            sql(&observer,&format!("ALTER TYPE {qschema}.leaf RENAME ATTRIBUTE {previous_field} TO {field}; ALTER TABLE {qschema}.stored ALTER COLUMN own_field SET DEFAULT {}; ALTER DOMAIN {qschema}.base SET DEFAULT {}",if established { 31 } else { 29 },if established { 23 } else { 17 })).await.unwrap();
            let current_columns = column_oracle(&observer,&[root,empty]).await;
            let current_missing = missing_oracle(&observer,schema,&current_columns.selection).await;
            let current_payloads = payload_oracle(&observer,schema,&current_columns.selection).await;
            let current = capture(&reader,&roots,false).await;
            check_scope(&current,established,false);
            check_columns(&current,&current_columns);
            check_missing(&current,&current_missing);
            check_payloads(&current,&current_payloads);
            check_graph_modes(&observer,&current,&graph,&masks).await;
            assert_ne!(current_columns.attributes,columns.attributes);
            assert_ne!(current_payloads,payloads);
            assert_eq!(current_missing,missing);
            assert_eq!(initial["metadata"]["composites"],json!(columns.composites));
            assert_eq!(initial["metadata"]["attributes"],json!(columns.attributes));
            assert_eq!(initial["metadata"]["payloads"],json!(payloads));
            assert!(probe.modes(&observer,Some(backend),&fact_oids(&current)).await.is_empty());
            assert!(probe.modes(&observer,Some(backend),&[leaf,middle,stored]).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            sql(&reader,"COMMIT").await.unwrap();
        }
        // A referenced own-TEMP rowtype is a declaration, while the input root
        // alone supplies the application physical graph and retained counts.
        sql(&reader,&format!("CREATE TEMP TABLE own_composite_leaf(v {qschema}.child); CREATE TEMP TABLE own_composite_root(v pg_temp.own_composite_leaf[])")).await.unwrap();
        let temp_root: u32 = reader.query_one("SELECT 'pg_temp.own_composite_root'::regclass::oid",&[]).await.unwrap().get(0);
        let temp_leaf: u32 = reader.query_one("SELECT 'pg_temp.own_composite_leaf'::regclass::oid",&[]).await.unwrap().get(0);
        let temp_schema: String = reader.query_one("SELECT n.nspname::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE c.oid=$1",&[&temp_root]).await.unwrap().get(0);
        sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
        let temp = capture(&reader,&json!([[temp_schema,"own_composite_root",1]]),true).await;
        check_scope(&temp,false,true);
        let temp_columns = column_oracle(&reader,&[temp_root]).await;
        check_columns(&temp,&temp_columns);
        check_payloads(&temp,&payload_oracle(&reader,schema,&temp_columns.selection).await);
        assert_eq!(temp_columns.composites.len(),2);
        assert!(temp_columns.composites.iter().all(|fact|fact["schema"]==temp_schema && fact["persistence"]==u64::from(b't')));
        assert!(!fact_oids(&temp).contains(&temp_leaf));
        assert!(!probe.modes(&observer,Some(backend),&fact_oids(&temp)).await.is_empty());
        probe.no_coordination(&observer,backend).await;
        sql(&reader,"COMMIT; DROP TABLE pg_temp.own_composite_root; DROP TABLE pg_temp.own_composite_leaf").await.unwrap();
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
#[ignore = "requires the native PostgreSQL primary profile with two-phase transactions"]
async fn prepared_composite_field_changes_preserve_both_real_data_view_states() {
    let _serial = SERIAL.lock().await;
    let (reader, rd) = client().await;
    let (writer, wd) = client().await;
    let (observer, od) = client().await;
    let backend = pid(&reader).await;
    let probe = Observer::new(&observer).await;
    let schema = "storage_composite_prepared";
    let qschema = quoted(schema);
    let gids = [
        "composite_first_commit",
        "composite_first_abort",
        "composite_rr_commit",
        "composite_rr_abort",
    ];
    let outcome = AssertUnwindSafe(tokio::time::timeout(CASE_DEADLINE, async {
        sql(&observer,&format!("CREATE SCHEMA {qschema}; CREATE DOMAIN {qschema}.base AS integer DEFAULT 7; CREATE DOMAIN {qschema}.child AS {qschema}.base DEFAULT 11; CREATE TABLE {qschema}.sentinel(id integer); INSERT INTO {qschema}.sentinel VALUES(1)")).await.unwrap();
        create_payload_oracles(&observer,schema).await;
        for (case,gid) in gids.iter().enumerate() {
            let established = case>=2;
            let ending = if case%2==0 { "COMMIT" } else { "ROLLBACK" };
            let name = format!("target_{case}");
            let leaf_name = format!("leaf_{case}");
            let table = format!("{qschema}.{}",quoted(&name));
            let leaf_type = format!("{qschema}.{}",quoted(&leaf_name));
            sql(&observer,&format!("CREATE TYPE {leaf_type} AS (v {qschema}.child,gone text); ALTER TYPE {leaf_type} DROP ATTRIBUTE gone; CREATE TABLE {table}(v {leaf_type}[])")).await.unwrap();
            let root = oid(&observer,schema,&name).await;
            let leaf = oid(&observer,schema,&leaf_name).await;
            let roots = json!([[schema,name,1],[schema,name,1]]);
            let before_columns = column_oracle(&observer,&[root]).await;
            let before_payloads = payload_oracle(&observer,schema,&before_columns.selection).await;
            let before_data: i32 = observer.query_one(&format!("SELECT id FROM {qschema}.sentinel"),&[]).await.unwrap().get(0);
            sql(&reader,"BEGIN ISOLATION LEVEL REPEATABLE READ").await.unwrap();
            if established { sql(&reader,&format!("SELECT id FROM {qschema}.sentinel")).await.unwrap(); }
            let initial = capture(&reader,&roots,false).await;
            check_scope(&initial,established,false);
            check_columns(&initial,&before_columns);
            check_payloads(&initial,&before_payloads);
            let added = format!("{qschema}.{}",quoted(&format!("added_{case}")));
            let field = format!("live_{case}");
            sql(&writer,&format!("BEGIN; ALTER TYPE {leaf_type} RENAME ATTRIBUTE v TO {field}; CREATE DOMAIN {added} AS {qschema}.child DEFAULT {}; ALTER TABLE {table} ADD COLUMN extra {added}[]; UPDATE {qschema}.sentinel SET id=id+1; PREPARE TRANSACTION '{gid}'",case+41)).await.unwrap();
            assert!(probe.modes(&observer,None,&[root]).await.iter().any(|row|row.2=="AccessExclusiveLock" && row.3));
            let request = storage_command(&roots,false);
            let waiting = command(&reader,&request);
            tokio::pin!(waiting);
            tokio::select! {
                result = &mut waiting => panic!("prepared composite acquisition completed before native completion: {result:?}"),
                () = probe.wait_physical(&observer,backend,root) => {},
            }
            probe.check_prepared_storage_wait(&observer,backend,established).await;
            sql(&observer,&format!("{ending} PREPARED '{gid}'")).await.unwrap();
            waiting.await.unwrap();
            let completed = status(&reader).await;
            check_scope(&completed,established,false);
            let current_columns = column_oracle(&observer,&[root]).await;
            let current_payloads = payload_oracle(&observer,schema,&current_columns.selection).await;
            check_columns(&completed,&current_columns);
            check_payloads(&completed,&current_payloads);
            check_graph(&observer,&completed,&oracle(&observer,root).await,2).await;
            if ending=="COMMIT" {
                assert_eq!(current_columns.attributes.len(),before_columns.attributes.len()+1);
                assert!(current_columns.types.len()>before_columns.types.len());
                assert!(current_columns.attributes.iter().any(|fact|fact["relation_oid"]==leaf && fact["number"]==1 && fact["name"]==field));
                assert_ne!(current_columns.composites,before_columns.composites);
            } else {
                assert_eq!(current_columns.composites,before_columns.composites);
                assert_eq!(current_columns.attributes,before_columns.attributes);
                assert_eq!(current_columns.types,before_columns.types);
                assert_eq!(current_payloads,before_payloads);
            }
            assert_eq!(initial["metadata"]["composites"],json!(before_columns.composites));
            assert_eq!(initial["metadata"]["attributes"],json!(before_columns.attributes));
            assert_eq!(initial["metadata"]["types"],json!(before_columns.types));
            assert_eq!(initial["metadata"]["payloads"],json!(before_payloads));
            assert!(!fact_oids(&completed).contains(&leaf));
            assert!(probe.modes(&observer,Some(backend),&fact_oids(&completed)).await.is_empty());
            probe.no_coordination(&observer,backend).await;
            let observed_data: i32 = reader.query_one(&format!("SELECT id FROM {qschema}.sentinel"),&[]).await.unwrap().get(0);
            let current_data: i32 = observer.query_one(&format!("SELECT id FROM {qschema}.sentinel"),&[]).await.unwrap().get(0);
            assert_eq!(current_data,before_data+i32::from(ending=="COMMIT"));
            assert_eq!(observed_data,if established { before_data } else { current_data });
            sql(&reader,"COMMIT").await.unwrap();
        }
    })).catch_unwind().await;
    finish_targets(&observer, &gids).await;
    sql(&reader, "ROLLBACK").await.unwrap();
    sql(&writer, "ROLLBACK").await.unwrap();
    sql(
        &observer,
        &format!("DROP SCHEMA IF EXISTS {qschema} CASCADE"),
    )
    .await
    .unwrap();
    close(reader, rd).await;
    close(writer, wd).await;
    close(observer, od).await;
    finish(outcome);
}
