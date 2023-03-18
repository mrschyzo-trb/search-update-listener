use elasticsearch::auth::Credentials;
use elasticsearch::http::transport::{SingleNodeConnectionPool, TransportBuilder};
use elasticsearch::http::Url;
use elasticsearch::{Elasticsearch, UpdateParts};
use futures_lite::stream::StreamExt;
use lapin::{options::*, types::FieldTable, Connection, ConnectionProperties, Result};
use serde::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstrumentChangeMessage {
    id: uuid::Uuid,
    r#type: String,
    source_host: String,
    source_system: String,
    data: PlatformDTO,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct PlatformDTO {
    isin: String,
    instrument_type: InstrumentType,
    name_official: String,
    name_short: Option<String>,
    name_short_localized: Option<HashMap<String, String>>,
    wkn: String,
    symbol: Option<String>,
    intl_symbol: Option<String>,
    underlying_isin: Option<String>,
    underlying_name: Option<String>,
    product_category: Option<DerivativeProductCategory>,
    exchange_ids: Vec<String>,
    first_seen: Option<String>,
    jurisdictions: Vec<Jurisdiction>,
    sectors: Vec<String>,
    regions: Vec<String>,
    countries: Vec<String>,
    indices: Vec<String>,
    attributes: Option<Vec<String>>,
    issuer: Option<String>,
    issuer_display_name: Option<String>,
    ter: Option<String>,
    use_of_profits: Option<UseOfProfits>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Jurisdiction {
    jurisdiction: String,
    kid_link: Option<String>,
    kid_required: bool,
    savable: bool,
}

#[derive(Serialize, Deserialize)]
enum InstrumentType {
    #[serde(alias = "STOCK")]
    Stock,
    #[serde(alias = "BOND")]
    Bond,
    #[serde(alias = "DERIVATIVE")]
    Derivative,
    #[serde(alias = "SYNTHETIC")]
    Synthetic,
    #[serde(alias = "FUND")]
    Fund,
    #[serde(alias = "CRYPTO")]
    Crypto,
}

#[derive(Serialize, Deserialize)]
enum DerivativeProductCategory {
    #[serde(alias = "vanillaWarrant")]
    VanillaWarrant,
    #[serde(alias = "trackerCertificate")]
    TrackerCertificate,
    #[serde(alias = "factorCertificate")]
    FactorCertificate,
    #[serde(alias = "knockOutProduct")]
    KnockOutProduct,
}

#[derive(Serialize, Deserialize)]
enum UseOfProfits {
    #[serde(alias = "distributing")]
    Distributing,
    #[serde(alias = "accumulating")]
    Accumulating,
    #[serde(alias = "noIncome")]
    NoIncome,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstrumentUniverseChangeSet {
    attributes: HashSet<String>,
    countries: HashSet<String>,
    exchange_ids: HashSet<String>,
    indices: HashSet<String>,
    regions: HashSet<String>,
    savable: HashSet<String>,
    sectors: HashSet<String>,
    jurisdictions: HashSet<String>,
    first_seen: Option<String>,
    instrument_type: InstrumentType,
    intl_symbol: Option<String>,
    isin: String,
    issuer: Option<String>,
    issuer_display_name: Option<String>,
    official_name: String,
    product_category: Option<DerivativeProductCategory>,
    recall_name: String,
    short_name: Option<String>,
    short_name_localized: Option<HashMap<String, String>>,
    symbol: Option<String>,
    ter: Option<f64>,
    underlying_isin: Option<String>,
    underlying_name: Option<String>,
    use_of_profits: Option<UseOfProfits>,
    wkn: String,
}

impl From<PlatformDTO> for InstrumentUniverseChangeSet {
    fn from(value: PlatformDTO) -> Self {
        Self {
            attributes: value
                .attributes
                .map(|x| x.into_iter())
                .map(|x| {
                    x.chain(
                        vec!["savable".to_owned()]
                            .into_iter()
                            .filter(|_| value.jurisdictions.iter().any(|j| j.savable)),
                    )
                })
                .map(HashSet::from_iter)
                .unwrap_or(HashSet::new()),
            countries: HashSet::from_iter(value.countries),
            exchange_ids: HashSet::from_iter(value.exchange_ids),
            indices: HashSet::from_iter(value.indices),
            regions: HashSet::from_iter(value.regions),
            savable: HashSet::from_iter(
                value
                    .jurisdictions
                    .iter()
                    .filter(|x| x.savable)
                    .map(|x| x.jurisdiction.clone()),
            ),
            sectors: HashSet::from_iter(value.sectors),
            jurisdictions: HashSet::from_iter(
                value.jurisdictions.into_iter().map(|x| x.jurisdiction),
            ),
            first_seen: value.first_seen,
            instrument_type: value.instrument_type,
            intl_symbol: value.intl_symbol,
            isin: value.isin,
            issuer: value.issuer,
            issuer_display_name: value.issuer_display_name,
            official_name: value.name_official.clone(),
            product_category: value.product_category,
            short_name: value.name_short.clone(),
            recall_name: value.name_short.unwrap_or(value.name_official),
            short_name_localized: value.name_short_localized,
            symbol: value.symbol,
            ter: value.ter.and_then(|x| x.parse().ok()),
            underlying_isin: value.underlying_isin,
            underlying_name: value.underlying_name,
            use_of_profits: value.use_of_profits,
            wkn: value.wkn,
        }
    }
}
