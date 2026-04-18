//! Types and traits for the meeting-notes skill.

use async_trait::async_trait;

#[derive(Clone, Debug, PartialEq)]
pub struct MeetingNotesQuery {
    pub transcript: String,
    pub title: Option<String>,
    pub create_reminders: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionItem {
    pub text: String,
    pub assignee: Option<String>,
    pub due: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeetingNotesResult {
    pub title: String,
    pub summary: String,
    pub key_points: Vec<String>,
    pub decisions: Vec<String>,
    pub action_items: Vec<ActionItem>,
    pub reminders_created: usize,
}

impl MeetingNotesResult {
    pub fn to_prompt_context(&self) -> String {
        let mut out = vec![format!("Meeting: {}", self.title), self.summary.clone()];
        if !self.key_points.is_empty() {
            out.push("Key points:".to_string());
            for p in &self.key_points {
                out.push(format!("- {p}"));
            }
        }
        if !self.decisions.is_empty() {
            out.push("Decisions:".to_string());
            for d in &self.decisions {
                out.push(format!("- {d}"));
            }
        }
        if !self.action_items.is_empty() {
            out.push("Action items:".to_string());
            for a in &self.action_items {
                let who = a
                    .assignee
                    .as_deref()
                    .map(|w| format!(" ({w})"))
                    .unwrap_or_default();
                let when = a
                    .due
                    .as_deref()
                    .map(|w| format!(" — due {w}"))
                    .unwrap_or_default();
                out.push(format!("- {}{who}{when}", a.text));
            }
        }
        if self.reminders_created > 0 {
            out.push(format!("Created {} reminder(s).", self.reminders_created));
        }
        out.join("\n")
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum MeetingNotesError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("llm unavailable: {0}")]
    LlmUnavailable(String),
    #[error("llm produced unparseable output: {0}")]
    InvalidLlmOutput(String),
    #[error("reminders backend error: {0}")]
    Reminders(String),
}

#[async_trait]
pub trait MeetingNotesSkill: Send + Sync {
    async fn execute(
        &self,
        query: &MeetingNotesQuery,
    ) -> Result<MeetingNotesResult, MeetingNotesError>;
}

/// Adapter trait the runtime implements over its concrete LLM transport.
#[async_trait]
pub trait MeetingNotesLlm: Send + Sync {
    async fn complete_json(&self, system_prompt: &str, user_text: &str) -> Result<String, String>;
}
