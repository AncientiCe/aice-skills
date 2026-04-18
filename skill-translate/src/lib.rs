//! Translation skill backed by a local LLM (e.g. Ollama).
//!
//! The runtime adapts its concrete `LlmStream` to the small `TranslationLlm`
//! trait defined in `types`, keeping `aice-skills` free of any LLM transport
//! dependency.

mod types;

pub use types::{TranslateError, TranslateQuery, TranslateResult, TranslateSkill, TranslationLlm};

use async_trait::async_trait;
use metrics::{counter, histogram};
use std::sync::Arc;
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "translate";

pub struct LlmTranslateSkill {
    llm: Arc<dyn TranslationLlm>,
}

impl LlmTranslateSkill {
    pub fn new(llm: Arc<dyn TranslationLlm>) -> Self {
        Self { llm }
    }

    fn error_kind(error: &TranslateError) -> &'static str {
        match error {
            TranslateError::InvalidQuery(_) => "invalid_query",
            TranslateError::LlmUnavailable(_) => "llm_unavailable",
            TranslateError::EmptyTranslation => "empty_translation",
        }
    }

    /// Compose the system prompt that constrains the model to produce ONLY the
    /// translation, with no commentary, quotes, or surrounding text.
    pub fn system_prompt(target: &str, source: Option<&str>) -> String {
        let source_clause = match source {
            Some(s) if !s.trim().is_empty() => format!("from {} ", s.trim()),
            _ => String::new(),
        };
        format!(
            "You are a translation engine. Translate the user's text {source_clause}into {target}. \
             Output only the translated text. Do not add explanations, romanizations, quotes, \
             prefixes, or any other commentary. If the input is empty, output nothing.",
            source_clause = source_clause,
            target = target
        )
    }

    fn sanitize_translation(raw: &str) -> String {
        let mut s = raw.trim().to_string();
        for prefix in ["translation:", "translated:", "answer:", "output:"] {
            if s.to_lowercase().starts_with(prefix) {
                s = s[prefix.len()..].trim_start().to_string();
                break;
            }
        }
        s = s
            .trim()
            .trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
            .trim()
            .to_string();
        s
    }
}

#[async_trait]
impl TranslateSkill for LlmTranslateSkill {
    async fn execute(&self, query: &TranslateQuery) -> Result<TranslateResult, TranslateError> {
        let started_at = Instant::now();
        let result = async {
            let text = query.text.trim();
            let target = query.target_language.trim();
            if text.is_empty() {
                return Err(TranslateError::InvalidQuery("empty text".to_string()));
            }
            if target.is_empty() {
                return Err(TranslateError::InvalidQuery(
                    "empty target language".to_string(),
                ));
            }
            let prompt = Self::system_prompt(target, query.source_language.as_deref());
            let raw = self
                .llm
                .complete(&prompt, text)
                .await
                .map_err(TranslateError::LlmUnavailable)?;
            let translation = Self::sanitize_translation(&raw);
            if translation.is_empty() {
                return Err(TranslateError::EmptyTranslation);
            }
            Ok(TranslateResult {
                source_language: query.source_language.clone(),
                target_language: target.to_string(),
                original: text.to_string(),
                translation,
            })
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

pub struct MockTranslateSkill {
    pub result: Result<TranslateResult, TranslateError>,
}

impl MockTranslateSkill {
    pub fn ok(result: TranslateResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: TranslateError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl TranslateSkill for MockTranslateSkill {
    async fn execute(&self, _query: &TranslateQuery) -> Result<TranslateResult, TranslateError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticLlm {
        response: Result<String, String>,
    }

    #[async_trait]
    impl TranslationLlm for StaticLlm {
        async fn complete(&self, _prompt: &str, _user: &str) -> Result<String, String> {
            self.response.clone()
        }
    }

    fn query() -> TranslateQuery {
        TranslateQuery {
            text: "good morning".into(),
            source_language: Some("english".into()),
            target_language: "french".into(),
        }
    }

    #[tokio::test]
    async fn execute_success_returns_translation() {
        let llm = Arc::new(StaticLlm {
            response: Ok("bonjour".into()),
        });
        let skill = LlmTranslateSkill::new(llm);
        let result = skill.execute(&query()).await.expect("ok");
        assert_eq!(result.translation, "bonjour");
        assert_eq!(result.target_language, "french");
    }

    #[tokio::test]
    async fn execute_strips_quotes_and_labels() {
        let llm = Arc::new(StaticLlm {
            response: Ok("Translation: \"bonjour\"".into()),
        });
        let skill = LlmTranslateSkill::new(llm);
        let result = skill.execute(&query()).await.expect("ok");
        assert_eq!(result.translation, "bonjour");
    }

    #[tokio::test]
    async fn execute_rejects_empty_text() {
        let llm = Arc::new(StaticLlm {
            response: Ok("ignored".into()),
        });
        let skill = LlmTranslateSkill::new(llm);
        let mut q = query();
        q.text = "   ".into();
        let err = skill.execute(&q).await.expect_err("err");
        assert!(matches!(err, TranslateError::InvalidQuery(_)));
    }

    #[tokio::test]
    async fn execute_rejects_empty_translation() {
        let llm = Arc::new(StaticLlm {
            response: Ok("   ".into()),
        });
        let skill = LlmTranslateSkill::new(llm);
        let err = skill.execute(&query()).await.expect_err("err");
        assert_eq!(err, TranslateError::EmptyTranslation);
    }

    #[tokio::test]
    async fn execute_propagates_llm_error() {
        let llm = Arc::new(StaticLlm {
            response: Err("ollama down".into()),
        });
        let skill = LlmTranslateSkill::new(llm);
        let err = skill.execute(&query()).await.expect_err("err");
        assert!(matches!(err, TranslateError::LlmUnavailable(_)));
    }

    #[test]
    fn system_prompt_includes_source_when_specified() {
        let p = LlmTranslateSkill::system_prompt("french", Some("english"));
        assert!(p.contains("from english"));
        assert!(p.contains("into french"));
    }

    #[test]
    fn system_prompt_omits_source_when_unspecified() {
        let p = LlmTranslateSkill::system_prompt("french", None);
        assert!(!p.contains("from "));
        assert!(p.contains("into french"));
    }
}
