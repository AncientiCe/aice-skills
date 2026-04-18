use crate::types::{AirQualityError, AirQualityLocation, AirQualityResult, AqiCategory};
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

#[async_trait]
pub trait AirQualityProvider: Send + Sync {
    async fn geocode(&self, name: &str) -> Result<AirQualityLocation, AirQualityError>;
    async fn fetch(&self, loc: &AirQualityLocation) -> Result<AirQualityResult, AirQualityError>;
}

#[derive(Clone)]
pub struct OpenMeteoAirQualityProvider {
    client: reqwest::Client,
    geocoding_url: String,
    air_quality_url: String,
}

impl OpenMeteoAirQualityProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            geocoding_url: "https://geocoding-api.open-meteo.com/v1/search".to_string(),
            air_quality_url: "https://air-quality-api.open-meteo.com/v1/air-quality".to_string(),
        }
    }

    pub fn with_urls(geocoding_url: impl Into<String>, air_quality_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            geocoding_url: geocoding_url.into(),
            air_quality_url: air_quality_url.into(),
        }
    }
}

impl Default for OpenMeteoAirQualityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct GeocodingResponse {
    results: Option<Vec<GeocodingResult>>,
}

#[derive(Debug, Deserialize)]
struct GeocodingResult {
    name: String,
    latitude: f64,
    longitude: f64,
    country: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AirQualityResponse {
    current: Option<CurrentAirQuality>,
}

#[derive(Debug, Deserialize)]
struct CurrentAirQuality {
    us_aqi: Option<f64>,
    european_aqi: Option<f64>,
    pm2_5: Option<f64>,
    pm10: Option<f64>,
    ozone: Option<f64>,
}

#[async_trait]
impl AirQualityProvider for OpenMeteoAirQualityProvider {
    async fn geocode(&self, name: &str) -> Result<AirQualityLocation, AirQualityError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AirQualityError::InvalidQuery("empty location".to_string()));
        }
        let response = self
            .client
            .get(&self.geocoding_url)
            .query(&[("name", trimmed), ("count", "1")])
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    AirQualityError::UpstreamTimeout
                } else {
                    AirQualityError::Geocoding(err.to_string())
                }
            })?;
        if !response.status().is_success() {
            return Err(AirQualityError::Geocoding(format!(
                "status {}",
                response.status()
            )));
        }
        let payload = response
            .json::<GeocodingResponse>()
            .await
            .map_err(|err| AirQualityError::UpstreamParse(err.to_string()))?;
        let first = payload
            .results
            .and_then(|r| r.into_iter().next())
            .ok_or_else(|| AirQualityError::Geocoding("no results".to_string()))?;
        let display_name = if let Some(country) = first.country {
            format!("{}, {}", first.name, country)
        } else {
            first.name.clone()
        };
        Ok(AirQualityLocation {
            display_name,
            lat: first.latitude,
            lon: first.longitude,
        })
    }

    async fn fetch(&self, loc: &AirQualityLocation) -> Result<AirQualityResult, AirQualityError> {
        let response = self
            .client
            .get(&self.air_quality_url)
            .query(&[
                ("latitude", loc.lat.to_string()),
                ("longitude", loc.lon.to_string()),
                (
                    "current",
                    "us_aqi,european_aqi,pm2_5,pm10,ozone".to_string(),
                ),
            ])
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    AirQualityError::UpstreamTimeout
                } else {
                    AirQualityError::ProviderUnavailable(err.to_string())
                }
            })?;
        if !response.status().is_success() {
            return Err(AirQualityError::ProviderUnavailable(format!(
                "status {}",
                response.status()
            )));
        }
        let payload = response
            .json::<AirQualityResponse>()
            .await
            .map_err(|err| AirQualityError::UpstreamParse(err.to_string()))?;
        let current = payload
            .current
            .ok_or_else(|| AirQualityError::UpstreamParse("missing current".to_string()))?;
        let us_aqi = current.us_aqi.map(|v| v.round() as u16);
        let european_aqi = current.european_aqi.map(|v| v.round() as u16);
        let category = us_aqi
            .map(AqiCategory::from_us_aqi)
            .unwrap_or(AqiCategory::Unknown);
        Ok(AirQualityResult {
            location_display: loc.display_name.clone(),
            us_aqi,
            european_aqi,
            pm2_5: current.pm2_5,
            pm10: current.pm10,
            ozone: current.ozone,
            category,
        })
    }
}
