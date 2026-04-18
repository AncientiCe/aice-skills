use async_trait::async_trait;

/// Inputs for the template skill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateQuery {
    pub input: String,
}

/// Structured result. Implement `to_prompt_context` so the LLM has a
/// compact, deterministic string to weave into a spoken answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateResult {
    pub echoed: String,
}

impl TemplateResult {
    pub fn to_prompt_context(&self) -> String {
        format!("Echo: {}.", self.echoed)
    }
}

/// All recoverable errors. Add variants as the surface grows; never
/// shoehorn unrelated failures into a generic `Other(String)`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
}

impl TemplateError {
    /// Stable label for the `error_kind` metrics dimension.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidQuery(_) => "invalid_query",
            Self::ProviderUnavailable(_) => "provider_unavailable",
        }
    }
}

#[async_trait]
pub trait TemplateSkill: Send + Sync {
    async fn execute(&self, query: &TemplateQuery) -> Result<TemplateResult, TemplateError>;
}
