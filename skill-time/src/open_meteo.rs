//! Time skill: geocoding via Open-Meteo, then IANA timezone from forecast (timezone=auto).
//! We ignore the API's "time" value and compute local time ourselves with jiff and its bundled
//! tz database from the system clock, so the time is always correct for the location.

use crate::types::{TimeResult, TimeSkill, TimeSkillError};
use async_trait::async_trait;
use jiff::{tz::TimeZone, Timestamp, ZonedDateTime};
use metrics::{counter, histogram};
use serde::Deserialize;
use skill_weather::ResolvedLocation;
use std::time::Instant;

const GEOCODING_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";
const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
const BACKEND_SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const BACKEND_SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const BACKEND_DEPENDENCY_REQUESTS_TOTAL: &str = "backend_dependency_requests_total";
const BACKEND_DEPENDENCY_REQUEST_DURATION_SECONDS: &str =
    "backend_dependency_request_duration_seconds";

#[derive(Deserialize)]
struct GeocodingResponse {
    results: Option<Vec<GeocodingResult>>,
}

#[derive(Deserialize)]
struct GeocodingResult {
    name: String,
    latitude: f64,
    longitude: f64,
    country: Option<String>,
}

#[derive(Deserialize)]
struct ForecastResponse {
    timezone: Option<String>,
    timezone_abbreviation: Option<String>,
}

/// Parse IANA timezone (e.g. "Europe/London") and return the given instant in that zone.
/// Falls back to UTC if the string is not a valid IANA name.
fn zoned_at(tz_name: &str, now: Timestamp) -> ZonedDateTime {
    let tz = TimeZone::get(tz_name).unwrap_or(TimeZone::UTC);
    now.to_zoned_datetime(tz)
}

/// Time skill using Open-Meteo (geocoding + timezone lookup), time from system clock.
pub struct OpenMeteoTimeSkill {
    client: reqwest::Client,
}

impl OpenMeteoTimeSkill {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    fn record_backend_skill_result(result: &str, error_kind: Option<&str>) {
        counter!(
            BACKEND_SKILL_EXECUTE_TOTAL,
            1,
            "skill" => "time",
            "result" => result.to_string(),
            "error_kind" => error_kind.unwrap_or("none").to_string()
        );
    }

    fn record_backend_skill_duration(started_at: Instant) {
        histogram!(
            BACKEND_SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => "time"
        );
    }

    fn record_dependency_result(operation: &str, result: &str, error_kind: Option<&str>) {
        counter!(
            BACKEND_DEPENDENCY_REQUESTS_TOTAL,
            1,
            "dependency" => "open_meteo",
            "operation" => operation.to_string(),
            "result" => result.to_string(),
            "error_kind" => error_kind.unwrap_or("none").to_string()
        );
    }

    fn record_dependency_duration(operation: &str, started_at: Instant) {
        histogram!(
            BACKEND_DEPENDENCY_REQUEST_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "dependency" => "open_meteo",
            "operation" => operation.to_string()
        );
    }

