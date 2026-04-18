use async_trait::async_trait;

#[derive(Clone, Debug, PartialEq)]
pub struct DictionaryEntry {
    pub part_of_speech: Option<String>,
    pub definition: String,
    pub example: Option<String>,
    pub synonyms: Vec<String>,
    pub antonyms: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DictionaryResult {
    pub word: String,
    pub phonetic: Option<String>,
    pub entries: Vec<DictionaryEntry>,
}

impl DictionaryResult {
    pub fn to_prompt_context(&self) -> String {
        if self.entries.is_empty() {
            return format!("No definitions found for {}.", self.word);
        }
        let summary = self
            .entries
            .iter()
            .take(3)
            .map(|e| {
                let pos = e.part_of_speech.clone().unwrap_or_default();
                if pos.is_empty() {
                    e.definition.clone()
                } else {
                    format!("({pos}) {}", e.definition)
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!("{}: {}", self.word, summary)
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum DictionaryError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("word not found: {0}")]
    NotFound(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("provider timeout")]
    UpstreamTimeout,
    #[error("provider response parse failed: {0}")]
    UpstreamParse(String),
}

#[async_trait]
pub trait DictionarySkill: Send + Sync {
    async fn execute(&self, word: &str) -> Result<DictionaryResult, DictionaryError>;
}
