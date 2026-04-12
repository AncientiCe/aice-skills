use crate::api::AppleMusicClient;
use crate::session::{SessionSnapshot, SessionStore};
use crate::types::{WindowsAppleMusic, WindowsAppleMusicResult, WindowsAppleMusicSkillError};
use crate::webview_player::WebPlaybackController;
use async_trait::async_trait;
use metrics::{counter, histogram};
use std::sync::Mutex;
use std::time::Instant;

const WINDOWS_APPLE_MUSIC_EXECUTE_TOTAL: &str = "windows_apple_music_execute_total";
const WINDOWS_APPLE_MUSIC_ERRORS_TOTAL: &str = "windows_apple_music_errors_total";
const WINDOWS_APPLE_MUSIC_EXECUTE_DURATION_SECONDS: &str =
    "windows_apple_music_execute_duration_seconds";

#[derive(Default)]
struct PlaybackState {
    controller: Option<WebPlaybackController>,
    now_playing: Option<String>,
    state: String,
}

pub struct WindowsAppleMusicSkill {
    dry_run: bool,
    playback: Mutex<PlaybackState>,
}

impl WindowsAppleMusicSkill {
    pub fn new() -> Self {
        Self {
            dry_run: false,
            playback: Mutex::new(PlaybackState {
                controller: None,
                now_playing: None,
                state: "unknown".to_string(),
            }),
        }
    }

    pub fn new_for_tests() -> Self {
        Self {
            dry_run: true,
            playback: Mutex::new(PlaybackState {
                controller: None,
                now_playing: None,
                state: "unknown".to_string(),
            }),
        }
    }