    fn error_kind(error: &TimeSkillError) -> &'static str {
        match error {
            TimeSkillError::Geocoding(_) => "geocoding",
            TimeSkillError::TimeRequest(_) => "time_request",
            TimeSkillError::NoDefaultLocation => "no_default_location",
        }
    }

    async fn geocode(&self, name: &str) -> Result<ResolvedLocation, TimeSkillError> {
        let started_at = Instant::now();
        let res = self
            .client
            .get(GEOCODING_URL)
            .query(&[("name", name), ("count", "1")])
            .send()
            .await
            .map_err(|e| {
                Self::record_dependency_result("geocoding", "error", Some("request"));
                Self::record_dependency_duration("geocoding", started_at);
                TimeSkillError::Geocoding(e.to_string())
            })?;
        if !res.status().is_success() {
            Self::record_dependency_result("geocoding", "error", Some("http_status"));
            Self::record_dependency_duration("geocoding", started_at);
            return Err(TimeSkillError::Geocoding(format!(
                "status {}",
                res.status()
            )));
        }
        let body: GeocodingResponse = res.json().await.map_err(|e| {
            Self::record_dependency_result("geocoding", "error", Some("parse"));
            Self::record_dependency_duration("geocoding", started_at);
            TimeSkillError::Geocoding(e.to_string())
        })?;
        let first = body
            .results
            .and_then(|r| r.into_iter().next())
            .ok_or_else(|| {
                Self::record_dependency_result("geocoding", "error", Some("no_results"));
                Self::record_dependency_duration("geocoding", started_at);
                TimeSkillError::Geocoding("no results".to_string())
            })?;
        Self::record_dependency_result("geocoding", "success", None);
        Self::record_dependency_duration("geocoding", started_at);
        let display_name = if let Some(ref c) = first.country {
            format!("{}, {}", first.name, c)
        } else {
            first.name.clone()
        };
        Ok(ResolvedLocation {
            display_name,
            lat: first.latitude,
            lon: first.longitude,
        })
    }

    async fn fetch_time(&self, loc: &ResolvedLocation) -> Result<TimeResult, TimeSkillError> {
        let started_at = Instant::now();
        let res = self
            .client
            .get(FORECAST_URL)
            .query(&[
                ("latitude", loc.lat.to_string()),
                ("longitude", loc.lon.to_string()),
                ("timezone", "auto".to_string()),
            ])
            .send()
            .await
            .map_err(|e| {
                Self::record_dependency_result("time_lookup", "error", Some("request"));
                Self::record_dependency_duration("time_lookup", started_at);
                TimeSkillError::TimeRequest(e.to_string())
            })?;
        if !res.status().is_success() {
            Self::record_dependency_result("time_lookup", "error", Some("http_status"));
            Self::record_dependency_duration("time_lookup", started_at);
            return Err(TimeSkillError::TimeRequest(format!(
                "status {}",
                res.status()
            )));
        }
        let body: ForecastResponse = res.json().await.map_err(|e| {
            Self::record_dependency_result("time_lookup", "error", Some("parse"));
            Self::record_dependency_duration("time_lookup", started_at);
            TimeSkillError::TimeRequest(e.to_string())
        })?;
        Self::record_dependency_result("time_lookup", "success", None);
        Self::record_dependency_duration("time_lookup", started_at);
        let tz_name = body.timezone.as_deref().unwrap_or("UTC");
        let timezone_abbr = body
            .timezone_abbreviation
            .clone()
            .unwrap_or_else(|| tz_name.to_string());
        let local_dt = zoned_at(tz_name, Timestamp::now());
        let local_time = local_dt.strftime("%H:%M").to_string();
        Ok(TimeResult {
            location_display: loc.display_name.clone(),
            local_time,
            timezone: timezone_abbr,
        })
    }
}

impl Default for OpenMeteoTimeSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TimeSkill for OpenMeteoTimeSkill {
    async fn execute(
        &self,
        location: Option<&str>,
        default_location: Option<&ResolvedLocation>,
    ) -> Result<TimeResult, TimeSkillError> {
        let started_at = Instant::now();
        let result = async {
            let loc = if let Some(name) = location {
                self.geocode(name.trim()).await?
            } else if let Some(default) = default_location {
                default.clone()
            } else {
                return Err(TimeSkillError::NoDefaultLocation);
            };
            self.fetch_time(&loc).await
        }
        .await;
        match &result {
            Ok(_) => Self::record_backend_skill_result("success", None),
            Err(error) => Self::record_backend_skill_result("error", Some(Self::error_kind(error))),
        }
        Self::record_backend_skill_duration(started_at);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Timestamp {
        s.parse().expect("timestamp")
    }

    fn hhmm(tz: &str, instant: &str) -> String {
        zoned_at(tz, at(instant)).strftime("%H:%M").to_string()
    }

    #[test]
    fn named_zone_applies_its_offset_and_daylight_saving() {
        assert_eq!(hhmm("Europe/London", "2026-01-15T12:00:00Z"), "12:00");
        assert_eq!(hhmm("Europe/London", "2026-07-15T12:00:00Z"), "13:00");
        assert_eq!(hhmm("Asia/Kolkata", "2026-01-15T12:00:00Z"), "17:30");
        assert_eq!(hhmm("America/New_York", "2026-01-15T12:00:00Z"), "07:00");
    }

    #[test]
    fn unknown_zone_falls_back_to_utc() {
        assert_eq!(hhmm("Not/AZone", "2026-07-15T12:34:00Z"), "12:34");
        assert_eq!(hhmm("", "2026-07-15T12:34:00Z"), "12:34");
    }

    #[test]
    fn utc_and_gmt_names_resolve() {
        assert_eq!(hhmm("UTC", "2026-07-15T12:34:00Z"), "12:34");
        assert_eq!(hhmm("GMT", "2026-07-15T12:34:00Z"), "12:34");
    }
}
