mod http_provider;
mod types;

pub use types::{
    HolidayLookupError, HolidayLookupResult, HolidayLookupSkill, HolidayMatch, HolidayQuery,
};

use async_trait::async_trait;
use http_provider::{HolidayDataProvider, HttpHolidayDataProvider};
use metrics::{counter, histogram};
use skill_live_info_common::cache::TtlCache;
use skill_live_info_common::retry::with_retries;
use std::sync::Arc;
use std::time::{Duration, Instant};

const BACKEND_SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const BACKEND_SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";

pub struct HttpHolidayLookupSkill {
    provider: Arc<dyn HolidayDataProvider>,
    cache: TtlCache<String, HolidayLookupResult>,
    fresh_ttl: Duration,
    stale_ttl: Duration,
    retry_attempts: usize,
}

impl HttpHolidayLookupSkill {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(HttpHolidayDataProvider::new()),
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(24 * 60 * 60),
            stale_ttl: Duration::from_secs(72 * 60 * 60),
            retry_attempts: 2,
        }
    }

    fn cache_key(query: &HolidayQuery) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            query.country_code.trim().to_uppercase(),
            query.region_code.clone().unwrap_or_default().to_uppercase(),
            query.year.unwrap_or_default(),
            query
                .holiday_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_lowercase(),
            query
                .date
                .map(|date| date.strftime("%Y-%m-%d").to_string())
                .unwrap_or_default()
        )
    }

    fn error_kind(error: &HolidayLookupError) -> &'static str {
        match error {
            HolidayLookupError::InvalidCountry(_) => "invalid_country",
            HolidayLookupError::InvalidQuery(_) => "invalid_query",
            HolidayLookupError::ProviderUnavailable(_) => "provider_unavailable",
            HolidayLookupError::UpstreamTimeout => "upstream_timeout",
            HolidayLookupError::UpstreamParse(_) => "upstream_parse",
        }
    }

    #[cfg(test)]
    fn new_for_tests(provider: Arc<dyn HolidayDataProvider>) -> Self {
        Self {
            provider,
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_millis(1),
            stale_ttl: Duration::from_secs(2),
            retry_attempts: 1,
        }
    }
}

impl Default for HttpHolidayLookupSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HolidayLookupSkill for HttpHolidayLookupSkill {
    async fn execute(
        &self,
        query: &HolidayQuery,
    ) -> Result<HolidayLookupResult, HolidayLookupError> {
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
            "skill" => "holiday_lookup",
            "result" => result_label,
            "error_kind" => error_kind
        );
        histogram!(
            BACKEND_SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => "holiday_lookup"
        );

        result
    }
}

pub struct MockHolidayLookupSkill {
    pub result: Result<HolidayLookupResult, HolidayLookupError>,
}

impl MockHolidayLookupSkill {
    pub fn ok(result: HolidayLookupResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: HolidayLookupError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl HolidayLookupSkill for MockHolidayLookupSkill {
    async fn execute(
        &self,
        _query: &HolidayQuery,
    ) -> Result<HolidayLookupResult, HolidayLookupError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::Date;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::time::SystemTime;

    struct StubHolidayProvider {
        responses: Mutex<VecDeque<Result<HolidayLookupResult, HolidayLookupError>>>,
    }

    impl StubHolidayProvider {
        fn with_responses(responses: Vec<Result<HolidayLookupResult, HolidayLookupError>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    #[async_trait]
    impl HolidayDataProvider for StubHolidayProvider {
        async fn fetch(
            &self,
            _query: &HolidayQuery,
        ) -> Result<HolidayLookupResult, HolidayLookupError> {
            let mut guard = self.responses.lock().expect("lock");
            guard
                .pop_front()
                .unwrap_or_else(|| Err(HolidayLookupError::ProviderUnavailable("empty".into())))
        }
    }

    fn sample_query() -> HolidayQuery {
        HolidayQuery {
            holiday_name: Some("Easter".to_string()),
            date: None,
            country_code: "DE".to_string(),
            region_code: None,
            year: Some(2026),
        }
    }

    fn sample_result(name: &str) -> HolidayLookupResult {
        HolidayLookupResult {
            country_code: "DE".to_string(),
            region_code: None,
            matches: vec![HolidayMatch {
                name: name.to_string(),
                date: Date::new(2026, 4, 5).expect("valid"),
                is_public_holiday: true,
                local_name: Some(name.to_string()),
                observed_date: None,
            }],
            as_of: SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn execute_success() {
        let provider = Arc::new(StubHolidayProvider::with_responses(vec![Ok(
            sample_result("Easter"),
        )]));
        let skill = HttpHolidayLookupSkill::new_for_tests(provider);
        let result = skill.execute(&sample_query()).await.expect("ok");
        assert_eq!(result.matches[0].name, "Easter");
    }

    #[tokio::test]
    async fn execute_no_results() {
        let provider = Arc::new(StubHolidayProvider::with_responses(vec![Ok(
            HolidayLookupResult {
                country_code: "DE".to_string(),
                region_code: None,
                matches: vec![],
                as_of: SystemTime::now(),
            },
        )]));
        let skill = HttpHolidayLookupSkill::new_for_tests(provider);
        let result = skill.execute(&sample_query()).await.expect("ok");
        assert!(result.matches.is_empty());
    }

    #[tokio::test]
    async fn execute_upstream_error() {
        let provider = Arc::new(StubHolidayProvider::with_responses(vec![Err(
            HolidayLookupError::ProviderUnavailable("down".into()),
        )]));
        let skill = HttpHolidayLookupSkill::new_for_tests(provider);
        let result = skill.execute(&sample_query()).await;
        assert!(
            matches!(result, Err(HolidayLookupError::ProviderUnavailable(message)) if message == "down")
        );
    }

    #[tokio::test]
    async fn execute_stale_fallback() {
        let provider = Arc::new(StubHolidayProvider::with_responses(vec![
            Ok(sample_result("Easter")),
            Err(HolidayLookupError::ProviderUnavailable("down".into())),
        ]));
        let skill = HttpHolidayLookupSkill::new_for_tests(provider);
        let _first = skill.execute(&sample_query()).await.expect("first");
        tokio::time::sleep(Duration::from_millis(5)).await;
        let second = skill.execute(&sample_query()).await.expect("fallback");
        assert_eq!(second.matches[0].name, "Easter");
    }
}
