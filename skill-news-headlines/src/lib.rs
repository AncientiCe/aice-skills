mod http_provider;
mod summarize;
mod types;

pub use summarize::{collect_summaries, stream_summaries, NewsSummaryEvent};
pub use types::{
    NewsHeadline, NewsHeadlinesError, NewsHeadlinesQuery, NewsHeadlinesResult, NewsHeadlinesSkill,
    NewsSummaryLlm, SummarizedHeadline,
};

use async_trait::async_trait;
use http_provider::{HttpNewsDataProvider, NewsDataProvider};
use metrics::{counter, histogram};
use skill_live_info_common::cache::TtlCache;
use skill_live_info_common::retry::with_retries;
use std::sync::Arc;
use std::time::{Duration, Instant};

const BACKEND_SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const BACKEND_SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";

pub struct HttpNewsHeadlinesSkill {
    provider: Arc<dyn NewsDataProvider>,
    cache: TtlCache<String, NewsHeadlinesResult>,
    fresh_ttl: Duration,
    stale_ttl: Duration,
    retry_attempts: usize,
}

impl HttpNewsHeadlinesSkill {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(HttpNewsDataProvider::new()),
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(120),
            stale_ttl: Duration::from_secs(10 * 60),
            retry_attempts: 2,
        }
    }

    fn cache_key(query: &NewsHeadlinesQuery) -> String {
        format!(
            "{}|{}|{}",
            query.topic.trim().to_lowercase(),
            query
                .country_code
                .clone()
                .unwrap_or_default()
                .to_uppercase(),
            query.limit.unwrap_or(10)
        )
    }

    fn error_kind(error: &NewsHeadlinesError) -> &'static str {
        match error {
            NewsHeadlinesError::InvalidQuery(_) => "invalid_query",
            NewsHeadlinesError::ProviderUnavailable(_) => "provider_unavailable",
            NewsHeadlinesError::UpstreamTimeout => "upstream_timeout",
            NewsHeadlinesError::UpstreamParse(_) => "upstream_parse",
        }
    }

    #[cfg(test)]
    fn new_for_tests(provider: Arc<dyn NewsDataProvider>) -> Self {
        Self {
            provider,
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_millis(1),
            stale_ttl: Duration::from_secs(2),
            retry_attempts: 1,
        }
    }
}

impl Default for HttpNewsHeadlinesSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NewsHeadlinesSkill for HttpNewsHeadlinesSkill {
    async fn execute(
        &self,
        query: &NewsHeadlinesQuery,
    ) -> Result<NewsHeadlinesResult, NewsHeadlinesError> {
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
            "skill" => "news_headlines",
            "result" => result_label,
            "error_kind" => error_kind
        );
        histogram!(
            BACKEND_SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => "news_headlines"
        );

        result
    }
}

pub struct MockNewsHeadlinesSkill {
    pub result: Result<NewsHeadlinesResult, NewsHeadlinesError>,
}

impl MockNewsHeadlinesSkill {
    pub fn ok(result: NewsHeadlinesResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: NewsHeadlinesError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl NewsHeadlinesSkill for MockNewsHeadlinesSkill {
    async fn execute(
        &self,
        _query: &NewsHeadlinesQuery,
    ) -> Result<NewsHeadlinesResult, NewsHeadlinesError> {
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
        responses: Mutex<VecDeque<Result<NewsHeadlinesResult, NewsHeadlinesError>>>,
    }

    impl StubProvider {
        fn with_responses(responses: Vec<Result<NewsHeadlinesResult, NewsHeadlinesError>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    #[async_trait]
    impl NewsDataProvider for StubProvider {
        async fn fetch(
            &self,
            _query: &NewsHeadlinesQuery,
        ) -> Result<NewsHeadlinesResult, NewsHeadlinesError> {
            let mut guard = self.responses.lock().expect("lock");
            guard.pop_front().unwrap_or_else(|| {
                Err(NewsHeadlinesError::ProviderUnavailable("empty".to_string()))
            })
        }
    }

    fn query() -> NewsHeadlinesQuery {
        NewsHeadlinesQuery {
            topic: "technology".to_string(),
            country_code: None,
            limit: Some(5),
        }
    }

    fn result() -> NewsHeadlinesResult {
        NewsHeadlinesResult {
            headlines: vec![NewsHeadline {
                title: "Breaking headline".to_string(),
                source: Some("example.com".to_string()),
                url: Some("https://example.com/story".to_string()),
                published_at: Some("20260331T120000Z".to_string()),
            }],
            as_of: SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn execute_success() {
        let provider = Arc::new(StubProvider::with_responses(vec![Ok(result())]));
        let skill = HttpNewsHeadlinesSkill::new_for_tests(provider);

        let value = skill.execute(&query()).await.expect("ok");

        assert_eq!(value.headlines.len(), 1);
    }

    #[tokio::test]
    async fn execute_no_results() {
        let provider = Arc::new(StubProvider::with_responses(vec![Ok(
            NewsHeadlinesResult {
                headlines: vec![],
                as_of: SystemTime::now(),
            },
        )]));
        let skill = HttpNewsHeadlinesSkill::new_for_tests(provider);

        let value = skill.execute(&query()).await.expect("ok");

        assert!(value.headlines.is_empty());
    }

    #[tokio::test]
    async fn execute_upstream_error() {
        let provider = Arc::new(StubProvider::with_responses(vec![Err(
            NewsHeadlinesError::ProviderUnavailable("down".to_string()),
        )]));
        let skill = HttpNewsHeadlinesSkill::new_for_tests(provider);

        let value = skill.execute(&query()).await;

        assert!(matches!(
            value,
            Err(NewsHeadlinesError::ProviderUnavailable(message)) if message == "down"
        ));
    }

    #[tokio::test]
    async fn execute_stale_fallback() {
        let provider = Arc::new(StubProvider::with_responses(vec![
            Ok(result()),
            Err(NewsHeadlinesError::ProviderUnavailable("down".to_string())),
        ]));
        let skill = HttpNewsHeadlinesSkill::new_for_tests(provider);

        let _first = skill.execute(&query()).await.expect("first");

        tokio::time::sleep(Duration::from_millis(5)).await;

        let value = skill.execute(&query()).await.expect("fallback");
        assert_eq!(value.headlines.len(), 1);
    }
}
