use async_trait::async_trait;
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq)]
pub struct CurrencyQuery {
    pub amount: f64,
    pub from_currency: String,
    pub to_currency: String,
}

#[derive(Clone, Debug)]
pub struct CurrencyResult {
    pub amount: f64,
    pub from_currency: String,
    pub to_currency: String,
    pub converted_amount: f64,
    pub rate: f64,
    pub rate_date: String,
    pub as_of: SystemTime,
}

impl CurrencyResult {
    pub fn to_prompt_context(&self) -> String {
        format!(
            "{:.2} {} = {:.2} {} (rate {:.6}, {})",
            self.amount,
            self.from_currency,
            self.converted_amount,
            self.to_currency,
            self.rate,
            self.rate_date
        )
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum CurrencyError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("unsupported currency: {0}")]
    UnsupportedCurrency(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("provider timeout")]
    UpstreamTimeout,
    #[error("provider response parse failed: {0}")]
    UpstreamParse(String),
}

#[async_trait]
pub trait CurrencySkill: Send + Sync {
    async fn execute(&self, query: &CurrencyQuery) -> Result<CurrencyResult, CurrencyError>;
}
