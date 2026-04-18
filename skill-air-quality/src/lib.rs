//! Air quality skill backed by Open-Meteo's free air-quality API.

mod http_provider;
mod types;

pub use http_provider::{AirQualityProvider, OpenMeteoAirQualityProvider};
pub use types::{
    AirQualityError, AirQualityLocation, AirQualityResult, AirQualitySkill, AqiCategory,
};

use async_trait::async_trait;
use metrics::{counter, histogram};
use skill_live_info_common::cache::TtlCache;
use skill_live_info_common::retry::with_retries;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "air_quality";

pub struct HttpAirQualitySkill {
    provider: Arc<dyn AirQualityProvider>,
    cache: TtlCache<String, AirQualityResult>,
    fresh_ttl: Duration,
    stale_ttl: Duration,
    retry_attempts: usize,
}

impl HttpAirQualitySkill {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(OpenMeteoAirQualityProvider::new()),
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(60 * 15),
            stale_ttl: Duration::from_secs(60 * 60 * 2),
            retry_attempts: 2,
        }
    }

    pub fn with_provider(provider: Arc<dyn AirQualityProvider>) -> Self {
        Self {
            provider,
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(60 * 15),
            stale_ttl: Duration::from_secs(60 * 60 * 2),
            retry_attempts: 2,
        }
    }

    fn cache_key(loc: &AirQualityLocation) -> String {
        format!("{:.4}|{:.4}", loc.lat, loc.lon)
    }

    fn error_kind(error: &AirQualityError) -> &'static str {
        match error {
            AirQualityError::InvalidQuery(_) => "invalid_query",
            AirQualityError::Geocoding(_) => "geocoding",
            AirQualityError::ProviderUnavailable(_) => "provider_unavailable",
            AirQualityError::UpstreamTimeout => "upstream_timeout",
            AirQualityError::UpstreamParse(_) => "upstream_parse",
            AirQualityError::NoDefaultLocation => "no_default_location",
        }
    }
}

impl Default for HttpAirQualitySkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AirQualitySkill for HttpAirQualitySkill {
    async fn execute(
        &self,
        location: Option<&str>,
        default_location: Option<&AirQualityLocation>,
    ) -> Result<AirQualityResult, AirQualityError> {
        let started_at = Instant::now();
        let result = async {
            let loc = if let Some(name) = location {
                self.provider.geocode(name.trim()).await?
            } else if let Some(default) = default_location {
                default.clone()
            } else {
                return Err(AirQualityError::NoDefaultLocation);
            };
            let key = Self::cache_key(&loc);
            if let Some(cached) = self.cache.get_fresh(&key) {
                return Ok(cached.value);
            }
            let fetched = with_retries(self.retry_attempts, || self.provider.fetch(&loc)).await;
            match fetched {
                Ok(value) => {
                    self.cache
                        .insert(key.clone(), value.clone(), self.fresh_ttl, self.stale_ttl);
                    Ok(value)
                }
                Err(error) => {
                    if let Some(cached) = self.cache.get_stale_eligible(&key) {
                        Ok(cached.value)
                    } else {
                        Err(error)
                    }
                }
            }
        }
        .await;

        let mut result_label = "success";
        let mut error_kind = "none";
        if let Err(error) = &result {
            result_label = "error";
            error_kind = Self::error_kind(error);
        }
        counter!(
            SKILL_EXECUTE_TOTAL,
            1,
            "skill" => SKILL_LABEL,
            "result" => result_label,
            "error_kind" => error_kind
        );
        histogram!(
            SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => SKILL_LABEL
        );
        result
    }
}

pub struct MockAirQualitySkill {
    pub result: Result<AirQualityResult, AirQualityError>,
}

impl MockAirQualitySkill {
    pub fn ok(result: AirQualityResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: AirQualityError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl AirQualitySkill for MockAirQualitySkill {
    async fn execute(
        &self,
        _location: Option<&str>,
        _default_location: Option<&AirQualityLocation>,
    ) -> Result<AirQualityResult, AirQualityError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct StubProvider {
        location: AirQualityLocation,
        responses: Mutex<std::collections::VecDeque<Result<AirQualityResult, AirQualityError>>>,
    }

    #[async_trait]
    impl AirQualityProvider for StubProvider {
        async fn geocode(&self, _name: &str) -> Result<AirQualityLocation, AirQualityError> {
            Ok(self.location.clone())
        }
        async fn fetch(
            &self,
            _loc: &AirQualityLocation,
        ) -> Result<AirQualityResult, AirQualityError> {
            let mut guard = self.responses.lock().expect("lock");
            guard
                .pop_front()
                .unwrap_or_else(|| Err(AirQualityError::ProviderUnavailable("empty".into())))
        }
    }

    fn loc() -> AirQualityLocation {
        AirQualityLocation {
            display_name: "Brussels, Belgium".to_string(),
            lat: 50.85,
            lon: 4.35,
        }
    }

    fn ok_result() -> AirQualityResult {
        AirQualityResult {
            location_display: "Brussels, Belgium".to_string(),
            us_aqi: Some(42),
            european_aqi: Some(30),
            pm2_5: Some(8.0),
            pm10: Some(15.0),
            ozone: Some(60.0),
            category: AqiCategory::Good,
        }
    }

    #[tokio::test]
    async fn execute_with_default_location() {
        let provider = Arc::new(StubProvider {
            location: loc(),
            responses: Mutex::new([Ok(ok_result())].into_iter().collect()),
        });
        let skill = HttpAirQualitySkill::with_provider(provider);
        let result = skill.execute(None, Some(&loc())).await.expect("ok");
        assert_eq!(result.us_aqi, Some(42));
        assert_eq!(result.category, AqiCategory::Good);
    }

    #[tokio::test]
    async fn execute_with_named_location() {
        let provider = Arc::new(StubProvider {
            location: loc(),
            responses: Mutex::new([Ok(ok_result())].into_iter().collect()),
        });
        let skill = HttpAirQualitySkill::with_provider(provider);
        let result = skill.execute(Some("Brussels"), None).await.expect("ok");
        assert_eq!(result.location_display, "Brussels, Belgium");
    }

    #[tokio::test]
    async fn execute_no_default_returns_error() {
        let provider = Arc::new(StubProvider {
            location: loc(),
            responses: Mutex::new(std::collections::VecDeque::new()),
        });
        let skill = HttpAirQualitySkill::with_provider(provider);
        let err = skill.execute(None, None).await.expect_err("err");
        assert!(matches!(err, AirQualityError::NoDefaultLocation));
    }

    #[test]
    fn aqi_category_mapping() {
        assert_eq!(AqiCategory::from_us_aqi(0), AqiCategory::Good);
        assert_eq!(AqiCategory::from_us_aqi(75), AqiCategory::Moderate);
        assert_eq!(
            AqiCategory::from_us_aqi(120),
            AqiCategory::UnhealthyForSensitive
        );
        assert_eq!(AqiCategory::from_us_aqi(180), AqiCategory::Unhealthy);
        assert_eq!(AqiCategory::from_us_aqi(250), AqiCategory::VeryUnhealthy);
        assert_eq!(AqiCategory::from_us_aqi(400), AqiCategory::Hazardous);
    }
}
