//! Open-Meteo API: geocoding and forecast (no API key).

use crate::types::{
    AlertSeverity, DailyForecast, ResolvedLocation, SevereAlert, WeatherResult, WeatherSkill,
    WeatherSkillError,
};
use async_trait::async_trait;
use metrics::{counter, histogram};
use serde::Deserialize;
use std::collections::HashSet;
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
    current: Option<CurrentWeather>,
    daily: Option<DailyBlock>,
}

#[derive(Deserialize)]
struct CurrentWeather {
    temperature_2m: f64,
    relative_humidity_2m: Option<f64>,
    weather_code: u16,
    wind_speed_10m: Option<f64>,
}

#[derive(Deserialize, Default)]
struct DailyBlock {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    temperature_2m_max: Vec<f64>,
    #[serde(default)]
    temperature_2m_min: Vec<f64>,
    #[serde(default)]
    weather_code: Vec<u16>,
    #[serde(default)]
    precipitation_probability_max: Vec<Option<f64>>,
    #[serde(default)]
    wind_gusts_10m_max: Vec<Option<f64>>,
}

/// Open-Meteo weather skill implementation.
pub struct OpenMeteoWeatherSkill {
    client: reqwest::Client,
}

impl OpenMeteoWeatherSkill {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Resolve a place name to coordinates (e.g. for startup default location).
    pub async fn geocode_place(&self, name: &str) -> Result<ResolvedLocation, WeatherSkillError> {
        self.geocode(name).await
    }

    async fn geocode(&self, name: &str) -> Result<ResolvedLocation, WeatherSkillError> {
        let candidates = geocode_candidates(name);
        let mut last_error: Option<WeatherSkillError> = None;
        for candidate in candidates {
            match self.geocode_once(&candidate).await {
                Ok(Some(location)) => return Ok(location),
                Ok(None) => {
                    last_error = Some(WeatherSkillError::Geocoding("no results".to_string()));
                }
                Err(err) => return Err(err),
            }
        }
        Err(last_error.unwrap_or_else(|| WeatherSkillError::Geocoding("no results".to_string())))
    }

    async fn geocode_once(
        &self,
        name: &str,
    ) -> Result<Option<ResolvedLocation>, WeatherSkillError> {
        let started_at = Instant::now();
        let res = self
            .client
            .get(GEOCODING_URL)
            .query(&[("name", name), ("count", "1")])
            .send()
            .await
            .map_err(|e| {
                record_dependency_result("geocoding", "error", Some("request"));
                record_dependency_duration("geocoding", started_at);
                WeatherSkillError::Geocoding(e.to_string())
            })?;
        if !res.status().is_success() {
            record_dependency_result("geocoding", "error", Some("http_status"));
            record_dependency_duration("geocoding", started_at);
            return Err(WeatherSkillError::Geocoding(format!(
                "status {}",
                res.status()
            )));
        }
        let body: GeocodingResponse = res.json().await.map_err(|e| {
            record_dependency_result("geocoding", "error", Some("parse"));
            record_dependency_duration("geocoding", started_at);
            WeatherSkillError::Geocoding(e.to_string())
        })?;
        let Some(first) = body.results.and_then(|r| r.into_iter().next()) else {
            record_dependency_result("geocoding", "error", Some("no_results"));
            record_dependency_duration("geocoding", started_at);
            return Ok(None);
        };
        record_dependency_result("geocoding", "success", None);
        record_dependency_duration("geocoding", started_at);
        let display_name = if let Some(ref c) = first.country {
            format!("{}, {}", first.name, c)
        } else {
            first.name.clone()
        };
        Ok(Some(ResolvedLocation {
            display_name,
            lat: first.latitude,
            lon: first.longitude,
        }))
    }

