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
use std::time::Duration;
use std::net::TcpListener;
use std::thread;
use tokio::time::timeout;
use tokio_executor_trait::Tokio;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let settings = Settings::new()?;
    let batch_size = settings.application.batch;
    let idle_time = Duration::from_millis(settings.application.idle);
    println!(
        "Starting consumer - batch size {}; idle time {}ms",
        batch_size,
        idle_time.as_millis()
    );
    let mut consumer = build_rabbitmq_consumer(&settings.rabbit)
        .await
        .with_context(|| "Unable to build rabbitmq consumer")?;

    let client = build_elasticsearch_client(&settings.elasticsearch)
        .with_context(|| "Unable to build elasticsearch client")?;

    let mut deliveries: HashMap<String, Delivery> = HashMap::with_capacity(batch_size);
    let mut bulk_updates: HashMap<String, Value> = HashMap::with_capacity(batch_size);

    let _ = thread::spawn(build_probe);

    println!("Starting consuming messages");
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
                eprintln!("{}", error);
                ack_all(&mut deliveries).await;
                continue;
            }
        };
        if (response.json::<Map<String, Value>>().await).is_ok() {
            println!("Updated {} instrument(s)", update_count);
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

async fn build_rabbitmq_consumer(settings: &RabbitMQSettings) -> Result<Consumer> {
    println!(
        "Connecting to vhost '{}' at amqp://{}:****@{}:{} from queue '{}'",
        settings.vhost, settings.user, settings.host, settings.port, settings.queue
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
    println!(
        "Connecting to elasticsearch with user '{}' at {}",
        settings.user, settings.url
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
    let listener = TcpListener::bind("0.0.0.0:8081")?;

    println!("Probes installed in port 8081");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buf = [0;256];
                stream.read(&mut buf).ok();
                stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").ok();
            }
            Err(e) => {
                eprintln!("Unable to accept connection {:?}", e)
            }
        }
    }
    Ok(())
}
