use crate::types::{FuelPriceLookupError, FuelPriceLookupQuery, FuelPriceLookupResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

#[async_trait]
pub trait FuelPriceDataProvider: Send + Sync {
    async fn fetch(
        &self,
        query: &FuelPriceLookupQuery,
    ) -> Result<FuelPriceLookupResult, FuelPriceLookupError>;
}

#[derive(Clone)]
pub struct HttpFuelPriceDataProvider {
    client: reqwest::Client,
    uk_url: String,
    us_url: String,
    us_api_key: Option<String>,
}

impl HttpFuelPriceDataProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            uk_url: "https://fuelprices.energysavingtrust.org.uk/api/fuel_prices.json".to_string(),
            us_url: "https://api.eia.gov/v2/petroleum/pri/gnd/data/".to_string(),
            us_api_key: std::env::var("EIA_API_KEY").ok(),
        }
    }
    fn normalize_country(country: &str) -> Result<String, FuelPriceLookupError> {
        let normalized = country.trim().to_uppercase();
        if normalized.len() != 2 || !normalized.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(FuelPriceLookupError::InvalidCountry(country.to_string()));
        }
        Ok(normalized)
    }
}
impl Default for HttpFuelPriceDataProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct UkResponse {
    stations: Vec<UkStation>,
}
#[derive(Debug, Deserialize)]
struct UkStation {
    prices: HashMap<String, f64>,
}
#[derive(Debug, Deserialize)]
struct EiaResponse {
    response: EiaData,
}
#[derive(Debug, Deserialize)]
struct EiaData {
    data: Vec<EiaItem>,
}
#[derive(Debug, Deserialize)]
struct EiaItem {
    value: Option<String>,
    #[serde(rename = "area-name")]
    area_name: Option<String>,
}

#[async_trait]
impl FuelPriceDataProvider for HttpFuelPriceDataProvider {
    async fn fetch(
        &self,
        query: &FuelPriceLookupQuery,
    ) -> Result<FuelPriceLookupResult, FuelPriceLookupError> {
        let country = Self::normalize_country(&query.country_code)?;
        match country.as_str() {
            "GB" => {
                let response = self
                    .client
                    .get(&self.uk_url)
                    .timeout(Duration::from_secs(8))
                    .send()
                    .await
                    .map_err(|err| {
                        if err.is_timeout() {
                            FuelPriceLookupError::UpstreamTimeout
                        } else {
                            FuelPriceLookupError::ProviderUnavailable(err.to_string())
                        }
                    })?;
                if !response.status().is_success() {
                    return Err(FuelPriceLookupError::ProviderUnavailable(format!(
                        "status {}",
                        response.status()
                    )));
                }
                let payload = response
                    .json::<UkResponse>()
                    .await
                    .map_err(|err| FuelPriceLookupError::UpstreamParse(err.to_string()))?;
                let key = query
                    .fuel_type
                    .clone()
                    .unwrap_or_else(|| "E10".to_string())
                    .to_uppercase();
                let mut prices = Vec::new();
                for station in payload.stations {
                    if let Some(price) = station.prices.get(&key) {
                        prices.push(*price);
                    }
                }
                if prices.is_empty() {
                    return Err(FuelPriceLookupError::ProviderUnavailable(
                        "no prices available".to_string(),
                    ));
                }
                let average = prices.iter().sum::<f64>() / prices.len() as f64;
                Ok(FuelPriceLookupResult {
                    country_code: country,
                    region: query.region.clone(),
                    fuel_type: key,
                    price: average,
                    currency: "GBP".to_string(),
                    unit: "liter".to_string(),
                    source_granularity: "forecourt-average".to_string(),
                    as_of: SystemTime::now(),
                })
            }
            "US" => {
                let api_key = self
                    .us_api_key
                    .clone()
                    .ok_or(FuelPriceLookupError::MissingApiKey)?;
                let response = self
                    .client
                    .get(&self.us_url)
                    .query(&[
                        ("api_key", api_key),
                        ("frequency", "weekly".to_string()),
                        ("data[0]", "value".to_string()),
                        ("facets[product][]", "EPM0".to_string()),
                        ("sort[0][column]", "period".to_string()),
                        ("sort[0][direction]", "desc".to_string()),
                        ("offset", "0".to_string()),
                        ("length", "1".to_string()),
                    ])
                    .timeout(Duration::from_secs(8))
                    .send()
                    .await
                    .map_err(|err| {
                        if err.is_timeout() {
                            FuelPriceLookupError::UpstreamTimeout
                        } else {
                            FuelPriceLookupError::ProviderUnavailable(err.to_string())
                        }
                    })?;
                if !response.status().is_success() {
                    return Err(FuelPriceLookupError::ProviderUnavailable(format!(
                        "status {}",
                        response.status()
                    )));
                }
                let payload = response
                    .json::<EiaResponse>()
                    .await
                    .map_err(|err| FuelPriceLookupError::UpstreamParse(err.to_string()))?;
                let first = payload.response.data.first().ok_or_else(|| {
                    FuelPriceLookupError::ProviderUnavailable("no data".to_string())
                })?;
                let value = first
                    .value
                    .as_ref()
                    .ok_or_else(|| {
                        FuelPriceLookupError::UpstreamParse("missing value".to_string())
                    })?
                    .parse::<f64>()
                    .map_err(|err| FuelPriceLookupError::UpstreamParse(err.to_string()))?;
                Ok(FuelPriceLookupResult {
                    country_code: country,
                    region: first.area_name.clone(),
                    fuel_type: query
                        .fuel_type
                        .clone()
                        .unwrap_or_else(|| "gasoline".to_string()),
                    price: value,
                    currency: "USD".to_string(),
                    unit: "gallon".to_string(),
                    source_granularity: "national-weekly".to_string(),
                    as_of: SystemTime::now(),
                })
            }
            other => Err(FuelPriceLookupError::UnsupportedCountry(other.to_string())),
        }
    }
}
