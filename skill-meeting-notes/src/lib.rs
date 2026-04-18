//! Meeting notes skill: takes an existing transcript (typically produced by
//! `core-stt` Whisper), asks a local LLM to extract a structured summary +
//! action items, optionally pushes the action items into Reminders via
//! `skill-reminder`.

mod types;

pub use types::{
    ActionItem, MeetingNotesError, MeetingNotesLlm, MeetingNotesQuery, MeetingNotesResult,
    MeetingNotesSkill,
};

use async_trait::async_trait;
use metrics::{counter, histogram};
use skill_reminder::{ReminderSkill, ReminderSkillError};
use std::sync::Arc;
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "meeting_notes";

const SYSTEM_PROMPT: &str = "You are a meeting notes assistant. Read the transcript and respond \
with a single compact JSON object and nothing else. Schema:\n\
{\n  \"title\": string,\n  \"summary\": string,\n  \"key_points\": string[],\n  \
\"decisions\": string[],\n  \"action_items\": [{ \"text\": string, \"assignee\": string|null, \
\"due\": string|null }]\n}\n\
Constraints: summary <= 4 sentences; key_points <= 6; decisions <= 6; action_items <= 10. \
If a field has no items, return an empty array. Do not wrap the JSON in markdown.";

pub struct LlmMeetingNotesSkill {
    llm: Arc<dyn MeetingNotesLlm>,
    reminders: Option<Arc<dyn ReminderSkill>>,
}

impl LlmMeetingNotesSkill {
    pub fn new(llm: Arc<dyn MeetingNotesLlm>) -> Self {
        Self {
            llm,
            reminders: None,
        }
    }

    pub fn with_reminders(mut self, reminders: Arc<dyn ReminderSkill>) -> Self {
        self.reminders = Some(reminders);
        self
    }

