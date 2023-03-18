use super::common::*;
use super::inbound::*;
use serde::*;
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentUniverseChangeSet {
    pub attributes: HashSet<String>,
    pub countries: HashSet<String>,
    pub exchange_ids: HashSet<String>,
    pub indices: HashSet<String>,
    pub regions: HashSet<String>,
    pub savable: HashSet<String>,
    pub sectors: HashSet<String>,
    pub jurisdictions: HashSet<String>,
    pub first_seen: Option<String>,
    pub instrument_type: InstrumentType,
    pub intl_symbol: Option<String>,
    pub isin: String,
    pub issuer: Option<String>,
    pub issuer_display_name: Option<String>,
    pub official_name: String,
    pub product_category: Option<DerivativeProductCategory>,
    pub recall_name: String,
    pub short_name: Option<String>,
    pub short_name_localized: Option<HashMap<String, String>>,
    pub symbol: Option<String>,
    pub ter: Option<f64>,
    pub underlying_isin: Option<String>,
    pub underlying_name: Option<String>,
    pub use_of_profits: Option<UseOfProfits>,
    pub wkn: String,
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
