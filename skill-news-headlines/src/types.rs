use async_trait::async_trait;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct NewsHeadlinesQuery {
    pub topic: String,
    pub country_code: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewsHeadline {
    pub title: String,
    pub source: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewsHeadlinesResult {
    pub headlines: Vec<NewsHeadline>,
    pub as_of: SystemTime,
}

impl NewsHeadlinesResult {
    pub fn to_prompt_context(&self) -> String {
        if self.headlines.is_empty() {
            return "No headlines found.".to_string();
        }
        let summary = self
            .headlines
            .iter()
            .map(|item| item.title.clone())
            .collect::<Vec<_>>()
            .join("; ");
        format!("Top headlines: {}.", summary)
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum NewsHeadlinesError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("provider timeout")]
    UpstreamTimeout,
    #[error("provider response parse failed: {0}")]
    UpstreamParse(String),
}

#[async_trait]
pub trait NewsHeadlinesSkill: Send + Sync {
    async fn execute(
        &self,
        query: &NewsHeadlinesQuery,
    ) -> Result<NewsHeadlinesResult, NewsHeadlinesError>;
}

/// Per-headline summarizer that streams partial text chunks. Implementations
/// typically wrap a local Ollama `LlmStream` from the backend `core-llm` crate.
///
/// The trait is intentionally narrow so this crate doesn't pull in
/// `core-llm` directly; backend wiring adapts a real `LlmStream` to it.
#[async_trait]
pub trait NewsSummaryLlm: Send + Sync {
    /// Produce a streaming summary for `headline`. Each yielded chunk is a
    /// partial token/word stream. The receiver closes when generation ends.
    async fn summarize(
        &self,
        headline: &NewsHeadline,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, NewsHeadlinesError>;
}

/// One headline + its (optionally streamed) summary, batched on completion.
#[derive(Clone, Debug)]
pub struct SummarizedHeadline {
    pub headline: NewsHeadline,
    pub summary: Option<String>,
    pub summary_error: Option<String>,
}
