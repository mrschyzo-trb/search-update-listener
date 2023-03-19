mod data;

use crate::data::inbound::InstrumentChangeMessage;
use crate::data::outbound::InstrumentUniverseChangeSet;
use anyhow::{Context, Error, Result};
use elasticsearch::auth::Credentials;
use elasticsearch::http::transport::{SingleNodeConnectionPool, TransportBuilder};
use elasticsearch::http::Url;
use elasticsearch::{BulkOperation, BulkParts, Elasticsearch};
use futures_lite::stream::StreamExt;
use lapin::message::Delivery;
use lapin::{options::*, types::FieldTable, Connection, ConnectionProperties, Consumer};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
use tokio_executor_trait::Tokio;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let batch_size = 1000;
    let idle_time = Duration::from_millis(10000u64);
    let mut consumer = build_rabbitmq_consumer()
        .await
        .with_context(|| "Unable to build rabbitmq consumer")?;

    let client =
        build_elasticsearch_client().with_context(|| "Unable to build elasticsearch client")?;

    let mut deliveries: HashMap<String, Delivery> = HashMap::with_capacity(batch_size);
    let mut bulk_updates: HashMap<String, Value> = HashMap::with_capacity(batch_size);

    loop {
        let attempted_next = timeout(idle_time, consumer.next()).await;
        match attempted_next {
            Ok(None) | Err(_) => {
                if bulk_updates.is_empty() {
                    continue;
                }
            }
            Ok(Some(Err(failed_delivery))) => {
                eprintln!("Failed delivery from RMQ {}", failed_delivery);
                continue;
            }
            Ok(Some(Ok(delivery))) => {
                handle_delivery(&mut deliveries, &mut bulk_updates, delivery).await;
                if bulk_updates.len() < batch_size {
                    continue;
                }
            }
        };
        let bulk_requests: Vec<BulkOperation<Value>> = bulk_updates
            .drain()
            .map(|(isin, value)| {
                let mut request: Map<String, Value> = serde_json::Map::new();
                request.insert("doc_as_upsert".to_owned(), Value::Bool(true));
                request.insert("doc".to_owned(), value);
                BulkOperation::update(isin, request.into()).into()
            })
            .collect();

        let response = match client
            .bulk(BulkParts::Index("instruments"))
            .body(bulk_requests)
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => {
                ack_all(&mut deliveries).await;
                continue;
            }
        };
        if let Ok(json) = response.json::<Map<String, Value>>().await {
            println!("{:?}", json);
        };
        ack_all(&mut deliveries).await;
    }
}

async fn ack_all(deliveries: &mut HashMap<String, Delivery>) {
    if let Some((_, delivery)) = deliveries
        .drain()
        .max_by_key(|(_, delivery)| delivery.delivery_tag)
    {
        delivery.ack(BasicAckOptions { multiple: true }).await.ok();
    }
}

async fn handle_delivery(
    deliveries: &mut HashMap<String, Delivery>,
    bulk_updates: &mut HashMap<String, Value>,
    delivery: Delivery,
) {
    let change_set = match into_change_set(&delivery) {
        Err(error) => {
            eprintln!("Unable to turn message into changeset: {}", error);
            delivery.reject(BasicRejectOptions::default()).await.ok();
            return;
        }
        Ok(change_set) => change_set,
    };
    let value = match into_value(&change_set) {
        Err(error) => {
            eprintln!("Unable to turn message into changeset: {}", error);
            delivery.reject(BasicRejectOptions::default()).await.ok();
            return;
        }
        Ok(value) => value,
    };

    if let Some(collided_delivery) = deliveries.insert(change_set.isin.clone(), delivery) {
        collided_delivery.ack(BasicAckOptions::default()).await.ok();
    }
    bulk_updates.insert(change_set.isin, value);
}

fn into_value(change_set: &InstrumentUniverseChangeSet) -> Result<Value, Error> {
    serde_json::to_value(change_set).with_context(|| {
        format!(
            "Failed to serialise changeset for instrument {}",
            change_set.isin
        )
    })
}

fn into_change_set(delivery: &Delivery) -> Result<InstrumentUniverseChangeSet> {
    let delivered = &delivery.data;
    let data = std::str::from_utf8(delivered).with_context(|| {
        format!(
            "Cannot decode delivered bytes from rabbitMQ: {:?}",
            delivery.properties.message_id()
        )
    })?;
    let change: InstrumentChangeMessage = serde_json::from_str(data)
        .with_context(|| format!("Received data wasn't deserialisable: {}", data))?;

    Ok(change.data.into())
}

async fn build_rabbitmq_consumer() -> Result<Consumer> {
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

fn build_elasticsearch_client() -> Result<Elasticsearch> {
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
