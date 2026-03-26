//! Record window skill types and trait.

use async_trait::async_trait;
use std::path::PathBuf;

/// Structured result used by the answer composer.
#[derive(Clone, Debug)]
pub struct RecordWindowResult {
    pub path: PathBuf,
    pub app_name: String,
    pub duration_secs: Option<u64>,
}

impl RecordWindowResult {
    /// Format for inclusion in an LLM prompt.
    pub fn to_prompt_context(&self) -> String {
        let duration = self
            .duration_secs
            .map(|secs| format!("{secs}s"))
            .unwrap_or_else(|| "until manually stopped".to_string());
        format!(
            "Recorded {} to {} for {}.",
            self.app_name,
            self.path.display(),
            duration
        )
    }
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum RecordWindowSkillError {
    #[error("app not found: {0}")]
    AppNotFound(String),
    #[error("window bounds unavailable for app: {0}")]
    WindowBoundsUnavailable(String),
    #[error(
        "audio device not found: {0}. Install blackhole-2ch and configure Multi-Output + Aggregate Device in Audio MIDI Setup"
    )]
    AudioDeviceNotFound(String),
    #[error("ffmpeg not found in PATH. Install it with: brew install ffmpeg")]
    FfmpegNotFound,
    #[error("execution error: {0}")]
    Execution(String),
}

/// Record window skill: capture a specific app window on macOS with audio.
#[async_trait]
pub trait RecordWindowSkill: Send + Sync {
    async fn execute(
        &self,
        app_name: Option<&str>,
        filename: Option<&str>,
        duration_secs: Option<u64>,
    ) -> Result<RecordWindowResult, RecordWindowSkillError>;
}
