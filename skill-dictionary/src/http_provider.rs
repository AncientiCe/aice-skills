use crate::types::{DictionaryEntry, DictionaryError, DictionaryResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;
use urlencoding::encode;

#[async_trait]
pub trait DictionaryProvider: Send + Sync {
    async fn fetch(&self, word: &str) -> Result<DictionaryResult, DictionaryError>;
}

#[derive(Clone)]
pub struct DictionaryApiDevProvider {
    client: reqwest::Client,
    base_url: String,
}

impl DictionaryApiDevProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.dictionaryapi.dev/api/v2/entries/en".to_string(),
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }
}

impl Default for DictionaryApiDevProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct DictApiEntry {
    word: String,
    #[serde(default)]
    phonetic: Option<String>,
    #[serde(default)]
    meanings: Vec<DictApiMeaning>,
}

#[derive(Debug, Deserialize)]
struct DictApiMeaning {
    #[serde(rename = "partOfSpeech")]
    part_of_speech: Option<String>,
    #[serde(default)]
    definitions: Vec<DictApiDefinition>,
    #[serde(default)]
    synonyms: Vec<String>,
    #[serde(default)]
    antonyms: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DictApiDefinition {
    definition: String,
    #[serde(default)]
    example: Option<String>,
    #[serde(default)]
    synonyms: Vec<String>,
    #[serde(default)]
    antonyms: Vec<String>,
}

#[async_trait]
impl DictionaryProvider for DictionaryApiDevProvider {
    async fn fetch(&self, word: &str) -> Result<DictionaryResult, DictionaryError> {
        let trimmed = word.trim();
        if trimmed.is_empty() {
            return Err(DictionaryError::InvalidQuery("empty word".to_string()));
        }
        let url = format!("{}/{}", self.base_url, encode(trimmed));
        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    DictionaryError::UpstreamTimeout
                } else {
                    DictionaryError::ProviderUnavailable(err.to_string())
                }
            })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(DictionaryError::NotFound(trimmed.to_string()));
        }
        if !response.status().is_success() {
            return Err(DictionaryError::ProviderUnavailable(format!(
                "status {}",
                response.status()
            )));
        }
        let payload = response
            .json::<Vec<DictApiEntry>>()
            .await
            .map_err(|err| DictionaryError::UpstreamParse(err.to_string()))?;
        let first = payload
            .into_iter()
            .next()
            .ok_or_else(|| DictionaryError::NotFound(trimmed.to_string()))?;
        let phonetic = first.phonetic;
        let mut entries = Vec::new();
        for meaning in first.meanings {
            for def in meaning.definitions {
                let mut synonyms = meaning.synonyms.clone();
                synonyms.extend(def.synonyms.clone());
                synonyms.dedup();
                let mut antonyms = meaning.antonyms.clone();
                antonyms.extend(def.antonyms.clone());
                antonyms.dedup();
                entries.push(DictionaryEntry {
                    part_of_speech: meaning.part_of_speech.clone(),
                    definition: def.definition,
                    example: def.example,
                    synonyms,
                    antonyms,
                });
            }
        }
        Ok(DictionaryResult {
            word: first.word,
            phonetic,
            entries,
        })
    }
}
