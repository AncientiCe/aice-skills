use async_trait::async_trait;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct FuelPriceLookupQuery {
    pub country_code: String,
    pub region: Option<String>,
    pub fuel_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FuelPriceLookupResult {
    pub country_code: String,
    pub region: Option<String>,
    pub fuel_type: String,
    pub price: f64,
    pub currency: String,
    pub unit: String,
    pub source_granularity: String,
    pub as_of: SystemTime,
}

impl FuelPriceLookupResult {
    pub fn to_prompt_context(&self) -> String {
        format!(
            "{} fuel price in {}: {:.3} {} per {}",
            self.fuel_type, self.country_code, self.price, self.currency, self.unit
        )
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum FuelPriceLookupError {
    #[error("invalid country code: {0}")]
    InvalidCountry(String),
    #[error("unsupported country: {0}")]
    UnsupportedCountry(String),
    #[error("missing API key for US EIA data")]
    MissingApiKey,
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("provider timeout")]
    UpstreamTimeout,
    #[error("provider response parse failed: {0}")]
    UpstreamParse(String),
}

#[async_trait]
pub trait FuelPriceLookupSkill: Send + Sync {
    async fn execute(
        &self,
        query: &FuelPriceLookupQuery,
    ) -> Result<FuelPriceLookupResult, FuelPriceLookupError>;
}
