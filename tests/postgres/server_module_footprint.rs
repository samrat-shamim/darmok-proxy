//! Ordinary private module observations; these do not certify entry ownership.
use futures_util::StreamExt;
use serde_json::Value;
use std::time::Duration;
use tokio_postgres::{Client, NoTls, SimpleQueryEvent, TransactionState};

const DEADLINE: Duration = Duration::from_secs(20);
const SHOW: &str = "SHOW darmok_catalog_probe.module_footprint";

async fn sql(client: &Client, text: &str) {
    tokio::time::timeout(DEADLINE, client.batch_execute(text))
        .await
        .expect("native module fixture command did not complete")
        .unwrap();
}

async fn observation(client: &Client, snapshot: bool) -> Value {
    tokio::time::timeout(DEADLINE, async {
        let mut events = client.simple_query_events(SHOW).unwrap();
        let mut order = Vec::new();
        let mut result = None;
        while let Some(event) = events.next().await {
            match event.unwrap() {
                SimpleQueryEvent::RowDescription(columns) => {
                    assert_eq!(columns.len(), 1);
                    assert_eq!(columns[0].name(), "darmok_catalog_probe.module_footprint");
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
                other => panic!("unexpected native module observation: {other:?}"),
            }
        }
        assert_eq!(order, ["description", "row", "tag", "ready"]);
        let result = result.unwrap();
        assert_eq!(result["before_snapshot"], snapshot);
        assert_eq!(result["after_snapshot"], snapshot);
        assert_eq!(result["owners_unchanged"], true);

        // Independently decode the complete native image, including its final
        // terminator. Paths are bytes; server character encoding is irrelevant.
        let hex = result["image_hex"].as_str().unwrap();
        assert_eq!(hex.len() % 2, 0);
        let image: Vec<u8> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(image.len() as u64, result["bytes"].as_u64().unwrap());
        assert_eq!(image.last(), Some(&0));
        let paths: Vec<_> = image[..image.len() - 1]
            .split_inclusive(|byte| *byte == 0)
            .map(|path| {
                assert!(path.len() > 1 && path.last() == Some(&0));
                &path[..path.len() - 1]
            })
            .collect();
        assert_eq!(paths.len() as u64, result["paths"].as_u64().unwrap());
        assert_eq!(
            paths,
            [
                b"/usr/local/lib/postgresql/darmok_server.so".as_slice(),
                b"/usr/local/lib/postgresql/darmok_catalog_probe.so".as_slice(),
            ]
        );
        let requested = result["requested_bytes"].as_u64().unwrap();
        assert!(requested > image.len() as u64 && requested <= 1024 * 1024 + 128);
        result
    })
    .await
    .expect("native module observation did not complete")
}

async fn fixture(data_snapshot: bool) {
    let url = std::env::var("DARMOK_TEST_DATABASE_URL")
        .expect("native module fixture requires its primary product/probe profile");
    let (client, connection) = tokio::time::timeout(DEADLINE, tokio_postgres::connect(&url, NoTls))
        .await
        .unwrap()
        .unwrap();
    let driver = tokio::spawn(connection);
    sql(&client, "LOAD '$libdir/darmok_catalog_probe'").await;
    sql(&client, "BEGIN ISOLATION LEVEL REPEATABLE READ").await;
    if data_snapshot {
        tokio::time::timeout(DEADLINE, client.query_one("SELECT 1::int4", &[]))
            .await
            .unwrap()
            .unwrap();
    }
    let initial = observation(&client, data_snapshot).await;
    sql(&client, "SAVEPOINT module_observation").await;
    assert_eq!(observation(&client, data_snapshot).await, initial);
    sql(&client, "RELEASE SAVEPOINT module_observation").await;
    assert_eq!(observation(&client, data_snapshot).await, initial);
    sql(&client, "COMMIT").await;
    drop(client);
    tokio::time::timeout(DEADLINE, driver)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn complete_owned_image_preserves_the_unselected_data_view() {
    fixture(false).await;
}

#[tokio::test]
async fn complete_owned_image_preserves_the_selected_data_view() {
    fixture(true).await;
}