    fn normalize_action(action: Option<&str>) -> String {
        action
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "status".to_string())
    }

    fn load_session(&self) -> Result<SessionSnapshot, WindowsAppleMusicSkillError> {
        let store =
            SessionStore::new().map_err(|e| WindowsAppleMusicSkillError::Auth(e.to_string()))?;
        let session = store.load().unwrap_or_default();
        if session.is_configured() {
            Ok(session)
        } else {
            Err(WindowsAppleMusicSkillError::Auth(
                "session not configured; run Apple Music login flow".to_string(),
            ))
        }
    }

    fn with_playback_mut<F, T>(&self, apply: F) -> Result<T, WindowsAppleMusicSkillError>
    where
        F: FnOnce(&mut PlaybackState) -> Result<T, WindowsAppleMusicSkillError>,
    {
        let mut state = self.playback.lock().map_err(|_| {
            WindowsAppleMusicSkillError::Playback("playback mutex poisoned".to_string())
        })?;
        apply(&mut state)
    }

    fn ensure_controller(
        &self,
        session: &SessionSnapshot,
        storefront: &str,
    ) -> Result<(), WindowsAppleMusicSkillError> {
        self.with_playback_mut(|playback| {
            let needs_rebuild = playback
                .controller
                .as_ref()
                .map(|controller| controller.storefront() != storefront)
                .unwrap_or(true)
                || playback.controller.is_none();

            if !needs_rebuild {
                return Ok(());
            }

            if let Some(mut existing) = playback.controller.take() {
                existing.shutdown();
            }

            let controller = WebPlaybackController::new(session)
                .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
            playback.controller = Some(controller);
            Ok(())
        })
    }

    fn invoke_controller<F>(
        &self,
        session: &SessionSnapshot,
        storefront: &str,
        action: &str,
        op: F,
    ) -> Result<(), WindowsAppleMusicSkillError>
    where
        F: FnOnce(&WebPlaybackController) -> anyhow::Result<()>,
    {
        self.ensure_controller(session, storefront)?;
        self.with_playback_mut(|playback| {
            let controller = playback.controller.as_ref().ok_or_else(|| {
                WindowsAppleMusicSkillError::Playback("embedded player unavailable".to_string())
            })?;
            op(controller)
                .map_err(|e| WindowsAppleMusicSkillError::Playback(format!("{action} failed: {e}")))
        })
    }

    fn set_state(
        &self,
        state: &str,
        now_playing: Option<String>,
    ) -> Result<(), WindowsAppleMusicSkillError> {
        self.with_playback_mut(|playback| {
            playback.state = state.to_string();
            playback.now_playing = now_playing;
            Ok(())
        })
    }

    fn current_state(&self) -> Result<(String, Option<String>), WindowsAppleMusicSkillError> {
        self.with_playback_mut(|playback| {
            Ok((playback.state.clone(), playback.now_playing.clone()))
        })
    }

    async fn execute_inner(
        &self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<WindowsAppleMusicResult, WindowsAppleMusicSkillError> {
        if !cfg!(target_os = "windows") {
            return Err(WindowsAppleMusicSkillError::UnsupportedAction(
                "windows apple music skill requires Windows".to_string(),
            ));
        }

        let action = Self::normalize_action(action);
        if self.dry_run {
            return self.execute_dry_run(action.as_str(), target);
        }

        match action.as_str() {
            "play" => {
                let query = target
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or(WindowsAppleMusicSkillError::NoSource)?;
                let session = self.load_session()?;
                let client = AppleMusicClient::new(session.clone());
                let results = client
                    .search(query)
                    .await
                    .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
                let track = results
                    .first()
                    .ok_or(WindowsAppleMusicSkillError::NoSource)?;
                let storefront = client
                    .effective_storefront()
                    .await
                    .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
                self.invoke_controller(&session, storefront.as_str(), "play", |controller| {
                    controller.play(track.resolve_id.as_str(), Some(track.id.as_str()))
                })?;
                let label = format!("{} - {}", track.artist_name, track.track_name);
                self.set_state("playing", Some(label.clone()))?;
                Ok(WindowsAppleMusicResult {
                    summary: format!("Now playing {label}"),
                    now_playing: Some(label),
                    state: "playing".to_string(),
                })
            }
            "pause" => {
                let session = self.load_session()?;
                let storefront = AppleMusicClient::new(session.clone())
                    .effective_storefront()
                    .await
                    .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
                self.invoke_controller(&session, storefront.as_str(), "pause", |controller| {
                    controller.pause()
                })?;
                let (_, now_playing) = self.current_state()?;
                self.set_state("paused", now_playing.clone())?;
                Ok(WindowsAppleMusicResult {
                    summary: "Playback paused".to_string(),
                    now_playing,
                    state: "paused".to_string(),
                })
            }
            "resume" => {
                let session = self.load_session()?;
                let storefront = AppleMusicClient::new(session.clone())
                    .effective_storefront()
                    .await
                    .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
                self.invoke_controller(&session, storefront.as_str(), "resume", |controller| {
                    controller.resume()
                })?;
                let (_, now_playing) = self.current_state()?;
                self.set_state("playing", now_playing.clone())?;
                Ok(WindowsAppleMusicResult {
                    summary: "Playback resumed".to_string(),
                    now_playing,
                    state: "playing".to_string(),
                })
            }
            "stop" => {
                let session = self.load_session()?;
                let storefront = AppleMusicClient::new(session.clone())
                    .effective_storefront()
                    .await
                    .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
                self.invoke_controller(&session, storefront.as_str(), "stop", |controller| {
                    controller.stop()
                })?;
                self.set_state("stopped", None)?;
                Ok(WindowsAppleMusicResult {
                    summary: "Playback stopped".to_string(),
                    now_playing: None,
                    state: "stopped".to_string(),
                })
            }
            "next" => {
                let session = self.load_session()?;
                let storefront = AppleMusicClient::new(session.clone())
                    .effective_storefront()
                    .await
                    .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
                self.invoke_controller(&session, storefront.as_str(), "next", |controller| {
                    controller.next()
                })?;
                let (_, now_playing) = self.current_state()?;
                self.set_state("playing", now_playing.clone())?;
                Ok(WindowsAppleMusicResult {
                    summary: "Skipped to next track".to_string(),
                    now_playing,
                    state: "playing".to_string(),
                })
            }
            "previous" => {
                let session = self.load_session()?;
                let storefront = AppleMusicClient::new(session.clone())
                    .effective_storefront()
                    .await
                    .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
                self.invoke_controller(&session, storefront.as_str(), "previous", |controller| {
                    controller.previous()
                })?;
                let (_, now_playing) = self.current_state()?;
                self.set_state("playing", now_playing.clone())?;
                Ok(WindowsAppleMusicResult {
                    summary: "Returned to previous track".to_string(),
                    now_playing,
                    state: "playing".to_string(),
                })
            }
            "shuffle_on" => {
                let session = self.load_session()?;
                let storefront = AppleMusicClient::new(session.clone())
                    .effective_storefront()
                    .await
                    .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
                self.invoke_controller(
                    &session,
                    storefront.as_str(),
                    "shuffle_on",
                    |controller| controller.shuffle(true),
                )?;
                let (_, now_playing) = self.current_state()?;
                self.set_state("playing", now_playing.clone())?;
                Ok(WindowsAppleMusicResult {
                    summary: "Shuffle enabled".to_string(),
                    now_playing,
                    state: "playing".to_string(),
                })
            }
            "shuffle_off" => {
                let session = self.load_session()?;
                let storefront = AppleMusicClient::new(session.clone())
                    .effective_storefront()
                    .await
                    .map_err(|e| WindowsAppleMusicSkillError::Playback(e.to_string()))?;
                self.invoke_controller(
                    &session,
                    storefront.as_str(),
                    "shuffle_off",
                    |controller| controller.shuffle(false),
                )?;
                let (_, now_playing) = self.current_state()?;
                self.set_state("playing", now_playing.clone())?;
                Ok(WindowsAppleMusicResult {
                    summary: "Shuffle disabled".to_string(),
                    now_playing,
                    state: "playing".to_string(),
                })
            }
            "status" => {
                let (state, now_playing) = self.current_state()?;
                Ok(WindowsAppleMusicResult {
                    summary: "Windows Apple Music skill ready".to_string(),
                    now_playing,
                    state,
                })
            }
            _ => Err(WindowsAppleMusicSkillError::UnsupportedAction(action)),
        }
    }

    fn execute_dry_run(
        &self,
        action: &str,
        target: Option<&str>,
    ) -> Result<WindowsAppleMusicResult, WindowsAppleMusicSkillError> {
        match action {
            "play" => {
                let query = target
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or(WindowsAppleMusicSkillError::NoSource)?;
                let label = format!("Test Track - {query}");
                self.set_state("playing", Some(label.clone()))?;
                Ok(WindowsAppleMusicResult {
                    summary: format!("Now playing {label}"),
                    now_playing: Some(label),
                    state: "playing".to_string(),
                })
            }
            "pause" => {
                let (_, now_playing) = self.current_state()?;
                self.set_state("paused", now_playing.clone())?;
                Ok(WindowsAppleMusicResult {
                    summary: "Playback paused".to_string(),
                    now_playing,
                    state: "paused".to_string(),
                })
            }
            "resume" | "next" | "previous" | "shuffle_on" | "shuffle_off" => {
                let (_, now_playing) = self.current_state()?;
                self.set_state("playing", now_playing.clone())?;
                Ok(WindowsAppleMusicResult {
                    summary: format!("Action completed: {action}"),
                    now_playing,
                    state: "playing".to_string(),
                })
            }
            "stop" => {
                self.set_state("stopped", None)?;
                Ok(WindowsAppleMusicResult {
                    summary: "Playback stopped".to_string(),
                    now_playing: None,
                    state: "stopped".to_string(),
                })
            }
            "status" => {
                let (state, now_playing) = self.current_state()?;
                Ok(WindowsAppleMusicResult {
                    summary: "Windows Apple Music skill ready".to_string(),
                    now_playing,
                    state,
                })
            }
            _ => Err(WindowsAppleMusicSkillError::UnsupportedAction(
                action.to_string(),
            )),
        }
    }
}

