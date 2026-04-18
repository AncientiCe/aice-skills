//! Timer skill types and trait.

use crate::registry::TimerStatus;
use async_trait::async_trait;

/// Structured result for LLM to turn into a spoken answer.
#[derive(Clone, Debug)]
pub struct TimerResult {
    pub summary: String,
    pub timer_name: String,
    pub duration_display: String,
    pub duration_seconds: u64,
}

impl TimerResult {
    /// Format for inclusion in an LLM prompt.
    pub fn to_prompt_context(&self) -> String {
        format!(
            "{}. Timer '{}' started for {}.",
            self.summary, self.timer_name, self.duration_display
        )
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum TimerSkillError {
    #[error("execution error: {0}")]
    Execution(String),
    #[error("invalid duration: {0}")]
    InvalidDuration(String),
    #[error("timer app unavailable")]
    Unavailable,
}

/// Timer skill: create timers (macOS Clock, in-process, etc.) and inspect or
/// cancel active ones. `duration` is required; `name` is optional and an
/// ordinal label is derived from active timers when omitted.
///
/// `list`, `remaining`, and `cancel` have default implementations that return
/// empty/None/false so older mock implementations continue to compile.
#[async_trait]
pub trait TimerSkill: Send + Sync {
    async fn execute(
        &self,
        duration: &str,
        name: Option<&str>,
    ) -> Result<TimerResult, TimerSkillError>;

    /// Return a snapshot of all currently-active timers, sorted by remaining
    /// time ascending. Default implementation returns no timers.
    async fn list(&self) -> Vec<TimerStatus> {
        Vec::new()
    }

    /// Return the live status for a single timer if it exists. Default returns
    /// `None`.
    async fn remaining(&self, _name: &str) -> Option<TimerStatus> {
        None
    }

    /// Cancel an active timer. Returns `true` if the timer existed and was
    /// removed. Default returns `false`.
    async fn cancel(&self, _name: &str) -> bool {
        false
    }
}
