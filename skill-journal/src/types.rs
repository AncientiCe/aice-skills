//! Journal skill types and traits.

use async_trait::async_trait;
use jiff::Timestamp;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Sentiment {
    Positive,
    Neutral,
    Negative,
}

impl Sentiment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::Neutral => "neutral",
            Self::Negative => "negative",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "positive" | "pos" | "+" => Some(Self::Positive),
            "neutral" | "neu" | "0" => Some(Self::Neutral),
            "negative" | "neg" | "-" => Some(Self::Negative),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JournalEntry {
    pub id: i64,
    pub created_at: Timestamp,
    pub text: String,
    pub sentiment: Sentiment,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JournalAction {
    Add {
        text: String,
        sentiment: Option<Sentiment>,
        tags: Vec<String>,
    },
    Recall {
        from: Option<Timestamp>,
        to: Option<Timestamp>,
        contains: Option<String>,
        limit: usize,
    },
    Stats {
        from: Option<Timestamp>,
        to: Option<Timestamp>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct JournalStats {
    pub total_entries: usize,
    pub positive: usize,
    pub neutral: usize,
    pub negative: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JournalResult {
    Added(JournalEntry),
    Entries(Vec<JournalEntry>),
    Stats(JournalStats),
}

impl JournalResult {
    pub fn to_prompt_context(&self) -> String {
        match self {
            Self::Added(e) => format!(
                "Journal entry saved at {} ({}): {}",
                e.created_at.strftime("%Y-%m-%d %H:%M UTC"),
                e.sentiment.as_str(),
                e.text
            ),
            Self::Entries(entries) => {
                if entries.is_empty() {
                    return "No matching journal entries.".to_string();
                }
                let mut out = vec![format!("{} entry/entries:", entries.len())];
                for e in entries {
                    out.push(format!(
                        "- [{}] {} ({}): {}",
                        e.created_at.strftime("%Y-%m-%d %H:%M"),
                        e.sentiment.as_str(),
                        if e.tags.is_empty() {
                            "no tags".to_string()
                        } else {
                            e.tags.join(",")
                        },
                        e.text
                    ));
                }
                out.join("\n")
            }
            Self::Stats(s) => format!(
                "Journal stats: {} entries (positive: {}, neutral: {}, negative: {}).",
                s.total_entries, s.positive, s.neutral, s.negative
            ),
        }
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum JournalError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("not found: {0}")]
    NotFound(String),
}

#[async_trait]
pub trait JournalSkill: Send + Sync {
    async fn execute(&self, action: &JournalAction) -> Result<JournalResult, JournalError>;
}
