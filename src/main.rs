mod configuration;
mod data;

use crate::configuration::{ElasticsearchSettings, RabbitMQSettings, Settings};
use crate::data::inbound::InstrumentChangeMessage;
use crate::data::outbound::InstrumentUniverseChangeSet;
use anyhow::{Context, Error, Result};
use elasticsearch::auth::Credentials;
use elasticsearch::http::transport::{SingleNodeConnectionPool, TransportBuilder};
use elasticsearch::http::Url;
use elasticsearch::{BulkOperation, BulkParts, Elasticsearch};
use futures_lite::stream::StreamExt;
use lapin::message::Delivery;
use lapin::tcp::OwnedTLSConfig;
use lapin::uri::{AMQPAuthority, AMQPScheme, AMQPUri, AMQPUserInfo};
use lapin::{options::*, types::FieldTable, Connect, ConnectionProperties, Consumer};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;
use tokio::time::timeout;
use tokio_executor_trait::Tokio;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    setup_global_tracing_subscriber()?;

    let settings = Settings::new()?;
    let batch_size = settings.application.batch;
    let idle_time = Duration::from_millis(settings.application.idle);

    tracing::info!(
        batch_size = batch_size,
        idle_time = idle_time.as_millis(),
        "Booting up listener"
    );

    let mut consumer = build_rabbitmq_consumer(&settings.rabbit)
        .await
        .with_context(|| "Unable to build rabbitmq consumer")?;

    let client = build_elasticsearch_client(&settings.elasticsearch)
        .with_context(|| "Unable to build elasticsearch client")?;

    let mut deliveries: HashMap<String, Delivery> = HashMap::with_capacity(batch_size);
    let mut bulk_updates: HashMap<String, Value> = HashMap::with_capacity(batch_size);

    let _ = thread::spawn(build_probe);

    tracing::info!("Start RMQ consumption");
    loop {
        let attempted_next = timeout(idle_time, consumer.next()).await;
        match attempted_next {
            Ok(None) | Err(_) => {
                if bulk_updates.is_empty() {
                    continue;
                }
            }
            Ok(Some(Err(failed_delivery))) => {
                tracing::error!("Failed delivery from RMQ {:?}", failed_delivery);
                continue;
            }
            Ok(Some(Ok(delivery))) => {
                handle_delivery(&mut deliveries, &mut bulk_updates, delivery).await;
                if bulk_updates.len() < batch_size {
                    continue;
                }
            }
        };

        let update_count = bulk_updates.len();
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
            Err(error) => {
                tracing::error!(error = format!("{:?}", error), "Failed upsert");
                ack_all(&mut deliveries).await;
                continue;
            }
        };
        if (response.json::<Map<String, Value>>().await).is_ok() {
            tracing::info!(update_count = update_count, "Success upsert");
        };
        ack_all(&mut deliveries).await;
    }
}

fn setup_global_tracing_subscriber() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .json()
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .with_context(|| "Unable to create tracing subscriber".to_owned())
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
            tracing::error!(error = format!("{:?}", error), "Message -> changeset: fail");
            delivery.reject(BasicRejectOptions::default()).await.ok();
            return;
        }
        Ok(change_set) => change_set,
    };
    let value = match into_value(&change_set) {
        Err(error) => {
            tracing::error!(error = format!("{:?}", error), "Changeset -> json: fail");
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

async fn build_rabbitmq_consumer(settings: &RabbitMQSettings) -> Result<Consumer> {
    tracing::info!(
        vhost = settings.vhost,
        user = settings.user,
        host = settings.host,
        port = settings.port,
        queue = settings.queue,
        "Booting up RMQ consumer"
    );
    let channel = AMQPUri {
        scheme: AMQPScheme::AMQP,
        authority: AMQPAuthority {
            userinfo: AMQPUserInfo {
                username: settings.user.clone(),
                password: settings.password.clone(),
            },
            host: settings.host.clone(),
            port: settings.port,
        },
        vhost: settings.vhost.clone(),
        query: lapin::uri::AMQPQueryString::default(),
    }
    .connect(
        ConnectionProperties::default().with_executor(Tokio::current()),
        OwnedTLSConfig::default(),
    )
    .await?
    .create_channel()
    .await?;

    channel
        .basic_qos(settings.prefetch, BasicQosOptions::default())
        .await?;

    let consumer = channel
        .basic_consume(
            &settings.queue,
            &settings.tag,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;
    Ok(consumer)
}

fn build_elasticsearch_client(settings: &ElasticsearchSettings) -> Result<Elasticsearch> {
    tracing::info!(
        user = settings.user,
        url = settings.url,
        "Listener connecting to elasticsearch"
    );

    let url = Url::parse(&settings.url)?;
    let conn_pool = SingleNodeConnectionPool::new(url);
    let transport = TransportBuilder::new(conn_pool)
        .auth(Credentials::Basic(
            settings.user.clone(),
            settings.password.clone(),
        ))
        .build()?;

    Ok(Elasticsearch::new(transport))
}

fn build_probe() -> Result<()> {
    let addr = "0.0.0.0:8081";
    let listener = TcpListener::bind(addr)?;

    tracing::info!(addr = addr, "Readiness and liveness probes online");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buf = [0; 256];
                stream.read(&mut buf).ok();
                stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").ok();
            }
            Err(e) => {
                tracing::warn!(error = format!("{:?}", e), "Unable to accept connection")
            }
        }
    }
    Ok(())
}
