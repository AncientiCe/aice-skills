//! Dictionary skill backed by dictionaryapi.dev.

mod http_provider;
mod types;

pub use http_provider::{DictionaryApiDevProvider, DictionaryProvider};
pub use types::{DictionaryEntry, DictionaryError, DictionaryResult, DictionarySkill};

use async_trait::async_trait;
use metrics::{counter, histogram};
use skill_live_info_common::cache::TtlCache;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Like `skill_live_info_common::retry::with_retries`, but treats permanent
/// dictionary errors (`NotFound`, `InvalidQuery`) as final on the first attempt
/// so we don't burn retries on a typo lookup.
async fn retry_transient<F, Fut>(
    attempts: usize,
    mut operation: F,
) -> Result<DictionaryResult, DictionaryError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<DictionaryResult, DictionaryError>>,
{
    let attempts = attempts.max(1);
    let mut last_error = None;
    for index in 0..attempts {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                let is_permanent = matches!(
                    error,
                    DictionaryError::NotFound(_) | DictionaryError::InvalidQuery(_)
                );
                last_error = Some(error);
                if is_permanent || index + 1 == attempts {
                    break;
                }
            }
        }
    }
    Err(last_error.expect("at least one attempt"))
}

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "dictionary";

pub struct HttpDictionarySkill {
    provider: Arc<dyn DictionaryProvider>,
    cache: TtlCache<String, DictionaryResult>,
    fresh_ttl: Duration,
    stale_ttl: Duration,
    retry_attempts: usize,
}

impl HttpDictionarySkill {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(DictionaryApiDevProvider::new()),
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(60 * 60 * 24),
            stale_ttl: Duration::from_secs(60 * 60 * 24 * 7),
            retry_attempts: 2,
        }
    }

    pub fn with_provider(provider: Arc<dyn DictionaryProvider>) -> Self {
        Self {
            provider,
            cache: TtlCache::new(),
            fresh_ttl: Duration::from_secs(60 * 60 * 24),
            stale_ttl: Duration::from_secs(60 * 60 * 24 * 7),
            retry_attempts: 2,
        }
    }

    fn cache_key(word: &str) -> String {
        word.trim().to_lowercase()
    }

    fn error_kind(error: &DictionaryError) -> &'static str {
        match error {
            DictionaryError::InvalidQuery(_) => "invalid_query",
            DictionaryError::NotFound(_) => "not_found",
            DictionaryError::ProviderUnavailable(_) => "provider_unavailable",
            DictionaryError::UpstreamTimeout => "upstream_timeout",
            DictionaryError::UpstreamParse(_) => "upstream_parse",
        }
    }
}

impl Default for HttpDictionarySkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DictionarySkill for HttpDictionarySkill {
    async fn execute(&self, word: &str) -> Result<DictionaryResult, DictionaryError> {
        let started_at = Instant::now();
        let key = Self::cache_key(word);

        let result = async {
            if key.is_empty() {
                return Err(DictionaryError::InvalidQuery("empty word".to_string()));
            }
            if let Some(cached) = self.cache.get_fresh(&key) {
                return Ok(cached.value);
            }
            let fetched = retry_transient(self.retry_attempts, || self.provider.fetch(word)).await;
            match fetched {
                Ok(value) => {
                    self.cache
                        .insert(key.clone(), value.clone(), self.fresh_ttl, self.stale_ttl);
                    Ok(value)
                }
                Err(error) => {
                    if matches!(
                        error,
                        DictionaryError::NotFound(_) | DictionaryError::InvalidQuery(_)
                    ) {
                        return Err(error);
                    }
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

pub struct MockDictionarySkill {
    pub result: Result<DictionaryResult, DictionaryError>,
}

impl MockDictionarySkill {
    pub fn ok(result: DictionaryResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: DictionaryError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl DictionarySkill for MockDictionarySkill {
    async fn execute(&self, _word: &str) -> Result<DictionaryResult, DictionaryError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct StubProvider {
        responses: Mutex<VecDeque<Result<DictionaryResult, DictionaryError>>>,
    }

    impl StubProvider {
        fn with(responses: Vec<Result<DictionaryResult, DictionaryError>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    #[async_trait]
    impl DictionaryProvider for StubProvider {
        async fn fetch(&self, _word: &str) -> Result<DictionaryResult, DictionaryError> {
            let mut guard = self.responses.lock().expect("lock");
            guard
                .pop_front()
                .unwrap_or_else(|| Err(DictionaryError::ProviderUnavailable("empty".into())))
        }
    }

    fn ok_result() -> DictionaryResult {
        DictionaryResult {
            word: "ephemeral".to_string(),
            phonetic: Some("/ɪˈfɛm(ə)rəl/".to_string()),
            entries: vec![DictionaryEntry {
                part_of_speech: Some("adjective".to_string()),
                definition: "lasting for a very short time".to_string(),
                example: Some("fashions are ephemeral".to_string()),
                synonyms: vec!["transient".into(), "fleeting".into()],
                antonyms: vec!["permanent".into()],
            }],
        }
    }

    #[tokio::test]
    async fn execute_success() {
        let provider = Arc::new(StubProvider::with(vec![Ok(ok_result())]));
        let skill = HttpDictionarySkill::with_provider(provider);
        let result = skill.execute("ephemeral").await.expect("ok");
        assert_eq!(result.word, "ephemeral");
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].synonyms[0], "transient");
    }

    #[tokio::test]
    async fn execute_not_found() {
        let provider = Arc::new(StubProvider::with(vec![Err(DictionaryError::NotFound(
            "wibble".into(),
        ))]));
        let skill = HttpDictionarySkill::with_provider(provider);
        let err = skill.execute("wibble").await.expect_err("err");
        assert!(matches!(err, DictionaryError::NotFound(_)));
    }

    #[tokio::test]
    async fn execute_empty_word() {
        let provider = Arc::new(StubProvider::with(vec![Ok(ok_result())]));
        let skill = HttpDictionarySkill::with_provider(provider);
        let err = skill.execute("   ").await.expect_err("err");
        assert!(matches!(err, DictionaryError::InvalidQuery(_)));
    }
}
