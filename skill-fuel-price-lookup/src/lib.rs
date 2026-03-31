mod http_provider;
mod types;

pub use types::{
    FuelPriceLookupError, FuelPriceLookupQuery, FuelPriceLookupResult, FuelPriceLookupSkill,
};

use async_trait::async_trait;
use http_provider::{FuelPriceDataProvider, HttpFuelPriceDataProvider};
use metrics::{counter, histogram};
use skill_live_info_common::cache::TtlCache;
use skill_live_info_common::retry::with_retries;
use std::sync::Arc;
use std::time::{Duration, Instant};

const BACKEND_SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const BACKEND_SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";

pub struct HttpFuelPriceLookupSkill {
    provider: Arc<dyn FuelPriceDataProvider>,
    cache: TtlCache<String, FuelPriceLookupResult>,
    fresh_ttl: Duration,
    stale_ttl: Duration,
    retry_attempts: usize,
}

impl HttpFuelPriceLookupSkill {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(HttpFuelPriceDataProvider::new()),
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(15 * 60),
            stale_ttl: Duration::from_secs(60 * 60),
            retry_attempts: 2,
        }
    }

    fn cache_key(query: &FuelPriceLookupQuery) -> String {
        format!(
            "{}|{}|{}",
            query.country_code.trim().to_uppercase(),
            query.region.clone().unwrap_or_default().to_uppercase(),
            query.fuel_type.clone().unwrap_or_default().to_uppercase()
        )
    }

    fn error_kind(error: &FuelPriceLookupError) -> &'static str {
        match error {
            FuelPriceLookupError::InvalidCountry(_) => "invalid_country",
            FuelPriceLookupError::UnsupportedCountry(_) => "unsupported_country",
            FuelPriceLookupError::MissingApiKey => "missing_api_key",
            FuelPriceLookupError::ProviderUnavailable(_) => "provider_unavailable",
            FuelPriceLookupError::UpstreamTimeout => "upstream_timeout",
            FuelPriceLookupError::UpstreamParse(_) => "upstream_parse",
        }
    }

    #[cfg(test)]
    fn new_for_tests(provider: Arc<dyn FuelPriceDataProvider>) -> Self {
        Self {
            provider,
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_millis(1),
            stale_ttl: Duration::from_secs(2),
            retry_attempts: 1,
        }
    }
}

impl Default for HttpFuelPriceLookupSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FuelPriceLookupSkill for HttpFuelPriceLookupSkill {
    async fn execute(
        &self,
        query: &FuelPriceLookupQuery,
    ) -> Result<FuelPriceLookupResult, FuelPriceLookupError> {
        let started_at = Instant::now();
        let key = Self::cache_key(query);

        let result = async {
            if let Some(cached) = self.cache.get_fresh(&key) {
                return Ok(cached.value);
            }

            let fetched = with_retries(self.retry_attempts, || self.provider.fetch(query)).await;
            match fetched {
                Ok(mut value) => {
                    let cached = self.cache.insert(
                        key.clone(),
                        value.clone(),
                        self.fresh_ttl,
                        self.stale_ttl,
                    );
                    value.as_of = cached.as_of;
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
            BACKEND_SKILL_EXECUTE_TOTAL,
            1,
            "skill" => "fuel_price_lookup",
            "result" => result_label,
            "error_kind" => error_kind
        );
        histogram!(
            BACKEND_SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => "fuel_price_lookup"
        );

        result
    }
}

pub struct MockFuelPriceLookupSkill {
    pub result: Result<FuelPriceLookupResult, FuelPriceLookupError>,
}

impl MockFuelPriceLookupSkill {
    pub fn ok(result: FuelPriceLookupResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: FuelPriceLookupError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl FuelPriceLookupSkill for MockFuelPriceLookupSkill {
    async fn execute(
        &self,
        _query: &FuelPriceLookupQuery,
    ) -> Result<FuelPriceLookupResult, FuelPriceLookupError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::time::SystemTime;

    struct StubProvider {
        responses: Mutex<VecDeque<Result<FuelPriceLookupResult, FuelPriceLookupError>>>,
    }

    impl StubProvider {
        fn with_responses(
            responses: Vec<Result<FuelPriceLookupResult, FuelPriceLookupError>>,
        ) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    #[async_trait]
    impl FuelPriceDataProvider for StubProvider {
        async fn fetch(
            &self,
            _query: &FuelPriceLookupQuery,
        ) -> Result<FuelPriceLookupResult, FuelPriceLookupError> {
            let mut guard = self.responses.lock().expect("lock");
            guard.pop_front().unwrap_or_else(|| {
                Err(FuelPriceLookupError::ProviderUnavailable(
                    "empty".to_string(),
                ))
            })
        }
    }

    fn query() -> FuelPriceLookupQuery {
        FuelPriceLookupQuery {
            country_code: "GB".to_string(),
            region: None,
            fuel_type: Some("E10".to_string()),
        }
    }

    fn result() -> FuelPriceLookupResult {
        FuelPriceLookupResult {
            country_code: "GB".to_string(),
            region: None,
            fuel_type: "E10".to_string(),
            price: 1.54,
            currency: "GBP".to_string(),
            unit: "liter".to_string(),
            source_granularity: "forecourt-average".to_string(),
            as_of: SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn execute_success() {
        let provider = Arc::new(StubProvider::with_responses(vec![Ok(result())]));
        let skill = HttpFuelPriceLookupSkill::new_for_tests(provider);

        let value = skill.execute(&query()).await.expect("ok");

        assert_eq!(value.country_code, "GB");
    }

    #[tokio::test]
    async fn execute_no_results_supported() {
        let provider = Arc::new(StubProvider::with_responses(vec![Err(
            FuelPriceLookupError::ProviderUnavailable("no prices available".to_string()),
        )]));
        let skill = HttpFuelPriceLookupSkill::new_for_tests(provider);

        let value = skill.execute(&query()).await;

        assert!(matches!(
            value,
            Err(FuelPriceLookupError::ProviderUnavailable(message)) if message == "no prices available"
        ));
    }

    #[tokio::test]
    async fn execute_upstream_error() {
        let provider = Arc::new(StubProvider::with_responses(vec![Err(
            FuelPriceLookupError::ProviderUnavailable("down".to_string()),
        )]));
        let skill = HttpFuelPriceLookupSkill::new_for_tests(provider);

        let value = skill.execute(&query()).await;

        assert!(matches!(
            value,
            Err(FuelPriceLookupError::ProviderUnavailable(message)) if message == "down"
        ));
    }

    #[tokio::test]
    async fn execute_stale_fallback() {
        let provider = Arc::new(StubProvider::with_responses(vec![
            Ok(result()),
            Err(FuelPriceLookupError::ProviderUnavailable(
                "down".to_string(),
            )),
        ]));
        let skill = HttpFuelPriceLookupSkill::new_for_tests(provider);

        let _first = skill.execute(&query()).await.expect("first");

        tokio::time::sleep(Duration::from_millis(5)).await;

        let value = skill.execute(&query()).await.expect("fallback");
        assert_eq!(value.fuel_type, "E10");
    }
}
