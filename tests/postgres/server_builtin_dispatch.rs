//! Ordinary live builtin dispatch observations, not complete provider admission.
use futures_util::StreamExt;
use serde_json::Value;
use std::time::Duration;
use tokio_postgres::{Client, NoTls, SimpleQueryEvent, TransactionState};

const DEADLINE: Duration = Duration::from_secs(20);
const SHOW: &str = "SHOW darmok_catalog_probe.builtin_dispatch";

async fn sql(client: &Client, text: &str) {
    tokio::time::timeout(DEADLINE, client.batch_execute(text))
        .await
        .expect("native builtin fixture command did not complete")
        .unwrap();
}

async fn observation(client: &Client, snapshot: bool, builtin_count: u64) -> Value {
    tokio::time::timeout(DEADLINE, async {
        let mut events = client.simple_query_events(SHOW).unwrap();
        let mut order = Vec::new();
        let mut result = None;
        while let Some(event) = events.next().await {
            match event.unwrap() {
                SimpleQueryEvent::RowDescription(columns) => {
                    assert_eq!(columns.len(), 1);
                    assert_eq!(columns[0].name(), "darmok_catalog_probe.builtin_dispatch");
                    assert_eq!(columns[0].type_oid(), 25);
                    assert_eq!(columns[0].format(), 0);
                    order.push("description");
                }
                SimpleQueryEvent::Row(row) => {
                    assert!(result.is_none());
                    result = Some(serde_json::from_str::<Value>(row.get(0).unwrap()).unwrap());
                    order.push("row");
                }
                SimpleQueryEvent::CommandComplete(tag) => {
                    assert_eq!(tag, "SHOW");
                    order.push("tag");
                }
                SimpleQueryEvent::ReadyForQuery(state) => {
                    assert_eq!(state, TransactionState::Transaction);
                    order.push("ready");
                }
                other => panic!("unexpected native builtin observation: {other:?}"),
            }
        }
        assert_eq!(order, ["description", "row", "tag", "ready"]);
        let result = result.unwrap();
        assert_eq!(result.as_object().unwrap().len(), 6);
        assert_eq!(result["before_snapshot"], snapshot);
        assert_eq!(result["after_snapshot"], snapshot);
        assert_eq!(result["owners_unchanged"], true);
        assert_eq!(result["builtin_count"], builtin_count);
        assert!(result["last_builtin_oid"].as_u64().unwrap() >= 330);
        let rows = result["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for (row, (oid, name)) in rows
            .iter()
            .zip([(3, "heap_tableam_handler"), (330, "bthandler")])
        {
            assert_eq!(row.as_object().unwrap().len(), 7);
            assert_eq!(row["oid"], oid);
            assert_eq!(row["name"], name);
            assert_eq!(row["nargs"], 1);
            assert_eq!(row["strict"], true);
            assert_eq!(row["retset"], false);
            assert_eq!(row["linked_symbol"], true);
            assert!(row["index"].as_u64().unwrap() < builtin_count);
        }
        assert_eq!(rows[0]["index"], 0);
        assert!(rows[1]["index"].as_u64().unwrap() > 0);
        result
    })
    .await
    .expect("native builtin observation did not complete")
}

async fn fixture(data_snapshot: bool) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("native builtin fixture requires its primary product/probe profile");
    let (client, connection) = tokio::time::timeout(DEADLINE, tokio_postgres::connect(&url, NoTls))
        .await
        .unwrap()
        .unwrap();
    let driver = tokio::spawn(connection);
    let version = tokio::time::timeout(DEADLINE, client.query_one("SHOW server_version_num", &[]))
        .await
        .unwrap()
        .unwrap();
    // Independent release observations bind the two pinned executable counts.
    // A different profile must be verified explicitly, not guessed by major.
    let builtin_count = match version.get::<_, String>(0).as_str() {
        "170011" => 3023,
        "180006" => 3102,
        other => panic!("unverified native builtin fixture version: {other}"),
    };
    sql(&client, "LOAD '$libdir/darmok_catalog_probe'").await;
    sql(&client, "BEGIN ISOLATION LEVEL REPEATABLE READ").await;
    if data_snapshot {
        tokio::time::timeout(DEADLINE, client.query_one("SELECT 1::int4", &[]))
            .await
            .unwrap()
            .unwrap();
    }
    let initial = observation(&client, data_snapshot, builtin_count).await;
    sql(&client, "SAVEPOINT builtin_observation").await;
    assert_eq!(
        observation(&client, data_snapshot, builtin_count).await,
        initial
    );
    sql(&client, "RELEASE SAVEPOINT builtin_observation").await;
    assert_eq!(
        observation(&client, data_snapshot, builtin_count).await,
        initial
    );
    sql(&client, "COMMIT").await;
    drop(client);
    tokio::time::timeout(DEADLINE, driver)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn linked_builtin_rows_preserve_the_unselected_data_view() {
    fixture(false).await;
}

#[tokio::test]
async fn linked_builtin_rows_preserve_the_selected_data_view() {
    fixture(true).await;
}
