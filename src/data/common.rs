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
#[serde(rename_all(serialize = "camelCase"))]
pub enum InstrumentType {
    #[serde(alias = "STOCK", alias = "stock")]
    Stock,
    #[serde(alias = "BOND", alias = "bond")]
    Bond,
    #[serde(alias = "DERIVATIVE", alias = "derivative")]
    Derivative,
    #[serde(alias = "SYNTHETIC", alias = "synthetic")]
    Synthetic,
    #[serde(alias = "FUND", alias = "fund")]
    Fund,
    #[serde(alias = "CRYPTO", alias = "crypto")]
    Crypto,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub enum DerivativeProductCategory {
    #[serde(alias = "vanillaWarrant", alias = "VANILLA_WARRANT")]
    VanillaWarrant,
    #[serde(alias = "trackerCertificate", alias = "TRACKER_CERTIFICATE")]
    TrackerCertificate,
    #[serde(
        alias = "factorCertificate",
        alias = "FACTOR_CERTIFICATE",
        alias = "FAKTOR_CERTIFICATE"
    )]
    FactorCertificate,
    #[serde(alias = "knockOutProduct", alias = "KNOCK_OUT_PRODUCT")]
    KnockOutProduct,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub enum UseOfProfits {
    #[serde(alias = "distributing", alias = "DISTRIBUTING")]
    Distributing,
    #[serde(alias = "accumulating", alias = "ACCUMULATING")]
    Accumulating,
    #[serde(alias = "noIncome", alias = "no_income", alias = "NO_INCOME")]
    NoIncome,
}
