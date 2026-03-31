use crate::types::{HoroscopeDailyError, HoroscopeDailyQuery, HoroscopeDailyResult};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use serde::Deserialize;
use std::time::{Duration, SystemTime};

#[async_trait]
pub trait HoroscopeDataProvider: Send + Sync {
    async fn fetch(
        &self,
        query: &HoroscopeDailyQuery,
    ) -> Result<HoroscopeDailyResult, HoroscopeDailyError>;
}

#[derive(Clone)]
pub struct HttpHoroscopeDataProvider {
    client: reqwest::Client,
    base_url: String,
}

impl HttpHoroscopeDataProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://aztro.sameerkumar.website".to_string(),
        }
    }
    fn normalize_sign(sign: &str) -> Result<String, HoroscopeDailyError> {
        let normalized = sign.trim().to_lowercase();
        let valid = [
            "aries",
            "taurus",
            "gemini",
            "cancer",
            "leo",
            "virgo",
            "libra",
            "scorpio",
            "sagittarius",
            "capricorn",
            "aquarius",
            "pisces",
        ];
        if valid.contains(&normalized.as_str()) {
            Ok(normalized)
        } else {
            Err(HoroscopeDailyError::InvalidSign(sign.to_string()))
        }
    }
    fn day_for_date(date: Option<chrono::NaiveDate>) -> Result<String, HoroscopeDailyError> {
        let now = Utc::now().date_naive();
        match date {
            None => Ok("today".to_string()),
            Some(value) if value == now => Ok("today".to_string()),
            Some(value) if value == now - ChronoDuration::days(1) => Ok("yesterday".to_string()),
            Some(value) if value == now + ChronoDuration::days(1) => Ok("tomorrow".to_string()),
            Some(value) => Err(HoroscopeDailyError::UnsupportedDate(value.to_string())),
        }
    }
}
impl Default for HttpHoroscopeDataProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct AztroResponse {
    description: String,
    mood: Option<String>,
    color: Option<String>,
    lucky_number: Option<String>,
}

#[async_trait]
impl HoroscopeDataProvider for HttpHoroscopeDataProvider {
    async fn fetch(
        &self,
        query: &HoroscopeDailyQuery,
    ) -> Result<HoroscopeDailyResult, HoroscopeDailyError> {
        let sign = Self::normalize_sign(&query.sign)?;
        let day = Self::day_for_date(query.date)?;
        let response = self
            .client
            .post(&self.base_url)
            .query(&[("sign", sign.as_str()), ("day", day.as_str())])
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    HoroscopeDailyError::UpstreamTimeout
                } else {
                    HoroscopeDailyError::ProviderUnavailable(err.to_string())
                }
            })?;
        if !response.status().is_success() {
            return Err(HoroscopeDailyError::ProviderUnavailable(format!(
                "status {}",
                response.status()
            )));
        }
        let payload = response
            .json::<AztroResponse>()
            .await
            .map_err(|err| HoroscopeDailyError::UpstreamParse(err.to_string()))?;
        Ok(HoroscopeDailyResult {
            sign,
            day,
            summary: payload.description,
            mood: payload.mood,
            color: payload.color,
            lucky_number: payload.lucky_number,
            as_of: SystemTime::now(),
        })
    }
}
