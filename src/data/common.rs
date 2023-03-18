use serde::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Jurisdiction {
    pub jurisdiction: String,
    pub kid_link: Option<String>,
    pub kid_required: bool,
    pub savable: bool,
}

#[derive(Serialize, Deserialize)]
pub enum InstrumentType {
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
pub enum DerivativeProductCategory {
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
pub enum UseOfProfits {
    #[serde(alias = "distributing")]
    Distributing,
    #[serde(alias = "accumulating")]
    Accumulating,
    #[serde(alias = "noIncome")]
    NoIncome,
}