    fn error_kind(error: &MeetingNotesError) -> &'static str {
        match error {
            MeetingNotesError::InvalidQuery(_) => "invalid_query",
            MeetingNotesError::LlmUnavailable(_) => "llm_unavailable",
            MeetingNotesError::InvalidLlmOutput(_) => "invalid_llm_output",
            MeetingNotesError::Reminders(_) => "reminders",
        }
    }

    pub fn parse_llm_json(raw: &str) -> Result<ParsedNotes, MeetingNotesError> {
        let cleaned = strip_code_fence(raw).trim().to_string();
        let v: serde_json::Value = serde_json::from_str(&cleaned)
            .map_err(|e| MeetingNotesError::InvalidLlmOutput(format!("not valid json: {e}")))?;
        let title = v
            .get("title")
            .and_then(|s| s.as_str())
            .unwrap_or("Meeting")
            .trim()
            .to_string();
        let summary = v
            .get("summary")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let key_points = string_array(v.get("key_points"));
        let decisions = string_array(v.get("decisions"));
        let action_items = parse_action_items(v.get("action_items"));
        Ok(ParsedNotes {
            title,
            summary,
            key_points,
            decisions,
            action_items,
        })
    }

    async fn create_reminders(&self, items: &[ActionItem]) -> Result<usize, MeetingNotesError> {
        let Some(rems) = &self.reminders else {
            return Ok(0);
        };
        let mut created = 0usize;
        for item in items {
            let when = item.due.as_deref();
            match rems.execute(&item.text, when).await {
                Ok(_) => created += 1,
                Err(ReminderSkillError::InvalidDate(_)) => {
                    if let Err(e) = rems.execute(&item.text, None).await {
                        return Err(MeetingNotesError::Reminders(e.to_string()));
                    }
                    created += 1;
                }
                Err(e) => return Err(MeetingNotesError::Reminders(e.to_string())),
            }
        }
        Ok(created)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedNotes {
    pub title: String,
    pub summary: String,
    pub key_points: Vec<String>,
    pub decisions: Vec<String>,
    pub action_items: Vec<ActionItem>,
}

fn string_array(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn parse_action_items(v: Option<&serde_json::Value>) -> Vec<ActionItem> {
    v.and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let text = item.get("text")?.as_str()?.trim().to_string();
                    if text.is_empty() {
                        return None;
                    }
                    let assignee = item
                        .get("assignee")
                        .and_then(|s| s.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    let due = item
                        .get("due")
                        .and_then(|s| s.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    Some(ActionItem {
                        text,
                        assignee,
                        due,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        return rest.trim_end_matches("```").trim();
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        return rest.trim_end_matches("```").trim();
    }
    trimmed
}

#[async_trait]
impl MeetingNotesSkill for LlmMeetingNotesSkill {
    async fn execute(
        &self,
        query: &MeetingNotesQuery,
    ) -> Result<MeetingNotesResult, MeetingNotesError> {
        let started_at = Instant::now();
        let result = async {
            let transcript = query.transcript.trim();
            if transcript.is_empty() {
                return Err(MeetingNotesError::InvalidQuery(
                    "empty transcript".to_string(),
                ));
            }
            let raw = self
                .llm
                .complete_json(SYSTEM_PROMPT, transcript)
                .await
                .map_err(MeetingNotesError::LlmUnavailable)?;
            let parsed = Self::parse_llm_json(&raw)?;
            let title = match query.title.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(t) => t.trim().to_string(),
                None => parsed.title.clone(),
            };
            let reminders_created = if query.create_reminders {
                self.create_reminders(&parsed.action_items).await?
            } else {
                0
            };
            Ok(MeetingNotesResult {
                title,
                summary: parsed.summary,
                key_points: parsed.key_points,
                decisions: parsed.decisions,
                action_items: parsed.action_items,
                reminders_created,
            })
        }
        .await;

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

pub struct MockMeetingNotesSkill {
    pub result: Result<MeetingNotesResult, MeetingNotesError>,
}

impl MockMeetingNotesSkill {
    pub fn ok(result: MeetingNotesResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: MeetingNotesError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl MeetingNotesSkill for MockMeetingNotesSkill {
    async fn execute(
        &self,
        _query: &MeetingNotesQuery,
    ) -> Result<MeetingNotesResult, MeetingNotesError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_reminder::{ReminderResult, ReminderSkill};
    use std::sync::Mutex;

    struct StaticLlm {
        response: Result<String, String>,
    }

    #[async_trait]
    impl MeetingNotesLlm for StaticLlm {
        async fn complete_json(&self, _system: &str, _user: &str) -> Result<String, String> {
            self.response.clone()
        }
    }

    #[derive(Default)]
    struct RecordingReminder {
        calls: Mutex<Vec<(String, Option<String>)>>,
    }

    #[async_trait]
    impl ReminderSkill for RecordingReminder {
        async fn execute(
            &self,
            title: &str,
            when: Option<&str>,
        ) -> Result<ReminderResult, ReminderSkillError> {
            self.calls
                .lock()
                .expect("calls mutex poisoned")
                .push((title.to_string(), when.map(str::to_string)));
            Ok(ReminderResult {
                summary: "ok".to_string(),
                title: title.to_string(),
                when: when.map(str::to_string),
            })
        }
    }

    fn good_response() -> String {
        r#"{
            "title": "Standup",
            "summary": "Discussed sprint progress.",
            "key_points": ["Backend almost ready", "Frontend blocked on API"],
            "decisions": ["Ship beta on Friday"],
            "action_items": [
                { "text": "Write release notes", "assignee": "Alice", "due": "Friday" },
                { "text": "Fix login bug", "assignee": null, "due": null }
            ]
        }"#
        .to_string()
    }

    #[tokio::test]
    async fn execute_extracts_structure() {
        let llm = Arc::new(StaticLlm {
            response: Ok(good_response()),
        });
        let skill = LlmMeetingNotesSkill::new(llm);
        let res = skill
            .execute(&MeetingNotesQuery {
                transcript: "alice: ship beta on friday. bob: i'll write the notes".into(),
                title: None,
                create_reminders: false,
            })
            .await
            .expect("ok");
        assert_eq!(res.title, "Standup");
        assert_eq!(res.action_items.len(), 2);
        assert_eq!(res.action_items[0].assignee.as_deref(), Some("Alice"));
        assert_eq!(res.reminders_created, 0);
    }

    #[tokio::test]
    async fn execute_creates_reminders_when_requested() {
        let llm = Arc::new(StaticLlm {
            response: Ok(good_response()),
        });
        let recorder = Arc::new(RecordingReminder::default());
        let skill = LlmMeetingNotesSkill::new(llm).with_reminders(recorder.clone());
        let res = skill
            .execute(&MeetingNotesQuery {
                transcript: "...".into(),
                title: Some("Override Title".into()),
                create_reminders: true,
            })
            .await
            .expect("ok");
        assert_eq!(res.title, "Override Title");
        assert_eq!(res.reminders_created, 2);
        let calls = recorder.calls.lock().expect("calls");
        assert_eq!(calls[0].0, "Write release notes");
    }

    #[tokio::test]
    async fn execute_rejects_empty_transcript() {
        let llm = Arc::new(StaticLlm {
            response: Ok(good_response()),
        });
        let skill = LlmMeetingNotesSkill::new(llm);
        let err = skill
            .execute(&MeetingNotesQuery {
                transcript: "  ".into(),
                title: None,
                create_reminders: false,
            })
            .await
            .expect_err("err");
        assert!(matches!(err, MeetingNotesError::InvalidQuery(_)));
    }

    #[tokio::test]
    async fn execute_handles_code_fenced_json() {
        let llm = Arc::new(StaticLlm {
            response: Ok(format!("```json\n{}\n```", good_response())),
        });
        let skill = LlmMeetingNotesSkill::new(llm);
        let res = skill
            .execute(&MeetingNotesQuery {
                transcript: "x".into(),
                title: None,
                create_reminders: false,
            })
            .await
            .expect("ok");
        assert_eq!(res.title, "Standup");
    }

    #[tokio::test]
    async fn execute_propagates_invalid_json() {
        let llm = Arc::new(StaticLlm {
            response: Ok("totally not json".into()),
        });
        let skill = LlmMeetingNotesSkill::new(llm);
        let err = skill
            .execute(&MeetingNotesQuery {
                transcript: "x".into(),
                title: None,
                create_reminders: false,
            })
            .await
            .expect_err("err");
        assert!(matches!(err, MeetingNotesError::InvalidLlmOutput(_)));
    }
}
