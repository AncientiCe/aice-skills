//! Weather skill types and trait.

use async_trait::async_trait;

/// Resolved user or default location (display name and coordinates).
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedLocation {
    pub display_name: String,
    pub lat: f64,
    pub lon: f64,
}

/// One day in a multi-day forecast.
#[derive(Clone, Debug, PartialEq)]
pub struct DailyForecast {
    pub date: String,
    pub temp_max_c: f64,
    pub temp_min_c: f64,
    pub weather_code: u16,
    pub description: String,
    pub precipitation_probability_pct: Option<u8>,
}

/// Severity for synthesized severe-weather alerts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertSeverity {
    Advisory,
    Watch,
    Warning,
}

impl AlertSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Advisory => "advisory",
            Self::Watch => "watch",
            Self::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SevereAlert {
    pub severity: AlertSeverity,
    pub kind: String,
    pub message: String,
}

/// Structured weather result for LLM to turn into a spoken answer.
#[derive(Clone, Debug)]
pub struct WeatherResult {
    pub location_display: String,
    pub temp_c: f64,
    pub humidity_pct: Option<u8>,
    pub weather_code: u16,
    pub description: String,
    pub daily_forecast: Vec<DailyForecast>,
    pub alerts: Vec<SevereAlert>,
}

impl WeatherResult {
    /// Format for inclusion in an LLM prompt (context for answer composition).
    pub fn to_prompt_context(&self) -> String {
        let humidity = self
            .humidity_pct
            .map(|h| format!("{}% humidity", h))
            .unwrap_or_else(|| "humidity N/A".to_string());
        let mut out = format!(
            "Location: {}. Temperature: {}°C. {}. Conditions: {}.",
            self.location_display, self.temp_c, humidity, self.description
        );
        if !self.daily_forecast.is_empty() {
            out.push_str("\nForecast:");
            for d in &self.daily_forecast {
                let pop = d
                    .precipitation_probability_pct
                    .map(|p| format!(", {p}% precip"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "\n- {}: {} — {}°C / {}°C{}",
                    d.date, d.description, d.temp_min_c, d.temp_max_c, pop
                ));
            }
        }
        if !self.alerts.is_empty() {
            out.push_str("\nAlerts:");
            for a in &self.alerts {
                out.push_str(&format!(
                    "\n- [{}] {}: {}",
                    a.severity.as_str(),
                    a.kind,
                    a.message
                ));
            }
        }
        out
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum WeatherSkillError {
    #[error("geocoding failed: {0}")]
    Geocoding(String),
    #[error("forecast request failed: {0}")]
    Forecast(String),
    #[error("no default location configured")]
    NoDefaultLocation,
}

/// Weather skill: optional location override, fallback to default (e.g. startup-resolved).
#[async_trait]
pub trait WeatherSkill: Send + Sync {
    async fn execute(
        &self,
        location: Option<&str>,
        default_location: Option<&ResolvedLocation>,
    ) -> Result<WeatherResult, WeatherSkillError>;
}