impl Default for WindowsAppleMusicSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WindowsAppleMusic for WindowsAppleMusicSkill {
    async fn execute(
        &self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<WindowsAppleMusicResult, WindowsAppleMusicSkillError> {
        let action_label = Self::normalize_action(action);
        let t0 = Instant::now();
        let result = self
            .execute_inner(Some(action_label.as_str()), target)
            .await;

        match &result {
            Ok(_) => counter!(
                WINDOWS_APPLE_MUSIC_EXECUTE_TOTAL,
                1,
                "result" => "success",
                "action" => action_label.clone()
            ),
            Err(e) => {
                counter!(
                    WINDOWS_APPLE_MUSIC_EXECUTE_TOTAL,
                    1,
                    "result" => "error",
                    "action" => action_label.clone()
                );
                counter!(
                    WINDOWS_APPLE_MUSIC_ERRORS_TOTAL,
                    1,
                    "action" => action_label.clone(),
                    "kind" => e.to_string()
                );
            }
        }
        histogram!(
            WINDOWS_APPLE_MUSIC_EXECUTE_DURATION_SECONDS,
            t0.elapsed().as_secs_f64(),
            "action" => action_label
        );

        result
    }
}

pub struct MockWindowsAppleMusicSkill {
    pub result: Result<WindowsAppleMusicResult, WindowsAppleMusicSkillError>,
}

impl MockWindowsAppleMusicSkill {
    pub fn ok(result: WindowsAppleMusicResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(e: WindowsAppleMusicSkillError) -> Self {
        Self { result: Err(e) }
    }
}

#[async_trait]
impl WindowsAppleMusic for MockWindowsAppleMusicSkill {
    async fn execute(
        &self,
        _action: Option<&str>,
        _target: Option<&str>,
    ) -> Result<WindowsAppleMusicResult, WindowsAppleMusicSkillError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::WindowsAppleMusicSkill;

    #[test]
    fn normalize_action_defaults_to_status() {
        assert_eq!(WindowsAppleMusicSkill::normalize_action(None), "status");
    }

    #[test]
    fn normalize_action_lowercases_and_trims() {
        assert_eq!(
            WindowsAppleMusicSkill::normalize_action(Some("  ShUFfLe_On  ")),
            "shuffle_on"
        );
    }
}
