mod data;

use crate::data::inbound::InstrumentChangeMessage;
use crate::data::outbound::InstrumentUniverseChangeSet;
use anyhow::Context;
use elasticsearch::auth::Credentials;
use elasticsearch::http::transport::{SingleNodeConnectionPool, TransportBuilder};
use elasticsearch::http::Url;
use elasticsearch::{Elasticsearch, UpdateParts};
use futures_lite::stream::StreamExt;
use lapin::{options::*, types::FieldTable, Connection, ConnectionProperties, Consumer};
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;
use tokio_executor_trait::Tokio;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut consumer = build_rabbitmq_consumer()
        .await
        .with_context(|| "Unable to build rabbitmq consumer")?;

    let client =
        build_elasticsearch_client().with_context(|| "Unable to build elasticsearch client")?;

    while let Ok(Some(Ok(delivery))) =
        timeout(Duration::from_millis(1000u64), consumer.next()).await
    {
        let delivered = &delivery.data;
        let data = std::str::from_utf8(delivered).with_context(|| {
            format!(
                "Cannot decode delivered bytes from rabbitMQ: {:?}",
                delivery.properties.message_id()
            )
        })?;
        let change: InstrumentChangeMessage = serde_json::from_str(data)
            .with_context(|| format!("Received data wasn't deserialisable: {}", data))?;
        let change_set: InstrumentUniverseChangeSet = change.data.into();
        let serialised_change_set = serde_json::to_string(&change_set).with_context(|| {
            format!(
                "Failed to serialise changeset for instrument {}",
                &change_set.isin
            )
        })?;
        let mut request: serde_json::Map<String, Value> = serde_json::Map::new();
        request.insert("doc_as_upsert".to_owned(), Value::Bool(true));
        request.insert(
            "doc".to_owned(),
            Value::Object(serde_json::from_str(&serialised_change_set)?),
        );

        let json: serde_json::Map<String, Value> = client
            .update(UpdateParts::IndexId(
                "instruments",
                change_set.isin.as_str(),
            ))
            .body(request)
            .send()
            .await?
            .json()
            .await?;
        println!("{:?}", json);
        delivery.ack(BasicAckOptions::default()).await?;
    }
    Ok(())
}

async fn build_rabbitmq_consumer() -> anyhow::Result<Consumer> {
    let addr = "amqp://127.0.0.1:5672/%2f";
    let tokio = Tokio::current();
    let conn =
        Connection::connect(addr, ConnectionProperties::default().with_executor(tokio)).await?;
    let channel = conn.create_channel().await?;
    let consumer = channel
        .basic_consume(
            "instrumentsearch.platform",
            "rusterino",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;
    Ok(consumer)
}

fn build_elasticsearch_client() -> anyhow::Result<Elasticsearch> {
    let url = Url::parse("http://localhost:9200")?;
    let conn_pool = SingleNodeConnectionPool::new(url);
    let transport = TransportBuilder::new(conn_pool)
        .auth(Credentials::Basic(
            "elastic".to_owned(),
            "123test".to_owned(),
        ))
        .disable_proxy()
        .build()?;

    Ok(Elasticsearch::new(transport))
}