    async fn fetch_forecast(
        &self,
        loc: &ResolvedLocation,
    ) -> Result<WeatherResult, WeatherSkillError> {
        let started_at = Instant::now();
        let res = self
            .client
            .get(FORECAST_URL)
            .query(&[
                ("latitude", loc.lat.to_string()),
                ("longitude", loc.lon.to_string()),
                (
                    "current",
                    "temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m".to_string(),
                ),
                (
                    "daily",
                    "temperature_2m_max,temperature_2m_min,weather_code,\
                     precipitation_probability_max,wind_gusts_10m_max"
                        .to_string(),
                ),
                ("forecast_days", "5".to_string()),
                ("timezone", "auto".to_string()),
            ])
            .send()
            .await
            .map_err(|e| {
                record_dependency_result("forecast", "error", Some("request"));
                record_dependency_duration("forecast", started_at);
                WeatherSkillError::Forecast(e.to_string())
            })?;
        if !res.status().is_success() {
            record_dependency_result("forecast", "error", Some("http_status"));
            record_dependency_duration("forecast", started_at);
            return Err(WeatherSkillError::Forecast(format!(
                "status {}",
                res.status()
            )));
        }
        let body: ForecastResponse = res.json().await.map_err(|e| {
            record_dependency_result("forecast", "error", Some("parse"));
            record_dependency_duration("forecast", started_at);
            WeatherSkillError::Forecast(e.to_string())
        })?;
        let current = body.current.ok_or_else(|| {
            record_dependency_result("forecast", "error", Some("missing_current"));
            record_dependency_duration("forecast", started_at);
            WeatherSkillError::Forecast("missing current".to_string())
        })?;
        record_dependency_result("forecast", "success", None);
        record_dependency_duration("forecast", started_at);
        let description = weather_code_to_description(current.weather_code);
        let humidity_pct = current
            .relative_humidity_2m
            .map(|h| h.clamp(0.0, 100.0) as u8);
        let daily = body.daily.unwrap_or_default();
        let daily_forecast = build_daily_forecast(&daily);
        let alerts = synthesize_alerts(current.weather_code, current.wind_speed_10m, &daily);
        Ok(WeatherResult {
            location_display: loc.display_name.clone(),
            temp_c: current.temperature_2m,
            humidity_pct,
            weather_code: current.weather_code,
            description,
            daily_forecast,
            alerts,
        })
    }
}

fn build_daily_forecast(daily: &DailyBlock) -> Vec<DailyForecast> {
    let len = daily.time.len();
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let max = daily.temperature_2m_max.get(i).copied().unwrap_or(0.0);
        let min = daily.temperature_2m_min.get(i).copied().unwrap_or(0.0);
        let code = daily.weather_code.get(i).copied().unwrap_or(0);
        let pop = daily
            .precipitation_probability_max
            .get(i)
            .and_then(|p| *p)
            .map(|p| p.clamp(0.0, 100.0) as u8);
        out.push(DailyForecast {
            date: daily.time[i].clone(),
            temp_max_c: max,
            temp_min_c: min,
            weather_code: code,
            description: weather_code_to_description(code),
            precipitation_probability_pct: pop,
        });
    }
    out
}

