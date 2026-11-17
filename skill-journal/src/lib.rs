//! Voice-journal skill.
//!
//! - `Add` — append a timestamped entry; sentiment auto-classified if not
//!   provided.
//! - `Recall` — fetch entries by time window and/or substring.
//! - `Stats` — aggregate sentiment counts over a window.
//!
//! Storage is local SQLite (one file owned by the user).

mod sentiment;
mod store;
mod types;

pub use store::SqliteJournalStore;
pub use types::{
    JournalAction, JournalEntry, JournalError, JournalResult, JournalSkill, JournalStats, Sentiment,
};

use async_trait::async_trait;
use jiff::Timestamp;
use metrics::{counter, histogram};
use std::sync::Arc;
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "journal";

pub struct LocalJournalSkill {
    store: Arc<SqliteJournalStore>,
}

impl LocalJournalSkill {
    pub fn new(store: Arc<SqliteJournalStore>) -> Self {
        Self { store }
    }

    fn error_kind(error: &JournalError) -> &'static str {
        match error {
            JournalError::InvalidQuery(_) => "invalid_query",
            JournalError::Storage(_) => "storage",
            JournalError::NotFound(_) => "not_found",
        }
    }
}

#[async_trait]
impl JournalSkill for LocalJournalSkill {
    async fn execute(&self, action: &JournalAction) -> Result<JournalResult, JournalError> {
        let started_at = Instant::now();
        let result = match action {
            JournalAction::Add {
                text,
                sentiment,
                tags,
            } => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    Err(JournalError::InvalidQuery("empty entry text".to_string()))
                } else {
                    let s = sentiment.unwrap_or_else(|| sentiment::classify(trimmed));
                    let entry = self.store.insert(trimmed, s, tags, Timestamp::now())?;
                    Ok(JournalResult::Added(entry))
                }
            }
            JournalAction::Recall {
                from,
                to,
                contains,
                limit,
            } => {
                let lim = (*limit).clamp(1, 200);
                let entries = self.store.recall(*from, *to, contains.as_deref(), lim)?;
                Ok(JournalResult::Entries(entries))
            }
            JournalAction::Stats { from, to } => {
                let stats = self.store.stats(*from, *to)?;
                Ok(JournalResult::Stats(stats))
            }
        };

        let mut result_label = "success";
        let mut error_kind = "none";
        if let Err(e) = &result {
            result_label = "error";
            error_kind = Self::error_kind(e);
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

pub struct MockJournalSkill {
    pub result: Result<JournalResult, JournalError>,
}

impl MockJournalSkill {
    pub fn ok(result: JournalResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: JournalError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl JournalSkill for MockJournalSkill {
    async fn execute(&self, _action: &JournalAction) -> Result<JournalResult, JournalError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill() -> LocalJournalSkill {
        let store = Arc::new(SqliteJournalStore::open_in_memory().expect("memory db"));
        LocalJournalSkill::new(store)
    }

    #[tokio::test]
    async fn add_persists_entry_with_auto_sentiment() {
        let s = skill();
        let res = s
            .execute(&JournalAction::Add {
                text: "Today was a great and happy day.".into(),
                sentiment: None,
                tags: vec!["work".to_string()],
            })
            .await
            .expect("ok");
        match res {
            JournalResult::Added(e) => {
                assert_eq!(e.sentiment, Sentiment::Positive);
                assert_eq!(e.tags, vec!["work".to_string()]);
            }
            _ => panic!("expected Added"),
        }
    }

    #[tokio::test]
    async fn add_rejects_empty_text() {
        let s = skill();
        let err = s
            .execute(&JournalAction::Add {
                text: "   ".into(),
                sentiment: None,
                tags: Vec::new(),
            })
            .await
            .expect_err("err");
        assert!(matches!(err, JournalError::InvalidQuery(_)));
    }

    #[tokio::test]
    async fn recall_returns_recent_first() {
        let s = skill();
        for text in ["one happy", "two awful", "three calm"] {
            s.execute(&JournalAction::Add {
                text: text.into(),
                sentiment: None,
                tags: Vec::new(),
            })
            .await
            .expect("ok");
        }
        let res = s
            .execute(&JournalAction::Recall {
                from: None,
                to: None,
                contains: None,
                limit: 10,
            })
            .await
            .expect("ok");
        match res {
            JournalResult::Entries(entries) => {
                assert_eq!(entries.len(), 3);
                assert!(entries[0].text.contains("three"));
            }
            _ => panic!("expected entries"),
        }
    }

    #[tokio::test]
    async fn recall_filters_by_substring() {
        let s = skill();
        for text in ["walked the dog", "wrote some code", "ran a marathon"] {
            s.execute(&JournalAction::Add {
                text: text.into(),
                sentiment: None,
                tags: Vec::new(),
            })
            .await
            .expect("ok");
        }
        let res = s
            .execute(&JournalAction::Recall {
                from: None,
                to: None,
                contains: Some("code".into()),
                limit: 10,
            })
            .await
            .expect("ok");
        match res {
            JournalResult::Entries(entries) => {
                assert_eq!(entries.len(), 1);
                assert!(entries[0].text.contains("code"));
            }
            _ => panic!("expected entries"),
        }
    }

    #[tokio::test]
    async fn stats_counts_buckets() {
        let s = skill();
        let cases = [
            ("happy day", Some(Sentiment::Positive)),
            ("good morning", Some(Sentiment::Positive)),
            ("awful evening", Some(Sentiment::Negative)),
            ("normal day", Some(Sentiment::Neutral)),
        ];
        for (t, sent) in cases {
            s.execute(&JournalAction::Add {
                text: t.into(),
                sentiment: sent,
                tags: Vec::new(),
            })
            .await
            .expect("ok");
        }
        let res = s
            .execute(&JournalAction::Stats {
                from: None,
                to: None,
            })
            .await
            .expect("ok");
        match res {
            JournalResult::Stats(s) => {
                assert_eq!(s.total_entries, 4);
                assert_eq!(s.positive, 2);
                assert_eq!(s.neutral, 1);
                assert_eq!(s.negative, 1);
            }
            _ => panic!("expected stats"),
        }
    }
}
