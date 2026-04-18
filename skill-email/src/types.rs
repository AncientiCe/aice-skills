//! Email skill types and traits shared across providers (Apple Mail, IMAP).

use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq)]
pub struct EmailMessage {
    pub id: String,
    pub sender: String,
    pub subject: String,
    pub snippet: String,
    pub received_at: Option<DateTime<Utc>>,
    pub unread: bool,
    pub flagged: bool,
    pub mailbox: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EmailAction {
    ListUnread { limit: usize },
    ListInbox { limit: usize },
    Search { query: String, limit: usize },
    Triage { limit: usize },
}

impl EmailAction {
    pub fn parse(action: &str, query: Option<&str>, limit: Option<usize>) -> Option<Self> {
        let l = limit.unwrap_or(10).clamp(1, 50);
        match action.trim().to_lowercase().as_str() {
            "list_unread" | "unread" => Some(Self::ListUnread { limit: l }),
            "list_inbox" | "inbox" | "list" => Some(Self::ListInbox { limit: l }),
            "search" => Some(Self::Search {
                query: query.unwrap_or("").trim().to_string(),
                limit: l,
            }),
            "triage" => Some(Self::Triage { limit: l }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmailQuery {
    pub action: EmailAction,
    pub mailbox: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TriageBucket {
    pub label: String,
    pub messages: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmailResult {
    pub summary: String,
    pub messages: Vec<EmailMessage>,
    pub triage_buckets: Vec<TriageBucket>,
}

impl EmailResult {
    pub fn to_prompt_context(&self) -> String {
        let mut out = vec![self.summary.clone()];
        for m in &self.messages {
            let when = m
                .received_at
                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "unknown time".to_string());
            let unread = if m.unread { " [unread]" } else { "" };
            out.push(format!(
                "- {when}{unread} from {}: {} — {}",
                m.sender, m.subject, m.snippet
            ));
        }
        for bucket in &self.triage_buckets {
            out.push(format!("[{}]", bucket.label));
            for line in &bucket.messages {
                out.push(format!("  - {line}"));
            }
        }
        out.join("\n")
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum EmailError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("auth error: {0}")]
    Auth(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unsupported action: {0}")]
    Unsupported(String),
    #[error("llm unavailable: {0}")]
    LlmUnavailable(String),
    #[error("invalid llm output: {0}")]
    InvalidLlmOutput(String),
}

#[async_trait]
pub trait EmailSkill: Send + Sync {
    async fn execute(&self, query: &EmailQuery) -> Result<EmailResult, EmailError>;
}

/// Adapter trait the runtime implements over its concrete LLM transport. Used
/// by triage to bucket messages.
#[async_trait]
pub trait EmailLlm: Send + Sync {
    async fn complete_json(&self, system_prompt: &str, user_text: &str) -> Result<String, String>;
}

/// Generic IMAP fetcher trait owned by the runtime. Keeps this crate
/// dependency-free (no IMAP client crate) so we can plug in any backend.
#[async_trait]
pub trait ImapFetcher: Send + Sync {
    async fn list_unread(
        &self,
        mailbox: Option<&str>,
        limit: usize,
    ) -> Result<Vec<EmailMessage>, EmailError>;

    async fn list_inbox(
        &self,
        mailbox: Option<&str>,
        limit: usize,
    ) -> Result<Vec<EmailMessage>, EmailError>;

    async fn search(
        &self,
        mailbox: Option<&str>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<EmailMessage>, EmailError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_handles_aliases_and_clamps_limit() {
        assert_eq!(
            EmailAction::parse("unread", None, None),
            Some(EmailAction::ListUnread { limit: 10 })
        );
        assert_eq!(
            EmailAction::parse("inbox", None, Some(99)),
            Some(EmailAction::ListInbox { limit: 50 })
        );
        assert_eq!(
            EmailAction::parse("inbox", None, Some(0)),
            Some(EmailAction::ListInbox { limit: 1 })
        );
        assert_eq!(
            EmailAction::parse("search", Some("urgent"), Some(5)),
            Some(EmailAction::Search {
                query: "urgent".to_string(),
                limit: 5
            })
        );
        assert_eq!(EmailAction::parse("nope", None, None), None);
    }
}
