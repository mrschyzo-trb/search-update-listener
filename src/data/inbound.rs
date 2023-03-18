use super::common::*;
use serde::*;
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentChangeMessage {
    pub id: uuid::Uuid,
    pub r#type: String,
    pub source_host: String,
    pub source_system: String,
    pub data: PlatformDTO,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlatformDTO {
    pub isin: String,
    pub instrument_type: InstrumentType,
    pub name_official: String,
    pub name_short: Option<String>,
    pub name_short_localized: Option<HashMap<String, String>>,
    pub wkn: String,
    pub symbol: Option<String>,
    pub intl_symbol: Option<String>,
    pub underlying_isin: Option<String>,
    pub underlying_name: Option<String>,
    pub product_category: Option<DerivativeProductCategory>,
    pub exchange_ids: Vec<String>,
    pub first_seen: Option<String>,
    pub jurisdictions: Vec<Jurisdiction>,
    pub sectors: Vec<String>,
    pub regions: Vec<String>,
    pub countries: Vec<String>,
    pub indices: Vec<String>,
    pub attributes: Option<Vec<String>>,
    pub issuer: Option<String>,
    pub issuer_display_name: Option<String>,
    pub ter: Option<String>,
    pub use_of_profits: Option<UseOfProfits>,
}
