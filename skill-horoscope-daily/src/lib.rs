mod http_provider;
mod types;

pub use types::{
    HoroscopeDailyError, HoroscopeDailyQuery, HoroscopeDailyResult, HoroscopeDailySkill,
};

use async_trait::async_trait;
use http_provider::{HoroscopeDataProvider, HttpHoroscopeDataProvider};
use metrics::{counter, histogram};
use skill_live_info_common::cache::TtlCache;
use skill_live_info_common::retry::with_retries;
use std::sync::Arc;
use std::time::{Duration, Instant};

const BACKEND_SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const BACKEND_SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";

pub struct HttpHoroscopeDailySkill {
    provider: Arc<dyn HoroscopeDataProvider>,
    cache: TtlCache<String, HoroscopeDailyResult>,
    fresh_ttl: Duration,
    stale_ttl: Duration,
    retry_attempts: usize,
}

impl HttpHoroscopeDailySkill {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(HttpHoroscopeDataProvider::new()),
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(24 * 60 * 60),
            stale_ttl: Duration::from_secs(72 * 60 * 60),
            retry_attempts: 2,
        }
    }

    fn cache_key(query: &HoroscopeDailyQuery) -> String {
        format!(
            "{}|{}",
            query.sign.trim().to_lowercase(),
            query.date.map(|d| d.to_string()).unwrap_or_default()
        )
    }

    fn error_kind(error: &HoroscopeDailyError) -> &'static str {
        match error {
            HoroscopeDailyError::InvalidSign(_) => "invalid_sign",
            HoroscopeDailyError::UnsupportedDate(_) => "unsupported_date",
            HoroscopeDailyError::ProviderUnavailable(_) => "provider_unavailable",
            HoroscopeDailyError::UpstreamTimeout => "upstream_timeout",
            HoroscopeDailyError::UpstreamParse(_) => "upstream_parse",
        }
    }

    #[cfg(test)]
    fn new_for_tests(provider: Arc<dyn HoroscopeDataProvider>) -> Self {
        Self {
            provider,
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_millis(1),
            stale_ttl: Duration::from_secs(2),
            retry_attempts: 1,
        }
    }
}

impl Default for HttpHoroscopeDailySkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HoroscopeDailySkill for HttpHoroscopeDailySkill {
    async fn execute(
        &self,
        query: &HoroscopeDailyQuery,
    ) -> Result<HoroscopeDailyResult, HoroscopeDailyError> {
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
            "skill" => "horoscope_daily",
            "result" => result_label,
            "error_kind" => error_kind
        );
        histogram!(
            BACKEND_SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => "horoscope_daily"
        );

        result
    }
}

pub struct MockHoroscopeDailySkill {
    pub result: Result<HoroscopeDailyResult, HoroscopeDailyError>,
}

impl MockHoroscopeDailySkill {
    pub fn ok(result: HoroscopeDailyResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: HoroscopeDailyError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl HoroscopeDailySkill for MockHoroscopeDailySkill {
    async fn execute(
        &self,
        _query: &HoroscopeDailyQuery,
    ) -> Result<HoroscopeDailyResult, HoroscopeDailyError> {
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
        responses: Mutex<VecDeque<Result<HoroscopeDailyResult, HoroscopeDailyError>>>,
    }

    impl StubProvider {
        fn with_responses(
            responses: Vec<Result<HoroscopeDailyResult, HoroscopeDailyError>>,
        ) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    #[async_trait]
    impl HoroscopeDataProvider for StubProvider {
        async fn fetch(
            &self,
            _query: &HoroscopeDailyQuery,
        ) -> Result<HoroscopeDailyResult, HoroscopeDailyError> {
            let mut guard = self.responses.lock().expect("lock");
            guard.pop_front().unwrap_or_else(|| {
                Err(HoroscopeDailyError::ProviderUnavailable(
                    "empty".to_string(),
                ))
            })
        }
    }

    fn query() -> HoroscopeDailyQuery {
        HoroscopeDailyQuery {
            sign: "leo".to_string(),
            date: None,
        }
    }

    fn result(text: &str) -> HoroscopeDailyResult {
        HoroscopeDailyResult {
            sign: "leo".to_string(),
            day: "today".to_string(),
            summary: text.to_string(),
            mood: Some("Focused".to_string()),
            color: Some("Gold".to_string()),
            lucky_number: Some("9".to_string()),
            as_of: SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn execute_success() {
        let provider = Arc::new(StubProvider::with_responses(vec![Ok(result("Great day"))]));
        let skill = HttpHoroscopeDailySkill::new_for_tests(provider);

        let value = skill.execute(&query()).await.expect("ok");

        assert_eq!(value.sign, "leo");
    }

    #[tokio::test]
    async fn execute_no_results() {
        let provider = Arc::new(StubProvider::with_responses(vec![Ok(result(""))]));
        let skill = HttpHoroscopeDailySkill::new_for_tests(provider);

        let value = skill.execute(&query()).await.expect("ok");

        assert_eq!(value.summary, "");
    }

    #[tokio::test]
    async fn execute_upstream_error() {
        let provider = Arc::new(StubProvider::with_responses(vec![Err(
            HoroscopeDailyError::ProviderUnavailable("down".to_string()),
        )]));
        let skill = HttpHoroscopeDailySkill::new_for_tests(provider);

        let value = skill.execute(&query()).await;

        assert!(matches!(
            value,
            Err(HoroscopeDailyError::ProviderUnavailable(message)) if message == "down"
        ));
    }

    #[tokio::test]
    async fn execute_stale_fallback() {
        let provider = Arc::new(StubProvider::with_responses(vec![
            Ok(result("Great day")),
            Err(HoroscopeDailyError::ProviderUnavailable("down".to_string())),
        ]));
        let skill = HttpHoroscopeDailySkill::new_for_tests(provider);

        let _first = skill.execute(&query()).await.expect("first");

        tokio::time::sleep(Duration::from_millis(5)).await;

        let value = skill.execute(&query()).await.expect("fallback");
        assert_eq!(value.summary, "Great day");
    }
}
