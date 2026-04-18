//! Currency conversion skill backed by frankfurter.app.

mod http_provider;
mod types;

pub use http_provider::{CurrencyRateProvider, FrankfurterProvider};
pub use types::{CurrencyError, CurrencyQuery, CurrencyResult, CurrencySkill};

use async_trait::async_trait;
use metrics::{counter, histogram};
use skill_live_info_common::cache::TtlCache;
use skill_live_info_common::retry::with_retries;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "currency";

pub struct HttpCurrencySkill {
    provider: Arc<dyn CurrencyRateProvider>,
    cache: TtlCache<String, CurrencyResult>,
    fresh_ttl: Duration,
    stale_ttl: Duration,
    retry_attempts: usize,
}

impl HttpCurrencySkill {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(FrankfurterProvider::new()),
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(60 * 30),
            stale_ttl: Duration::from_secs(60 * 60 * 24),
            retry_attempts: 2,
        }
    }

    pub fn with_provider(provider: Arc<dyn CurrencyRateProvider>) -> Self {
        Self {
            provider,
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(60 * 30),
            stale_ttl: Duration::from_secs(60 * 60 * 24),
            retry_attempts: 2,
        }
    }

    fn cache_key(query: &CurrencyQuery) -> String {
        format!(
            "{}|{}|{}",
            query.from_currency.trim().to_uppercase(),
            query.to_currency.trim().to_uppercase(),
            query.amount
        )
    }

    fn error_kind(error: &CurrencyError) -> &'static str {
        match error {
            CurrencyError::InvalidQuery(_) => "invalid_query",
            CurrencyError::UnsupportedCurrency(_) => "unsupported_currency",
            CurrencyError::ProviderUnavailable(_) => "provider_unavailable",
            CurrencyError::UpstreamTimeout => "upstream_timeout",
            CurrencyError::UpstreamParse(_) => "upstream_parse",
        }
    }

    #[cfg(test)]
    fn new_for_tests(provider: Arc<dyn CurrencyRateProvider>) -> Self {
        Self {
            provider,
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_millis(1),
            stale_ttl: Duration::from_secs(2),
            retry_attempts: 1,
        }
    }
}

impl Default for HttpCurrencySkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CurrencySkill for HttpCurrencySkill {
    async fn execute(&self, query: &CurrencyQuery) -> Result<CurrencyResult, CurrencyError> {
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

pub struct MockCurrencySkill {
    pub result: Result<CurrencyResult, CurrencyError>,
}

impl MockCurrencySkill {
    pub fn ok(result: CurrencyResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: CurrencyError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl CurrencySkill for MockCurrencySkill {
    async fn execute(&self, _query: &CurrencyQuery) -> Result<CurrencyResult, CurrencyError> {
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
        responses: Mutex<VecDeque<Result<CurrencyResult, CurrencyError>>>,
    }

    impl StubProvider {
        fn with_responses(responses: Vec<Result<CurrencyResult, CurrencyError>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    #[async_trait]
    impl CurrencyRateProvider for StubProvider {
        async fn fetch(&self, _query: &CurrencyQuery) -> Result<CurrencyResult, CurrencyError> {
            let mut guard = self.responses.lock().expect("lock");
            guard
                .pop_front()
                .unwrap_or_else(|| Err(CurrencyError::ProviderUnavailable("empty".into())))
        }
    }

    fn query() -> CurrencyQuery {
        CurrencyQuery {
            amount: 50.0,
            from_currency: "EUR".into(),
            to_currency: "USD".into(),
        }
    }

    fn ok_result() -> CurrencyResult {
        CurrencyResult {
            amount: 50.0,
            from_currency: "EUR".into(),
            to_currency: "USD".into(),
            converted_amount: 54.5,
            rate: 1.09,
            rate_date: "2026-04-18".into(),
            as_of: SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn execute_success() {
        let provider = Arc::new(StubProvider::with_responses(vec![Ok(ok_result())]));
        let skill = HttpCurrencySkill::new_for_tests(provider);
        let value = skill.execute(&query()).await.expect("ok");
        assert_eq!(value.from_currency, "EUR");
        assert_eq!(value.to_currency, "USD");
        assert!((value.converted_amount - 54.5).abs() < 1e-6);
    }

    #[tokio::test]
    async fn execute_propagates_unsupported_currency() {
        let provider = Arc::new(StubProvider::with_responses(vec![Err(
            CurrencyError::UnsupportedCurrency("EUR->ZZZ".into()),
        )]));
        let skill = HttpCurrencySkill::new_for_tests(provider);
        let err = skill.execute(&query()).await.expect_err("err");
        assert!(matches!(err, CurrencyError::UnsupportedCurrency(_)));
    }

    #[tokio::test]
    async fn execute_falls_back_to_stale_cache() {
        let provider = Arc::new(StubProvider::with_responses(vec![
            Ok(ok_result()),
            Err(CurrencyError::ProviderUnavailable("down".into())),
        ]));
        let skill = HttpCurrencySkill::new_for_tests(provider);
        let _first = skill.execute(&query()).await.expect("first");
        tokio::time::sleep(Duration::from_millis(5)).await;
        let value = skill.execute(&query()).await.expect("fallback");
        assert!((value.converted_amount - 54.5).abs() < 1e-6);
    }
}
