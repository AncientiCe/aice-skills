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
            // aztro.sameerkumar.website is gone (404 for every sign).
            base_url: "https://freehoroscopeapi.com".to_string(),
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
struct DailyResponse {
    data: DailyData,
}

#[derive(Debug, Deserialize)]
struct DailyData {
    horoscope: String,
}

/// Daily horoscope URL on freehoroscopeapi.com; `day` is today, tomorrow or yesterday.
fn daily_request_url(base_url: &str, sign: &str, day: &str) -> String {
    format!(
        "{}/api/v1/get-horoscope/daily?sign={}&day={}",
        base_url.trim_end_matches('/'),
        sign,
        day.to_uppercase()
    )
}

/// The horoscope text of a daily answer; an empty text is a parse error.
fn parse_daily_response(body: &str) -> Result<String, HoroscopeDailyError> {
    let payload: DailyResponse = serde_json::from_str(body)
        .map_err(|err| HoroscopeDailyError::UpstreamParse(err.to_string()))?;
    let text = payload.data.horoscope.trim();
    if text.is_empty() {
        return Err(HoroscopeDailyError::UpstreamParse(
            "empty horoscope".to_string(),
        ));
    }
    Ok(text.to_string())
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
            .get(daily_request_url(&self.base_url, &sign, &day))
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
        let body = response
            .text()
            .await
            .map_err(|err| HoroscopeDailyError::UpstreamParse(err.to_string()))?;
        let summary = parse_daily_response(&body)?;
        // The provider gives only the text: no mood, colour or lucky number.
        Ok(HoroscopeDailyResult {
            sign,
            day,
            summary,
            mood: None,
            color: None,
            lucky_number: None,
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
    fn asks_the_provider_for_the_sign_and_day() {
        let url = daily_request_url("https://freehoroscopeapi.com", "aries", "today");
        assert_eq!(
            url,
            "https://freehoroscopeapi.com/api/v1/get-horoscope/daily?sign=aries&day=TODAY"
        );
    }

    #[test]
    fn reads_the_horoscope_text_from_the_provider_answer() {
        let body = r#"{"data":{"date":"2026-09-30","period":"daily","sign":"Aries","horoscope":"A quiet day for reading."}}"#;
        let text = parse_daily_response(body).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(text, "A quiet day for reading.");
    }

    #[test]
    fn an_answer_without_text_is_a_parse_error() {
        assert!(parse_daily_response(r#"{"data":{"horoscope":"  "}}"#).is_err());
        assert!(parse_daily_response("not json").is_err());
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
