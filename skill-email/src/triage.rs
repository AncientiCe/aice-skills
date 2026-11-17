//! LLM-driven email triage. Buckets a list of `EmailMessage`s into labeled
//! categories (e.g. "needs reply today", "fyi", "newsletter", "spam-likely").

use crate::types::{EmailError, EmailLlm, EmailMessage, TriageBucket};

pub(crate) const TRIAGE_SYSTEM_PROMPT: &str = "You triage emails. Read the user-provided list \
and respond with a single compact JSON object and nothing else. Schema:\n\
{ \"buckets\": [ { \"label\": string, \"messages\": string[] } ] }\n\
Each message in the input is identified by 'id'. The 'messages' array in each bucket must \
contain ids only. Use no more than 6 buckets. Suggested labels: 'needs reply today', \
'meeting / calendar', 'fyi', 'newsletter', 'spam-likely'. Do not wrap the JSON in markdown.";

pub(crate) async fn run_triage(
    llm: &dyn EmailLlm,
    messages: &[EmailMessage],
) -> Result<Vec<TriageBucket>, EmailError> {
    let user_text = format_messages_for_llm(messages);
    let raw = llm
        .complete_json(TRIAGE_SYSTEM_PROMPT, &user_text)
        .await
        .map_err(EmailError::LlmUnavailable)?;
    let cleaned = strip_code_fence(&raw).trim().to_string();
    let v: serde_json::Value = serde_json::from_str(&cleaned)
        .map_err(|e| EmailError::InvalidLlmOutput(format!("not valid json: {e}")))?;
    let buckets = v
        .get("buckets")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::with_capacity(buckets.len());
    for b in buckets {
        let label = b
            .get("label")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if label.is_empty() {
            continue;
        }
        let ids = b
            .get("messages")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let lines: Vec<String> = ids
            .into_iter()
            .filter_map(|id| {
                messages
                    .iter()
                    .find(|m| m.id == id)
                    .map(|m| format!("{}: {}", m.sender, m.subject))
            })
            .collect();
        out.push(TriageBucket {
            label,
            messages: lines,
        });
    }
    Ok(out)
}

fn format_messages_for_llm(messages: &[EmailMessage]) -> String {
    let mut out = Vec::with_capacity(messages.len());
    for m in messages {
        let received = m
            .received_at
            .map(|t| t.strftime("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        out.push(format!(
            "id={} | received={} | from={} | subject={} | snippet={}",
            m.id, received, m.sender, m.subject, m.snippet
        ));
    }
    out.join("\n")
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct StaticLlm {
        response: String,
    }

    #[async_trait]
    impl EmailLlm for StaticLlm {
        async fn complete_json(&self, _system: &str, _user: &str) -> Result<String, String> {
            Ok(self.response.clone())
        }
    }

    fn message(id: &str, subject: &str) -> EmailMessage {
        EmailMessage {
            id: id.to_string(),
            sender: format!("user-{id}@example.com"),
            subject: subject.to_string(),
            snippet: "...".to_string(),
            received_at: None,
            unread: true,
            flagged: false,
            mailbox: None,
        }
    }

    #[tokio::test]
    async fn triage_groups_by_id() {
        let llm = StaticLlm {
            response: r#"{"buckets":[
                {"label":"needs reply today","messages":["1"]},
                {"label":"newsletter","messages":["2"]}
            ]}"#
            .to_string(),
        };
        let msgs = vec![message("1", "Project update"), message("2", "Newsletter")];
        let buckets = run_triage(&llm, &msgs).await.expect("ok");
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].label, "needs reply today");
        assert_eq!(buckets[0].messages.len(), 1);
        assert!(buckets[0].messages[0].contains("Project update"));
    }

    #[tokio::test]
    async fn triage_handles_code_fenced_json() {
        let llm = StaticLlm {
            response: "```json\n{\"buckets\":[]}\n```".to_string(),
        };
        let buckets = run_triage(&llm, &[]).await.expect("ok");
        assert_eq!(buckets.len(), 0);
    }

    #[tokio::test]
    async fn triage_invalid_json_propagates_error() {
        let llm = StaticLlm {
            response: "nope".to_string(),
        };
        let err = run_triage(&llm, &[message("1", "x")])
            .await
            .expect_err("err");
        assert!(matches!(err, EmailError::InvalidLlmOutput(_)));
    }
}
