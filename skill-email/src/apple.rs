//! Apple Mail.app provider via AppleScript.

use crate::triage::run_triage;
use crate::types::{
    EmailAction, EmailError, EmailLlm, EmailMessage, EmailQuery, EmailResult, EmailSkill,
};
use async_trait::async_trait;
use jiff::{tz::TimeZone, Timestamp};
use metrics::{counter, histogram};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "email_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "email_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "apple_mail";

#[derive(Clone)]
pub struct AppleMailEmailSkill {
    dry_run: bool,
    llm: Option<Arc<dyn EmailLlm>>,
}

impl AppleMailEmailSkill {
    pub fn new() -> Self {
        Self {
            dry_run: false,
            llm: None,
        }
    }

    pub fn new_for_tests() -> Self {
        Self {
            dry_run: true,
            llm: None,
        }
    }

    pub fn with_llm(mut self, llm: Arc<dyn EmailLlm>) -> Self {
        self.llm = Some(llm);
        self
    }

    fn escape_applescript_string(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn run_script(&self, script: &str) -> Result<String, EmailError> {
        if self.dry_run {
            return Ok(String::new());
        }
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| EmailError::ProviderUnavailable(e.to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(EmailError::ProviderUnavailable(if stderr.is_empty() {
                "osascript command failed".to_string()
            } else {
                stderr
            }));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub(crate) fn build_unread_script(limit: usize, mailbox: Option<&str>) -> String {
        let mailbox_clause = match mailbox {
            Some(m) if !m.trim().is_empty() => format!(
                "set targetMailbox to mailbox \"{}\" of inbox",
                Self::escape_applescript_string(m)
            ),
            _ => "set targetMailbox to inbox".to_string(),
        };
        format!(
            "set output to \"\"\n\
             set lim to {limit}\n\
             tell application \"Mail\"\n\
             {mailbox_clause}\n\
             set msgs to (messages of targetMailbox whose read status is false)\n\
             set i to 0\n\
             repeat with m in msgs\n\
             if i is greater than or equal to lim then exit repeat\n\
             set i to i + 1\n\
             set s to sender of m\n\
             set sj to subject of m\n\
             set dr to date received of m\n\
             try\n\
             set sn to (extract content of m)\n\
             on error\n\
             set sn to \"\"\n\
             end try\n\
             set output to output & (id of m as text) & \"|\" & s & \"|\" & sj & \"|\" & sn & \"|\" & dr & linefeed\n\
             end repeat\n\
             end tell\n\
             return output"
        )
    }

    pub(crate) fn build_inbox_script(limit: usize) -> String {
        format!(
            "set output to \"\"\n\
             set lim to {limit}\n\
             tell application \"Mail\"\n\
             set msgs to messages of inbox\n\
             set i to 0\n\
             repeat with m in msgs\n\
             if i is greater than or equal to lim then exit repeat\n\
             set i to i + 1\n\
             set s to sender of m\n\
             set sj to subject of m\n\
             set dr to date received of m\n\
             set unread to (read status of m is false)\n\
             try\n\
             set sn to (extract content of m)\n\
             on error\n\
             set sn to \"\"\n\
             end try\n\
             set output to output & (id of m as text) & \"|\" & s & \"|\" & sj & \"|\" & sn & \"|\" & dr & \"|\" & (unread as text) & linefeed\n\
             end repeat\n\
             end tell\n\
             return output"
        )
    }
}

impl Default for AppleMailEmailSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmailSkill for AppleMailEmailSkill {
    async fn execute(&self, query: &EmailQuery) -> Result<EmailResult, EmailError> {
        let started_at = Instant::now();
        let result = async {
            if !cfg!(target_os = "macos") && !self.dry_run {
                return Err(EmailError::Unsupported(
                    "apple mail provider requires macOS".to_string(),
                ));
            }
            match &query.action {
                EmailAction::ListUnread { limit } => {
                    let raw = self
                        .run_script(&Self::build_unread_script(*limit, query.mailbox.as_deref()))?;
                    let messages = parse_unread_lines(&raw, query.mailbox.as_deref());
                    Ok(EmailResult {
                        summary: format!("{} unread message(s)", messages.len()),
                        messages,
                        triage_buckets: Vec::new(),
                    })
                }
                EmailAction::ListInbox { limit } => {
                    let raw = self.run_script(&Self::build_inbox_script(*limit))?;
                    let messages = parse_inbox_lines(&raw, query.mailbox.as_deref());
                    Ok(EmailResult {
                        summary: format!("{} message(s) in inbox", messages.len()),
                        messages,
                        triage_buckets: Vec::new(),
                    })
                }
                EmailAction::Search { query: q, limit: _ } => Err(EmailError::Unsupported(
                    format!("apple mail search not implemented (query: {q})"),
                )),
                EmailAction::Triage { limit } => {
                    let raw = self
                        .run_script(&Self::build_unread_script(*limit, query.mailbox.as_deref()))?;
                    let messages = parse_unread_lines(&raw, query.mailbox.as_deref());
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

pub(crate) fn error_kind_str(error: &EmailError) -> &'static str {
    match error {
        EmailError::InvalidQuery(_) => "invalid_query",
        EmailError::Auth(_) => "auth",
        EmailError::ProviderUnavailable(_) => "provider_unavailable",
        EmailError::NotFound(_) => "not_found",
        EmailError::Unsupported(_) => "unsupported",
        EmailError::LlmUnavailable(_) => "llm_unavailable",
        EmailError::InvalidLlmOutput(_) => "invalid_llm_output",
    }
}

fn parse_unread_lines(raw: &str, mailbox: Option<&str>) -> Vec<EmailMessage> {
    let mut out = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            continue;
        }
        out.push(EmailMessage {
            id: parts[0].trim().to_string(),
            sender: parts[1].trim().to_string(),
            subject: parts[2].trim().to_string(),
            snippet: snippet(parts[3]),
            received_at: parts.get(4).and_then(|s| parse_apple_date(s.trim())),
            unread: true,
            flagged: false,
            mailbox: mailbox.map(str::to_string),
        });
    }
    out
}

fn parse_inbox_lines(raw: &str, mailbox: Option<&str>) -> Vec<EmailMessage> {
    let mut out = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            continue;
        }
        let unread = parts
            .get(5)
            .map(|s| s.trim().eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        out.push(EmailMessage {
            id: parts[0].trim().to_string(),
            sender: parts[1].trim().to_string(),
            subject: parts[2].trim().to_string(),
            snippet: snippet(parts[3]),
            received_at: parts.get(4).and_then(|s| parse_apple_date(s.trim())),
            unread,
            flagged: false,
            mailbox: mailbox.map(str::to_string),
        });
    }
    out
}

fn snippet(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.chars().count() <= 200 {
        trimmed.to_string()
    } else {
        let truncated: String = trimmed.chars().take(197).collect();
        format!("{truncated}...")
    }
}

fn parse_apple_date(s: &str) -> Option<Timestamp> {
    let cleaned = s.replace(" at ", " ");
    for fmt in [
        "%A, %B %d, %Y %I:%M:%S %p",
        "%a %b %d %H:%M:%S %Y",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(datetime) =
            jiff::fmt::strtime::parse(fmt, &cleaned).and_then(|tm| tm.to_datetime())
        {
            return datetime
                .to_zoned_datetime(TimeZone::UTC)
                .ok()
                .map(|z| z.timestamp());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> Timestamp {
        s.parse().expect("timestamp")
    }

    #[test]
    fn parse_apple_date_reads_every_supported_layout_as_utc() {
        let expected = ts("2026-04-18T09:05:00Z");
        for raw in [
            "Saturday, April 18, 2026 at 9:05:00 AM",
            "Sat Apr 18 09:05:00 2026",
            "2026-04-18 09:05:00",
            "2026-04-18T09:05:00",
        ] {
            assert_eq!(parse_apple_date(raw), Some(expected), "{raw}");
        }
        assert_eq!(
            parse_apple_date("Saturday, April 18, 2026 at 9:05:00 PM"),
            Some(ts("2026-04-18T21:05:00Z"))
        );
        assert_eq!(parse_apple_date("not a date"), None);
    }

    #[tokio::test]
    async fn dry_run_unread_returns_zero() {
        let skill = AppleMailEmailSkill::new_for_tests();
        let res = skill
            .execute(&EmailQuery {
                action: EmailAction::ListUnread { limit: 5 },
                mailbox: None,
            })
            .await
            .expect("ok");
        assert_eq!(res.messages.len(), 0);
    }

    #[test]
    fn build_unread_script_includes_mailbox() {
        let s = AppleMailEmailSkill::build_unread_script(3, Some("Work"));
        assert!(s.contains("mailbox \"Work\""));
        assert!(s.contains("set lim to 3"));
    }

    #[test]
    fn snippet_truncates_long_text() {
        let long = "a".repeat(500);
        let out = snippet(&long);
        assert_eq!(out.chars().count(), 200);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn parse_unread_lines_parses_pipe_format() {
        let raw = "id-1|alice@example.com|Hello|short body|2026-04-18 09:00:00\n";
        let msgs = parse_unread_lines(raw, Some("INBOX"));
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender, "alice@example.com");
        assert!(msgs[0].unread);
        assert_eq!(msgs[0].mailbox.as_deref(), Some("INBOX"));
    }
}
