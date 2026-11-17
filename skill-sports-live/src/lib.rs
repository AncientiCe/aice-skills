mod http_provider;
mod types;

pub use types::{SportsEvent, SportsLiveError, SportsLiveQuery, SportsLiveResult, SportsLiveSkill};

use async_trait::async_trait;
use http_provider::{HttpSportsDataProvider, SportsDataProvider};
use metrics::{counter, histogram};
use skill_live_info_common::cache::TtlCache;
use skill_live_info_common::retry::with_retries;
use std::sync::Arc;
use std::time::{Duration, Instant};

const BACKEND_SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const BACKEND_SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";

pub struct HttpSportsLiveSkill {
    provider: Arc<dyn SportsDataProvider>,
    cache: TtlCache<String, SportsLiveResult>,
    fresh_ttl: Duration,
    stale_ttl: Duration,
    retry_attempts: usize,
}

impl HttpSportsLiveSkill {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(HttpSportsDataProvider::new()),
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(90),
            stale_ttl: Duration::from_secs(10 * 60),
            retry_attempts: 2,
        }
    }

    fn cache_key(query: &SportsLiveQuery) -> String {
        format!(
            "{}|{}",
            query.query.trim().to_lowercase(),
            query
                .date
                .map(|date| date.strftime("%Y-%m-%d").to_string())
                .unwrap_or_default()
        )
    }

    fn error_kind(error: &SportsLiveError) -> &'static str {
        match error {
            SportsLiveError::InvalidQuery(_) => "invalid_query",
            SportsLiveError::ProviderUnavailable(_) => "provider_unavailable",
            SportsLiveError::UpstreamTimeout => "upstream_timeout",
            SportsLiveError::UpstreamParse(_) => "upstream_parse",
        }
    }

    #[cfg(test)]
    fn new_for_tests(provider: Arc<dyn SportsDataProvider>) -> Self {
        Self {
            provider,
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_millis(1),
            stale_ttl: Duration::from_secs(2),
            retry_attempts: 1,
        }
    }
}

impl Default for HttpSportsLiveSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SportsLiveSkill for HttpSportsLiveSkill {
    async fn execute(&self, query: &SportsLiveQuery) -> Result<SportsLiveResult, SportsLiveError> {
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
            "skill" => "sports_live",
            "result" => result_label,
            "error_kind" => error_kind
        );
        histogram!(
            BACKEND_SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => "sports_live"
        );

        result
    }
}

pub struct MockSportsLiveSkill {
    pub result: Result<SportsLiveResult, SportsLiveError>,
}

impl MockSportsLiveSkill {
    pub fn ok(result: SportsLiveResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: SportsLiveError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl SportsLiveSkill for MockSportsLiveSkill {
    async fn execute(&self, _query: &SportsLiveQuery) -> Result<SportsLiveResult, SportsLiveError> {
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
        responses: Mutex<VecDeque<Result<SportsLiveResult, SportsLiveError>>>,
    }

    impl StubProvider {
        fn with_responses(responses: Vec<Result<SportsLiveResult, SportsLiveError>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    #[async_trait]
    impl SportsDataProvider for StubProvider {
        async fn fetch(
            &self,
            _query: &SportsLiveQuery,
        ) -> Result<SportsLiveResult, SportsLiveError> {
            let mut guard = self.responses.lock().expect("lock");
            guard
                .pop_front()
                .unwrap_or_else(|| Err(SportsLiveError::ProviderUnavailable("empty".to_string())))
        }
    }

    fn query() -> SportsLiveQuery {
        SportsLiveQuery {
            query: "Arsenal".to_string(),
            date: None,
        }
    }

    fn result(name: &str) -> SportsLiveResult {
        SportsLiveResult {
            events: vec![SportsEvent {
                league: Some("EPL".to_string()),
                event: name.to_string(),
                home_team: Some("Arsenal".to_string()),
                away_team: Some("Chelsea".to_string()),
                start_time: Some("2026-03-31T18:00:00".to_string()),
                status: Some("Match Finished".to_string()),
                home_score: Some(2),
                away_score: Some(1),
                scorers: vec!["Saka".to_string()],
            }],
            as_of: SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn execute_success() {
        let provider = Arc::new(StubProvider::with_responses(vec![Ok(result(
            "Arsenal vs Chelsea",
        ))]));
        let skill = HttpSportsLiveSkill::new_for_tests(provider);
        let value = skill.execute(&query()).await.expect("ok");
        assert_eq!(value.events.len(), 1);
    }

    #[tokio::test]
    async fn execute_no_results() {
        let provider = Arc::new(StubProvider::with_responses(vec![Ok(SportsLiveResult {
            events: vec![],
            as_of: SystemTime::now(),
        })]));
        let skill = HttpSportsLiveSkill::new_for_tests(provider);
        let value = skill.execute(&query()).await.expect("ok");
        assert!(value.events.is_empty());
    }

    #[tokio::test]
    async fn execute_upstream_error() {
        let provider = Arc::new(StubProvider::with_responses(vec![Err(
            SportsLiveError::ProviderUnavailable("down".to_string()),
        )]));
        let skill = HttpSportsLiveSkill::new_for_tests(provider);
        let result = skill.execute(&query()).await;
        assert!(
            matches!(result, Err(SportsLiveError::ProviderUnavailable(message)) if message == "down")
        );
    }

    #[tokio::test]
    async fn execute_stale_fallback() {
        let provider = Arc::new(StubProvider::with_responses(vec![
            Ok(result("Arsenal vs Chelsea")),
            Err(SportsLiveError::ProviderUnavailable("down".to_string())),
        ]));
        let skill = HttpSportsLiveSkill::new_for_tests(provider);
        let _first = skill.execute(&query()).await.expect("first");
        tokio::time::sleep(Duration::from_millis(5)).await;
        let second = skill.execute(&query()).await.expect("fallback");
        assert_eq!(second.events.len(), 1);
    }
}