/// Open-Meteo's free tier doesn't include first-party severe-weather alerts
/// for every region. We synthesize a minimal set from current + daily values
/// so users get *some* heads-up without paying for a separate alerts API.
fn synthesize_alerts(
    current_code: u16,
    current_wind_kmh: Option<f64>,
    daily: &DailyBlock,
) -> Vec<SevereAlert> {
    let mut alerts = Vec::new();
    if matches!(current_code, 95 | 96 | 99) {
        alerts.push(SevereAlert {
            severity: AlertSeverity::Warning,
            kind: "thunderstorm".to_string(),
            message: "Thunderstorm conditions reported now.".to_string(),
        });
    }
    if let Some(w) = current_wind_kmh {
        if w >= 90.0 {
            alerts.push(SevereAlert {
                severity: AlertSeverity::Warning,
                kind: "high_wind".to_string(),
                message: format!("High winds: {w:.0} km/h."),
            });
        } else if w >= 60.0 {
            alerts.push(SevereAlert {
                severity: AlertSeverity::Watch,
                kind: "wind".to_string(),
                message: format!("Strong winds: {w:.0} km/h."),
            });
        }
    }
    for (i, code) in daily.weather_code.iter().enumerate() {
        if matches!(code, 71 | 73 | 75 | 85 | 86) {
            let when = daily
                .time
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("day {i}"));
            alerts.push(SevereAlert {
                severity: AlertSeverity::Advisory,
                kind: "snow".to_string(),
                message: format!("Snow expected on {when}."),
            });
            break;
        }
    }
    for (i, gust_opt) in daily.wind_gusts_10m_max.iter().enumerate() {
        if let Some(gust) = gust_opt {
            if *gust >= 90.0 {
                let when = daily
                    .time
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("day {i}"));
                alerts.push(SevereAlert {
                    severity: AlertSeverity::Watch,
                    kind: "wind_gust".to_string(),
                    message: format!("Gusts up to {gust:.0} km/h on {when}."),
                });
                break;
            }
        }
    }
    alerts
}

