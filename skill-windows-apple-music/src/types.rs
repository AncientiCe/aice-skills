use async_trait::async_trait;

#[derive(Clone, Debug)]
pub struct WindowsAppleMusicResult {
    pub summary: String,
    pub now_playing: Option<String>,
    pub state: String,
}

impl WindowsAppleMusicResult {
    pub fn to_prompt_context(&self) -> String {
        match &self.now_playing {
            Some(track) => format!("{}. {}. Now playing: {}.", self.summary, self.state, track),
            None => format!("{}. {}.", self.summary, self.state),
        }
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum WindowsAppleMusicSkillError {
    #[error("auth error: {0}")]
    Auth(String),
    #[error("playback or device error: {0}")]
    Playback(String),
    #[error("no active source or device")]
    NoSource,
    #[error("unsupported action: {0}")]
    UnsupportedAction(String),
}

#[async_trait]
pub trait WindowsAppleMusic: Send + Sync {
    async fn execute(
        &self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<WindowsAppleMusicResult, WindowsAppleMusicSkillError>;
}

#[cfg(test)]
mod tests {
    use super::WindowsAppleMusicResult;

    #[test]
    fn prompt_context_with_now_playing() {
        let result = WindowsAppleMusicResult {
            summary: "Ready".to_string(),
            now_playing: Some("Track".to_string()),
            state: "playing".to_string(),
        };
        assert_eq!(
            result.to_prompt_context(),
            "Ready. playing. Now playing: Track."
        );
    }

    #[test]
    fn prompt_context_without_now_playing() {
        let result = WindowsAppleMusicResult {
            summary: "Stopped".to_string(),
            now_playing: None,
            state: "stopped".to_string(),
        };
        assert_eq!(result.to_prompt_context(), "Stopped. stopped.");
    }
}
