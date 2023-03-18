mod data;

use crate::data::inbound::InstrumentChangeMessage;
use crate::data::outbound::InstrumentUniverseChangeSet;
use elasticsearch::auth::Credentials;
use elasticsearch::http::transport::{SingleNodeConnectionPool, TransportBuilder};
use elasticsearch::http::Url;
use elasticsearch::{Elasticsearch, UpdateParts};
use futures_lite::stream::StreamExt;
use lapin::{options::*, types::FieldTable, Connection, ConnectionProperties, Result};
use serde_json::Value;

use tokio::runtime::Runtime;

use tokio_executor_trait::Tokio;

async fn tokio_main() -> Result<()> {
    let addr = std::env::var("AMQP_ADDR").unwrap_or_else(|_| "amqp://127.0.0.1:5672/%2f".into());
    let tokio = Tokio::current();
    let conn =
        Connection::connect(&addr, ConnectionProperties::default().with_executor(tokio)).await?;
    let channel = conn.create_channel().await?;
    let url = Url::parse("http://localhost:9200").unwrap();
    let conn_pool = SingleNodeConnectionPool::new(url);
    let transport = TransportBuilder::new(conn_pool)
        .auth(Credentials::Basic(
            "elastic".to_owned(),
            "123test".to_owned(),
        ))
        .disable_proxy()
        .build()
        .unwrap();
    let client = Elasticsearch::new(transport);

    let mut consumer = channel
        .basic_consume(
            "instrumentsearch.platform",
            "rusterino",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;
    println!("Start consoom");
    while let Some(delivery) = consumer.next().await {
        let actual_delivery = delivery.unwrap();
        let delivered = &actual_delivery.data;
        let data = std::str::from_utf8(delivered).unwrap();
        let change: InstrumentChangeMessage = serde_json::from_str(data).unwrap();
        let change_set: InstrumentUniverseChangeSet = change.data.into();
        let serialised_change_set = serde_json::to_string_pretty(&change_set).unwrap();
        let mut request: serde_json::Map<String, Value> = serde_json::Map::new();
        request.insert("doc_as_upsert".to_owned(), Value::Bool(true));
        request.insert(
            "doc".to_owned(),
            Value::Object(serde_json::from_str(&serialised_change_set).unwrap()),
        );

        let response = client
            .update(UpdateParts::IndexId(
                "instruments",
                change_set.isin.as_str(),
            ))
            .body(request)
            .send()
            .await
            .unwrap();
        let json: serde_json::Map<String, Value> = response.json().await.unwrap();
        println!("{:?}", json);
        actual_delivery.ack(BasicAckOptions::default()).await?;
    }
    Ok(())
}

fn main() {
    let rt = Runtime::new().expect("failed to create runtime");
    rt.block_on(tokio_main()).expect("error");
}
