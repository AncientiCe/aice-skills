use crate::types::{CurrencyError, CurrencyQuery, CurrencyResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

#[async_trait]
pub trait CurrencyRateProvider: Send + Sync {
    async fn fetch(&self, query: &CurrencyQuery) -> Result<CurrencyResult, CurrencyError>;
}

#[derive(Clone)]
pub struct FrankfurterProvider {
    client: reqwest::Client,
    base_url: String,
}

impl FrankfurterProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.frankfurter.app".to_string(),
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }
}

impl Default for FrankfurterProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct FrankfurterResponse {
    amount: f64,
    base: String,
    date: String,
    rates: HashMap<String, f64>,
}

#[async_trait]
impl CurrencyRateProvider for FrankfurterProvider {
    async fn fetch(&self, query: &CurrencyQuery) -> Result<CurrencyResult, CurrencyError> {
        let from = normalize_code(&query.from_currency)?;
        let to = normalize_code(&query.to_currency)?;
        if !query.amount.is_finite() {
            return Err(CurrencyError::InvalidQuery("amount not finite".to_string()));
        }
        if from == to {
            return Ok(CurrencyResult {
                amount: query.amount,
                from_currency: from.clone(),
                to_currency: to,
                converted_amount: query.amount,
                rate: 1.0,
                rate_date: "n/a".to_string(),
                as_of: SystemTime::now(),
            });
        }
        let url = format!(
            "{}/latest?amount={}&from={}&to={}",
            self.base_url, query.amount, from, to
        );
        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    CurrencyError::UpstreamTimeout
                } else {
                    CurrencyError::ProviderUnavailable(err.to_string())
                }
            })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND
            || response.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY
        {
            return Err(CurrencyError::UnsupportedCurrency(format!("{from}->{to}")));
        }
        if !response.status().is_success() {
            return Err(CurrencyError::ProviderUnavailable(format!(
                "status {}",
                response.status()
            )));
        }
        let payload = response
            .json::<FrankfurterResponse>()
            .await
            .map_err(|err| CurrencyError::UpstreamParse(err.to_string()))?;
        let converted = payload
            .rates
            .get(&to)
            .copied()
            .ok_or_else(|| CurrencyError::UpstreamParse(format!("missing rate for {to}")))?;
        let rate = if payload.amount == 0.0 {
            0.0
        } else {
            converted / payload.amount
        };
        Ok(CurrencyResult {
            amount: payload.amount,
            from_currency: payload.base,
            to_currency: to,
            converted_amount: converted,
            rate,
            rate_date: payload.date,
            as_of: SystemTime::now(),
        })
    }
}

fn normalize_code(raw: &str) -> Result<String, CurrencyError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CurrencyError::InvalidQuery(
            "empty currency code".to_string(),
        ));
    }
    if trimmed.len() != 3 || !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(CurrencyError::InvalidQuery(format!(
            "currency code must be 3 letters: {raw}"
        )));
    }
    Ok(trimmed.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_uppercases_and_trims() {
        assert_eq!(normalize_code("usd").unwrap(), "USD");
        assert_eq!(normalize_code(" eur ").unwrap(), "EUR");
    }

    #[test]
    fn normalize_rejects_invalid() {
        assert!(matches!(
            normalize_code("US"),
            Err(CurrencyError::InvalidQuery(_))
        ));
        assert!(matches!(
            normalize_code("US1"),
            Err(CurrencyError::InvalidQuery(_))
        ));
        assert!(matches!(
            normalize_code(""),
            Err(CurrencyError::InvalidQuery(_))
        ));
    }
}