fn geocode_candidates(raw: &str) -> Vec<String> {
    fn push_unique(out: &mut Vec<String>, seen: &mut HashSet<String>, candidate: String) {
        let normalized = candidate.trim();
        if normalized.is_empty() {
            return;
        }
        let key = normalized.to_lowercase();
        if seen.insert(key) {
            out.push(normalized.to_string());
        }
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let trimmed = raw.trim();
    push_unique(&mut out, &mut seen, trimmed.to_string());

    let stripped = trimmed
        .trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .trim_end_matches(|c: char| ['?', '!', '.', ',', ';', ':'].contains(&c))
        .trim();
    push_unique(&mut out, &mut seen, stripped.to_string());

    for prefix in ["weather in ", "in ", "for "] {
        if let Some(suffix) = stripped.to_lowercase().strip_prefix(prefix) {
            let offset = stripped.len().saturating_sub(suffix.len());
            let candidate = stripped[offset..].trim();
            push_unique(&mut out, &mut seen, candidate.to_string());
        }
    }

    match stripped.to_lowercase().as_str() {
        "la" | "l.a" | "l.a." => {
            push_unique(
                &mut out,
                &mut seen,
                "Los Angeles, United States".to_string(),
            );
        }
        "nyc" => {
            push_unique(&mut out, &mut seen, "New York, United States".to_string());
        }
        "sf" => {
            push_unique(
                &mut out,
                &mut seen,
                "San Francisco, United States".to_string(),
            );
        }
        _ => {}
    }

    out
}

impl Default for OpenMeteoWeatherSkill {
    fn default() -> Self {
        Self::new()
    }
}

fn weather_code_to_description(code: u16) -> String {
    // WMO weather codes (Open-Meteo)
    match code {
        0 => "Clear sky",
        1..=3 => "Partly cloudy",
        45 | 48 => "Foggy",
        51 | 53 | 55 => "Drizzle",
        61 | 63 | 65 => "Rain",
        71 | 73 | 75 => "Snow",
        80..=82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Variable",
    }
    .to_string()
}

/// When location is None we use default_location: the app's resolved location from location
/// services (IP geolocation or config at startup). Geocode is only used when the user names a place.
#[async_trait]
impl WeatherSkill for OpenMeteoWeatherSkill {
    async fn execute(
        &self,
        location: Option<&str>,
        default_location: Option<&ResolvedLocation>,
    ) -> Result<WeatherResult, WeatherSkillError> {
        let started_at = Instant::now();
        let result = async {
            let loc = if let Some(name) = location {
                self.geocode(name.trim()).await?
            } else if let Some(default) = default_location {
                default.clone()
            } else {
                return Err(WeatherSkillError::NoDefaultLocation);
            };
            self.fetch_forecast(&loc).await
        }
        .await;
        match &result {
            Ok(_) => record_backend_skill_result("success", None),
            Err(error) => record_backend_skill_result("error", Some(error_kind(error))),
        }
        record_backend_skill_duration(started_at);
        result
    }
}

fn record_backend_skill_result(result: &str, error_kind: Option<&str>) {
    counter!(
        BACKEND_SKILL_EXECUTE_TOTAL,
        1,
        "skill" => "weather",
        "result" => result.to_string(),
        "error_kind" => error_kind.unwrap_or("none").to_string()
    );
}

fn record_backend_skill_duration(started_at: Instant) {
    histogram!(
        BACKEND_SKILL_EXECUTE_DURATION_SECONDS,
        started_at.elapsed().as_secs_f64(),
        "skill" => "weather"
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

fn error_kind(error: &WeatherSkillError) -> &'static str {
    match error {
        WeatherSkillError::Geocoding(_) => "geocoding",
        WeatherSkillError::Forecast(_) => "forecast",
        WeatherSkillError::NoDefaultLocation => "no_default_location",
    }
}

#[cfg(test)]
mod tests {
    use super::{build_daily_forecast, geocode_candidates, synthesize_alerts, DailyBlock};
    use crate::types::AlertSeverity;

    #[test]
    fn geocode_candidates_strip_trailing_punctuation() {
        let cands = geocode_candidates("Los Angeles?");
        assert!(
            cands.iter().any(|x| x == "Los Angeles"),
            "expected stripped city candidate, got: {:?}",
            cands
        );
    }

    #[test]
    fn geocode_candidates_extract_place_from_phrases() {
        let cands = geocode_candidates("weather in Los Angeles?");
        assert!(
            cands.iter().any(|x| x == "Los Angeles"),
            "expected place extracted from phrase, got: {:?}",
            cands
        );
    }

    #[test]
    fn geocode_candidates_expand_la_alias() {
        let cands = geocode_candidates("LA");
        assert!(
            cands.iter().any(|x| x == "Los Angeles, United States"),
            "expected LA alias expansion, got: {:?}",
            cands
        );
    }

    #[test]
    fn build_daily_forecast_zips_arrays() {
        let daily = DailyBlock {
            time: vec!["2026-04-18".into(), "2026-04-19".into()],
            temperature_2m_max: vec![20.0, 22.0],
            temperature_2m_min: vec![10.0, 12.0],
            weather_code: vec![0, 61],
            precipitation_probability_max: vec![Some(10.0), Some(80.0)],
            wind_gusts_10m_max: vec![Some(20.0), Some(30.0)],
        };
        let out = build_daily_forecast(&daily);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].date, "2026-04-18");
        assert_eq!(out[0].description, "Clear sky");
        assert_eq!(out[1].precipitation_probability_pct, Some(80));
        assert_eq!(out[1].description, "Rain");
    }

    #[test]
    fn synthesize_alerts_flags_thunderstorm_and_high_wind() {
        let alerts = synthesize_alerts(95, Some(95.0), &DailyBlock::default());
        assert!(alerts.iter().any(|a| a.kind == "thunderstorm"));
        assert!(alerts
            .iter()
            .any(|a| a.kind == "high_wind" && a.severity == AlertSeverity::Warning));
    }

    #[test]
    fn synthesize_alerts_flags_snow_advisory() {
        let daily = DailyBlock {
            time: vec!["2026-04-18".into(), "2026-04-19".into()],
            weather_code: vec![1, 75],
            ..Default::default()
        };
        let alerts = synthesize_alerts(1, None, &daily);
        assert!(alerts
            .iter()
            .any(|a| a.kind == "snow" && a.severity == AlertSeverity::Advisory));
    }

    #[test]
    fn synthesize_alerts_quiet_when_calm() {
        let alerts = synthesize_alerts(0, Some(10.0), &DailyBlock::default());
        assert!(alerts.is_empty(), "expected no alerts, got {:?}", alerts);
    }
}
