use crate::types::{HoroscopeDailyError, HoroscopeDailyQuery, HoroscopeDailyResult};
use async_trait::async_trait;
use jiff::{civil::Date, tz::TimeZone, Timestamp};
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
    fn day_for_date(date: Option<Date>) -> Result<String, HoroscopeDailyError> {
        let now = Timestamp::now().to_zoned_datetime(TimeZone::UTC).date();
        match date {
            None => Ok("today".to_string()),
            Some(value) if value == now => Ok("today".to_string()),
            Some(value) if now.yesterday().ok() == Some(value) => Ok("yesterday".to_string()),
            Some(value) if now.tomorrow().ok() == Some(value) => Ok("tomorrow".to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> Date {
        Timestamp::now().to_zoned_datetime(TimeZone::UTC).date()
    }

    #[test]
    fn day_for_date_maps_neighbouring_utc_days() {
        let today = today();
        assert_eq!(
            HttpHoroscopeDataProvider::day_for_date(None).expect("day"),
            "today"
        );
        assert_eq!(
            HttpHoroscopeDataProvider::day_for_date(today.yesterday().ok()).expect("day"),
            "yesterday"
        );
        assert_eq!(
            HttpHoroscopeDataProvider::day_for_date(today.tomorrow().ok()).expect("day"),
            "tomorrow"
        );
    }

    #[test]
    fn day_for_date_rejects_other_days_with_iso_text() {
        let far = Date::new(2001, 2, 3).expect("date");
        let err = HttpHoroscopeDataProvider::day_for_date(Some(far)).expect_err("unsupported");
        assert!(matches!(err, HoroscopeDailyError::UnsupportedDate(ref d) if d == "2001-02-03"));
    }
}
