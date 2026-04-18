use async_trait::async_trait;

#[derive(Clone, Debug, PartialEq)]
pub struct AirQualityLocation {
    pub display_name: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Debug)]
pub struct AirQualityResult {
    pub location_display: String,
    pub us_aqi: Option<u16>,
    pub european_aqi: Option<u16>,
    pub pm2_5: Option<f64>,
    pub pm10: Option<f64>,
    pub ozone: Option<f64>,
    pub category: AqiCategory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AqiCategory {
    Good,
    Moderate,
    UnhealthyForSensitive,
    Unhealthy,
    VeryUnhealthy,
    Hazardous,
    Unknown,
}

impl AqiCategory {
    pub fn label(&self) -> &'static str {
        match self {
            AqiCategory::Good => "good",
            AqiCategory::Moderate => "moderate",
            AqiCategory::UnhealthyForSensitive => "unhealthy for sensitive groups",
            AqiCategory::Unhealthy => "unhealthy",
            AqiCategory::VeryUnhealthy => "very unhealthy",
            AqiCategory::Hazardous => "hazardous",
            AqiCategory::Unknown => "unknown",
        }
    }

    /// Map a US-EPA AQI value to its category, mirroring AirNow.
    pub fn from_us_aqi(value: u16) -> Self {
        match value {
            0..=50 => AqiCategory::Good,
            51..=100 => AqiCategory::Moderate,
            101..=150 => AqiCategory::UnhealthyForSensitive,
            151..=200 => AqiCategory::Unhealthy,
            201..=300 => AqiCategory::VeryUnhealthy,
            _ => AqiCategory::Hazardous,
        }
    }
}

impl AirQualityResult {
    pub fn to_prompt_context(&self) -> String {
        let aqi = self
            .us_aqi
            .map(|v| format!("US AQI {v}"))
            .or_else(|| self.european_aqi.map(|v| format!("EU AQI {v}")))
            .unwrap_or_else(|| "AQI N/A".to_string());
        let pm = self
            .pm2_5
            .map(|v| format!("PM2.5 {v:.1}"))
            .unwrap_or_else(|| "PM2.5 N/A".to_string());
        format!(
            "Location: {}. {} ({}). {}.",
            self.location_display,
            aqi,
            self.category.label(),
            pm
        )
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum AirQualityError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("geocoding failed: {0}")]
    Geocoding(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("provider timeout")]
    UpstreamTimeout,
    #[error("provider response parse failed: {0}")]
    UpstreamParse(String),
    #[error("no default location configured")]
    NoDefaultLocation,
}

#[async_trait]
pub trait AirQualitySkill: Send + Sync {
    async fn execute(
        &self,
        location: Option<&str>,
        default_location: Option<&AirQualityLocation>,
    ) -> Result<AirQualityResult, AirQualityError>;
}
