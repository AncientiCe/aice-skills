use async_trait::async_trait;

#[derive(Clone, Debug, PartialEq)]
pub struct TranslateQuery {
    pub text: String,
    pub source_language: Option<String>,
    pub target_language: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranslateResult {
    pub source_language: Option<String>,
    pub target_language: String,
    pub original: String,
    pub translation: String,
}

impl TranslateResult {
    pub fn to_prompt_context(&self) -> String {
        let from = self
            .source_language
            .clone()
            .unwrap_or_else(|| "auto".to_string());
        format!(
            "Translation ({} -> {}): \"{}\" -> \"{}\"",
            from, self.target_language, self.original, self.translation
        )
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum TranslateError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("llm unavailable: {0}")]
    LlmUnavailable(String),
    #[error("llm produced empty translation")]
    EmptyTranslation,
}

/// Translation skill: phrase translation backed by a local LLM.
#[async_trait]
pub trait TranslateSkill: Send + Sync {
    async fn execute(&self, query: &TranslateQuery) -> Result<TranslateResult, TranslateError>;
}

/// Adapter trait the runtime implements over its concrete `LlmStream` so this
/// crate stays decoupled from any specific LLM transport. Returns the full
/// completion as a single string (no streaming required for short
/// translations).
#[async_trait]
pub trait TranslationLlm: Send + Sync {
    async fn complete(&self, system_prompt: &str, user_text: &str) -> Result<String, String>;
}
