//! Media skill types and trait.

use async_trait::async_trait;

/// Structured result for LLM to turn into a spoken answer.
#[derive(Clone, Debug)]
pub struct MediaResult {
    pub summary: String,
    pub now_playing: Option<String>,
    pub state: String,
}

impl MediaResult {
    /// Format for inclusion in an LLM prompt.
    pub fn to_prompt_context(&self) -> String {
        match &self.now_playing {
            Some(np) => format!("{}. {}. Now playing: {}.", self.summary, self.state, np),
            None => format!("{}. {}.", self.summary, self.state),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MediaResult;

    #[test]
    fn test_to_prompt_context_with_now_playing() {
        let result = MediaResult {
            summary: "Success".to_string(),
            now_playing: Some("Song".to_string()),
            state: "playing".to_string(),
        };
        assert_eq!(
            result.to_prompt_context(),
            "Success. playing. Now playing: Song."
        );
    }

    #[test]
    fn test_to_prompt_context_without_now_playing() {
        let result = MediaResult {
            summary: "Paused".to_string(),
            now_playing: None,
            state: "paused".to_string(),
        };
        assert_eq!(result.to_prompt_context(), "Paused. paused.");
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum MediaSkillError {
    #[error("auth error: {0}")]
    Auth(String),
    #[error("playback or device error: {0}")]
    Playback(String),
    #[error("no active source or device")]
    NoSource,
    #[error("unsupported action: {0}")]
    UnsupportedAction(String),
}

/// Media skill: play, pause, source, room; action and optional target from intent.
#[async_trait]
pub trait MediaSkill: Send + Sync {
    async fn execute(
        &self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<MediaResult, MediaSkillError>;
}
