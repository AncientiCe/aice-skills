use crate::types::{HolidayLookupError, HolidayLookupResult, HolidayMatch, HolidayQuery};
use async_trait::async_trait;
use chrono::{Datelike, NaiveDate, Utc};
use reqwest::StatusCode;
use serde::Deserialize;
use std::time::{Duration, SystemTime};

#[async_trait]
pub trait HolidayDataProvider: Send + Sync {
    async fn fetch(&self, query: &HolidayQuery) -> Result<HolidayLookupResult, HolidayLookupError>;
}

#[derive(Clone)]
pub struct HttpHolidayDataProvider {
    client: reqwest::Client,
    base_url: String,
}

impl HttpHolidayDataProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://date.nager.at".to_string(),
        }
    }
    fn validate_country(country: &str) -> Result<String, HolidayLookupError> {
        let normalized = country.trim().to_uppercase();
        if normalized.len() != 2 || !normalized.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(HolidayLookupError::InvalidCountry(country.to_string()));
        }
        Ok(normalized)
    }
    fn current_year() -> i32 {
        Utc::now().year()
    }
}
impl Default for HttpHolidayDataProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct NagerHoliday {
    #[serde(rename = "date")]
    date: String,
    #[serde(rename = "localName")]
    local_name: String,
    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "global")]
    global: bool,
    #[serde(rename = "counties")]
    counties: Option<Vec<String>>,
}

#[async_trait]
impl HolidayDataProvider for HttpHolidayDataProvider {
    async fn fetch(&self, query: &HolidayQuery) -> Result<HolidayLookupResult, HolidayLookupError> {
        let country = Self::validate_country(&query.country_code)?;
        let year = query.year.unwrap_or_else(Self::current_year);
        let url = format!(
            "{}/api/v3/PublicHolidays/{}/{}",
            self.base_url, year, country
        );
        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    HolidayLookupError::UpstreamTimeout
                } else {
                    HolidayLookupError::ProviderUnavailable(err.to_string())
                }
            })?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(HolidayLookupResult {
                country_code: country,
                region_code: query.region_code.clone(),
                matches: Vec::new(),
                as_of: SystemTime::now(),
            });
        }
        if !response.status().is_success() {
            return Err(HolidayLookupError::ProviderUnavailable(format!(
                "status {}",
                response.status()
            )));
        }
        let holidays = response
            .json::<Vec<NagerHoliday>>()
            .await
            .map_err(|err| HolidayLookupError::UpstreamParse(err.to_string()))?;
        let name_filter = query.holiday_name.as_ref().map(|x| x.to_lowercase());
        let date_filter = query.date;
        let mut matches = Vec::new();
        for item in holidays {
            let item_date = NaiveDate::parse_from_str(&item.date, "%Y-%m-%d")
                .map_err(|err| HolidayLookupError::UpstreamParse(err.to_string()))?;
            if let Some(filter) = &name_filter {
                let item_name = item.name.to_lowercase();
                let local_name = item.local_name.to_lowercase();
                if !item_name.contains(filter) && !local_name.contains(filter) {
                    continue;
                }
            }
            if let Some(filter_date) = date_filter {
                if filter_date != item_date {
                    continue;
                }
            }
            let region_applies = match (&query.region_code, &item.counties) {
                (Some(region), Some(counties)) => counties
                    .iter()
                    .any(|entry| entry.eq_ignore_ascii_case(region)),
                (Some(_), None) => item.global,
                (None, _) => true,
            };
            if !region_applies {
                continue;
            }
            matches.push(HolidayMatch {
                name: item.name,
                date: item_date,
                is_public_holiday: true,
                local_name: Some(item.local_name),
                observed_date: None,
            });
        }
        Ok(HolidayLookupResult {
            country_code: country,
            region_code: query.region_code.clone(),
            matches,
            as_of: SystemTime::now(),
        })
    }
}
