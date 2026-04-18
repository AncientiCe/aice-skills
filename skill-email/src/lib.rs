//! Email skill with two providers and shared LLM triage:
//!
//! - `AppleMailEmailSkill` — macOS-only AppleScript bridge to Mail.app.
//! - `ImapEmailSkill` — cross-platform; delegates IMAP I/O to a runtime-
//!   provided `ImapFetcher`, keeping this crate IMAP-client-agnostic.
//!
//! Both implement the same `EmailSkill` trait so the runtime can pick one
//! (or run both — e.g. Apple Mail for unread + IMAP for archive search).

mod apple;
mod imap;
mod triage;
mod types;

pub use apple::AppleMailEmailSkill;
pub use imap::ImapEmailSkill;
pub use types::{
    EmailAction, EmailError, EmailLlm, EmailMessage, EmailQuery, EmailResult, EmailSkill,
    ImapFetcher, TriageBucket,
};

use async_trait::async_trait;

pub struct MockEmailSkill {
    pub result: Result<EmailResult, EmailError>,
}

impl MockEmailSkill {
    pub fn ok(result: EmailResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: EmailError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl EmailSkill for MockEmailSkill {
    async fn execute(&self, _query: &EmailQuery) -> Result<EmailResult, EmailError> {
        self.result.clone()
    }
}
