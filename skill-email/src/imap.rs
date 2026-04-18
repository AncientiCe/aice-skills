//! Cross-platform IMAP-backed email skill. The actual IMAP I/O is delegated
//! to a runtime-provided `ImapFetcher` so this crate stays
//! IMAP-client-agnostic and easy to test.

use crate::apple::error_kind_str;
use crate::triage::run_triage;
use crate::types::{
    EmailAction, EmailError, EmailLlm, EmailQuery, EmailResult, EmailSkill, ImapFetcher,
};
use async_trait::async_trait;
use metrics::{counter, histogram};
use std::sync::Arc;
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "email_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "email_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "imap";

#[derive(Clone)]
pub struct ImapEmailSkill {
    fetcher: Arc<dyn ImapFetcher>,
    llm: Option<Arc<dyn EmailLlm>>,
}

impl ImapEmailSkill {
    pub fn new(fetcher: Arc<dyn ImapFetcher>) -> Self {
        Self { fetcher, llm: None }
    }

    pub fn with_llm(mut self, llm: Arc<dyn EmailLlm>) -> Self {
        self.llm = Some(llm);
        self
    }
}

#[async_trait]
impl EmailSkill for ImapEmailSkill {
    async fn execute(&self, query: &EmailQuery) -> Result<EmailResult, EmailError> {
        let started_at = Instant::now();
        let result = async {
            match &query.action {
                EmailAction::ListUnread { limit } => {
                    let messages = self
                        .fetcher
                        .list_unread(query.mailbox.as_deref(), *limit)
                        .await?;
                    Ok(EmailResult {
                        summary: format!("{} unread message(s)", messages.len()),
                        messages,
                        triage_buckets: Vec::new(),
                    })
                }
                EmailAction::ListInbox { limit } => {
                    let messages = self
                        .fetcher
                        .list_inbox(query.mailbox.as_deref(), *limit)
                        .await?;
                    Ok(EmailResult {
                        summary: format!("{} message(s) in inbox", messages.len()),
                        messages,
                        triage_buckets: Vec::new(),
                    })
                }
                EmailAction::Search { query: q, limit } => {
                    let messages = self
                        .fetcher
                        .search(query.mailbox.as_deref(), q, *limit)
                        .await?;
                    Ok(EmailResult {
                        summary: format!("{} match(es) for '{q}'", messages.len()),
                        messages,
                        triage_buckets: Vec::new(),
                    })
                }
                EmailAction::Triage { limit } => {
                    let messages = self
                        .fetcher
                        .list_unread(query.mailbox.as_deref(), *limit)
                        .await?;
                    let llm = self.llm.as_ref().ok_or_else(|| {
                        EmailError::InvalidQuery("triage requires an llm".to_string())
                    })?;
                    let buckets = if messages.is_empty() {
                        Vec::new()
                    } else {
                        run_triage(llm.as_ref(), &messages).await?
                    };
                    Ok(EmailResult {
                        summary: format!(
                            "Triaged {} unread message(s) into {} bucket(s)",
                            messages.len(),
                            buckets.len()
                        ),
                        messages,
                        triage_buckets: buckets,
                    })
                }
            }
        }
        .await;

        let mut result_label = "success";
        let mut error_kind = "none";
        if let Err(e) = &result {
            result_label = "error";
            error_kind = error_kind_str(e);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EmailMessage;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct StubFetcher {
        messages: Mutex<Vec<EmailMessage>>,
    }

    #[async_trait]
    impl ImapFetcher for StubFetcher {
        async fn list_unread(
            &self,
            _mailbox: Option<&str>,
            _limit: usize,
        ) -> Result<Vec<EmailMessage>, EmailError> {
            Ok(self.messages.lock().expect("messages").clone())
        }
        async fn list_inbox(
            &self,
            _mailbox: Option<&str>,
            _limit: usize,
        ) -> Result<Vec<EmailMessage>, EmailError> {
            Ok(self.messages.lock().expect("messages").clone())
        }
        async fn search(
            &self,
            _mailbox: Option<&str>,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<EmailMessage>, EmailError> {
            Ok(self.messages.lock().expect("messages").clone())
        }
    }

    fn message(id: &str, sender: &str) -> EmailMessage {
        EmailMessage {
            id: id.to_string(),
            sender: sender.to_string(),
            subject: format!("subject-{id}"),
            snippet: "snip".to_string(),
            received_at: None,
            unread: true,
            flagged: false,
            mailbox: None,
        }
    }

    #[tokio::test]
    async fn list_unread_summary_includes_count() {
        let fetcher = Arc::new(StubFetcher {
            messages: Mutex::new(vec![message("1", "a"), message("2", "b")]),
        });
        let skill = ImapEmailSkill::new(fetcher);
        let res = skill
            .execute(&EmailQuery {
                action: EmailAction::ListUnread { limit: 10 },
                mailbox: None,
            })
            .await
            .expect("ok");
        assert!(res.summary.contains("2"));
        assert_eq!(res.messages.len(), 2);
    }

    #[tokio::test]
    async fn triage_requires_llm() {
        let fetcher = Arc::new(StubFetcher {
            messages: Mutex::new(vec![message("1", "a")]),
        });
        let skill = ImapEmailSkill::new(fetcher);
        let err = skill
            .execute(&EmailQuery {
                action: EmailAction::Triage { limit: 10 },
                mailbox: None,
            })
            .await
            .expect_err("err");
        assert!(matches!(err, EmailError::InvalidQuery(_)));
    }
}
