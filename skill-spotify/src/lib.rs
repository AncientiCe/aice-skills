//! Cross-platform Spotify media skill implementing the shared `MediaSkill`
//! trait from `skill-media`.
//!
//! The OAuth dance is owned by the runtime; this crate consumes a small
//! `SpotifyAuth` trait that returns a fresh access token. The HTTP layer is
//! abstracted behind a `SpotifyTransport` trait so the skill can be tested
//! without making real network calls.

mod transport;
mod types;

pub use transport::{HttpSpotifyTransport, SpotifyHttpResponse, SpotifyRequest, SpotifyTransport};
pub use types::{
    SpotifyAuth, SpotifyError, SpotifyPlayState, SpotifyTrack, StaticSpotifyAuth, SPOTIFY_API_BASE,
};

use async_trait::async_trait;
use metrics::{counter, histogram};
use skill_media::{MediaResult, MediaSkill, MediaSkillError};
use std::sync::Arc;
use std::time::Instant;

const SPOTIFY_SKILL_EXECUTE_TOTAL: &str = "spotify_skill_execute_total";
const SPOTIFY_SKILL_ERRORS_TOTAL: &str = "spotify_skill_errors_total";
const SPOTIFY_SKILL_EXECUTE_DURATION_SECONDS: &str = "spotify_skill_execute_duration_seconds";

#[derive(Clone)]
pub struct SpotifySkill {
    auth: Arc<dyn SpotifyAuth>,
    transport: Arc<dyn SpotifyTransport>,
}

impl SpotifySkill {
    pub fn new(auth: Arc<dyn SpotifyAuth>, transport: Arc<dyn SpotifyTransport>) -> Self {
        Self { auth, transport }
    }

    fn normalize_action(action: Option<&str>) -> String {
        action
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "status".to_string())
    }

    fn map_error(error: SpotifyError) -> MediaSkillError {
        match error {
            SpotifyError::Auth(msg) => MediaSkillError::Auth(msg),
            SpotifyError::NoActiveDevice => MediaSkillError::NoSource,
            SpotifyError::NotFound(msg) => MediaSkillError::Playback(msg),
            SpotifyError::Transport(msg) => MediaSkillError::Playback(msg),
            SpotifyError::Api(msg) => MediaSkillError::Playback(msg),
            SpotifyError::Unsupported(msg) => MediaSkillError::UnsupportedAction(msg),
        }
    }

    async fn execute_inner(
        &self,
        action: &str,
        target: Option<&str>,
    ) -> Result<MediaResult, SpotifyError> {
        match action {
            "play" => {
                let query = target
                    .map(str::trim)
                    .filter(|q| !q.is_empty())
                    .ok_or(SpotifyError::NoActiveDevice)?;
                let track = self.search_first_track(query).await?;
                self.play_uri(&track.uri).await?;
                let label = format!("{} - {}", track.name, track.artist);
                Ok(MediaResult {
                    summary: format!("Now Playing - {label}"),
                    now_playing: Some(label),
                    state: "playing".to_string(),
                })
            }
            "pause" => {
                self.put_no_body("/me/player/pause").await?;
                Ok(MediaResult {
                    summary: "Paused Spotify".to_string(),
                    now_playing: None,
                    state: "paused".to_string(),
                })
            }
            "resume" => {
                self.put_no_body("/me/player/play").await?;
                let np = self.now_playing_label().await.ok().flatten();
                Ok(MediaResult {
                    summary: "Resumed Spotify".to_string(),
                    now_playing: np,
                    state: "playing".to_string(),
                })
            }
            "stop" => {
                self.put_no_body("/me/player/pause").await?;
                Ok(MediaResult {
                    summary: "Stopped Spotify".to_string(),
                    now_playing: None,
                    state: "stopped".to_string(),
                })
            }
            "next" => {
                self.post_no_body("/me/player/next").await?;
                let np = self.now_playing_label().await.ok().flatten();
                Ok(MediaResult {
                    summary: "Skipped to next track".to_string(),
                    now_playing: np,
                    state: "playing".to_string(),
                })
            }
            "previous" => {
                self.post_no_body("/me/player/previous").await?;
                let np = self.now_playing_label().await.ok().flatten();
                Ok(MediaResult {
                    summary: "Went to previous track".to_string(),
                    now_playing: np,
                    state: "playing".to_string(),
                })
            }
            "shuffle_on" => {
                self.put_no_body("/me/player/shuffle?state=true").await?;
                Ok(MediaResult {
                    summary: "Shuffle enabled".to_string(),
                    now_playing: None,
                    state: "playing".to_string(),
                })
            }
            "shuffle_off" => {
                self.put_no_body("/me/player/shuffle?state=false").await?;
                Ok(MediaResult {
                    summary: "Shuffle disabled".to_string(),
                    now_playing: None,
                    state: "playing".to_string(),
                })
            }
            "status" => {
                let state = self.fetch_state().await?;
                Ok(MediaResult {
                    summary: "Spotify status".to_string(),
                    now_playing: state.now_playing,
                    state: state.state,
                })
            }
            other => Err(SpotifyError::Unsupported(other.to_string())),
        }
    }

    async fn token(&self) -> Result<String, SpotifyError> {
        self.auth.access_token().await.map_err(SpotifyError::Auth)
    }

    async fn put_no_body(&self, path: &str) -> Result<(), SpotifyError> {
        let token = self.token().await?;
        let req = SpotifyRequest::put(path, &token, None);
        let resp = self.transport.execute(req).await?;
        Self::interpret_status(resp)
    }

    async fn post_no_body(&self, path: &str) -> Result<(), SpotifyError> {
        let token = self.token().await?;
        let req = SpotifyRequest::post(path, &token, None);
        let resp = self.transport.execute(req).await?;
        Self::interpret_status(resp)
    }

    fn interpret_status(resp: SpotifyHttpResponse) -> Result<(), SpotifyError> {
        match resp.status {
            200 | 202 | 204 => Ok(()),
            401 => Err(SpotifyError::Auth("expired or invalid token".to_string())),
            404 => Err(SpotifyError::NoActiveDevice),
            other => Err(SpotifyError::Api(format!(
                "spotify api returned status {other}: {}",
                resp.body.unwrap_or_default()
            ))),
        }
    }

    async fn search_first_track(&self, query: &str) -> Result<SpotifyTrack, SpotifyError> {
        let token = self.token().await?;
        let encoded = url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>();
        let path = format!("/search?q={encoded}&type=track&limit=1");
        let req = SpotifyRequest::get(&path, &token);
        let resp = self.transport.execute(req).await?;
        if resp.status == 401 {
            return Err(SpotifyError::Auth("expired or invalid token".to_string()));
        }
        if !(200..300).contains(&resp.status) {
            return Err(SpotifyError::Api(format!(
                "search failed with status {}: {}",
                resp.status,
                resp.body.unwrap_or_default()
            )));
        }
        let body = resp.body.unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| SpotifyError::Api(format!("invalid search response: {e}")))?;
        let item = v
            .get("tracks")
            .and_then(|t| t.get("items"))
            .and_then(|items| items.get(0))
            .ok_or_else(|| SpotifyError::NotFound(format!("no spotify track for '{query}'")))?;
        let uri = item
            .get("uri")
            .and_then(|s| s.as_str())
            .ok_or_else(|| SpotifyError::Api("track item missing uri".to_string()))?
            .to_string();
        let name = item
            .get("name")
            .and_then(|s| s.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let artist = item
            .get("artists")
            .and_then(|a| a.get(0))
            .and_then(|a| a.get("name"))
            .and_then(|s| s.as_str())
            .unwrap_or("Unknown")
            .to_string();
        Ok(SpotifyTrack { uri, name, artist })
    }

    async fn play_uri(&self, uri: &str) -> Result<(), SpotifyError> {
        let token = self.token().await?;
        let body = serde_json::json!({ "uris": [uri] }).to_string();
        let req = SpotifyRequest::put("/me/player/play", &token, Some(body));
        let resp = self.transport.execute(req).await?;
        Self::interpret_status(resp)
    }

    async fn fetch_state(&self) -> Result<SpotifyPlayState, SpotifyError> {
        let token = self.token().await?;
        let req = SpotifyRequest::get("/me/player", &token);
        let resp = self.transport.execute(req).await?;
        if resp.status == 204 {
            return Ok(SpotifyPlayState {
                state: "stopped".to_string(),
                now_playing: None,
            });
        }
        if resp.status == 401 {
            return Err(SpotifyError::Auth("expired or invalid token".to_string()));
        }
        if !(200..300).contains(&resp.status) {
            return Err(SpotifyError::Api(format!(
                "player state returned status {}",
                resp.status
            )));
        }
        let body = resp.body.unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| SpotifyError::Api(format!("invalid player payload: {e}")))?;
        let is_playing = v
            .get("is_playing")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        let now_playing = v.get("item").and_then(Self::label_from_item);
        Ok(SpotifyPlayState {
            state: if is_playing { "playing" } else { "paused" }.to_string(),
            now_playing,
        })
    }

    async fn now_playing_label(&self) -> Result<Option<String>, SpotifyError> {
        Ok(self.fetch_state().await?.now_playing)
    }

    fn label_from_item(item: &serde_json::Value) -> Option<String> {
        let name = item.get("name")?.as_str()?;
        let artist = item
            .get("artists")
            .and_then(|a| a.get(0))
            .and_then(|a| a.get("name"))
            .and_then(|s| s.as_str())
            .unwrap_or("Unknown");
        Some(format!("{name} - {artist}"))
    }

    fn error_kind(error: &MediaSkillError) -> &'static str {
        match error {
            MediaSkillError::Auth(_) => "auth",
            MediaSkillError::Playback(_) => "playback",
            MediaSkillError::NoSource => "no_source",
            MediaSkillError::UnsupportedAction(_) => "unsupported",
        }
    }
}

#[async_trait]
impl MediaSkill for SpotifySkill {
    async fn execute(
        &self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<MediaResult, MediaSkillError> {
        let action_label = Self::normalize_action(action);
        let started_at = Instant::now();
        let result = self
            .execute_inner(action_label.as_str(), target)
            .await
            .map_err(Self::map_error);

        match &result {
            Ok(_) => {
                counter!(
                    SPOTIFY_SKILL_EXECUTE_TOTAL,
                    1,
                    "result" => "success",
                    "action" => action_label.clone()
                );
            }
            Err(error) => {
                counter!(
                    SPOTIFY_SKILL_EXECUTE_TOTAL,
                    1,
                    "result" => "error",
                    "action" => action_label.clone()
                );
                counter!(
                    SPOTIFY_SKILL_ERRORS_TOTAL,
                    1,
                    "action" => action_label.clone(),
                    "kind" => Self::error_kind(error)
                );
            }
        }
        histogram!(
            SPOTIFY_SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "action" => action_label
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct StubTransport {
        calls: Mutex<Vec<SpotifyRequest>>,
        responses: Mutex<Vec<SpotifyHttpResponse>>,
    }

    impl StubTransport {
        fn new(responses: Vec<SpotifyHttpResponse>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                responses: Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl SpotifyTransport for StubTransport {
        async fn execute(&self, req: SpotifyRequest) -> Result<SpotifyHttpResponse, SpotifyError> {
            let mut responses = self.responses.lock().expect("responses mutex poisoned");
            let resp = responses
                .pop()
                .expect("no stubbed response remaining for transport call");
            self.calls.lock().expect("calls mutex poisoned").push(req);
            Ok(resp)
        }
    }

    fn auth() -> Arc<dyn SpotifyAuth> {
        Arc::new(StaticSpotifyAuth::new("test-token"))
    }

    fn no_content() -> SpotifyHttpResponse {
        SpotifyHttpResponse {
            status: 204,
            body: None,
        }
    }

    #[tokio::test]
    async fn pause_calls_put_pause() {
        let transport = Arc::new(StubTransport::new(vec![no_content()]));
        let skill = SpotifySkill::new(auth(), transport.clone());
        let res = skill.execute(Some("pause"), None).await.expect("ok");
        assert_eq!(res.state, "paused");
        let calls = transport.calls.lock().expect("calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "PUT");
        assert!(calls[0].path.ends_with("/me/player/pause"));
    }

    #[tokio::test]
    async fn next_calls_post_next() {
        let transport = Arc::new(StubTransport::new(vec![
            SpotifyHttpResponse {
                status: 204,
                body: None,
            },
            no_content(),
        ]));
        let skill = SpotifySkill::new(auth(), transport.clone());
        let res = skill.execute(Some("next"), None).await.expect("ok");
        assert_eq!(res.state, "playing");
        let calls = transport.calls.lock().expect("calls");
        assert_eq!(calls[0].method, "POST");
        assert!(calls[0].path.ends_with("/me/player/next"));
    }

    #[tokio::test]
    async fn play_searches_then_plays_uri() {
        let search_body = r#"{"tracks":{"items":[{"uri":"spotify:track:abc","name":"Bohemian Rhapsody","artists":[{"name":"Queen"}]}]}}"#;
        let transport = Arc::new(StubTransport::new(vec![
            no_content(),
            SpotifyHttpResponse {
                status: 200,
                body: Some(search_body.to_string()),
            },
        ]));
        let skill = SpotifySkill::new(auth(), transport.clone());
        let res = skill
            .execute(Some("play"), Some("bohemian rhapsody"))
            .await
            .expect("ok");
        assert_eq!(res.state, "playing");
        assert_eq!(
            res.now_playing.as_deref(),
            Some("Bohemian Rhapsody - Queen")
        );
        let calls = transport.calls.lock().expect("calls");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].method, "GET");
        assert!(calls[0].path.contains("/search?q="));
        assert_eq!(calls[1].method, "PUT");
        assert!(calls[1].path.ends_with("/me/player/play"));
        assert!(calls[1]
            .body
            .as_deref()
            .unwrap_or("")
            .contains("spotify:track:abc"));
    }

    #[tokio::test]
    async fn play_returns_no_source_when_no_target() {
        let transport = Arc::new(StubTransport::new(vec![]));
        let skill = SpotifySkill::new(auth(), transport);
        let err = skill.execute(Some("play"), None).await.expect_err("err");
        assert!(matches!(err, MediaSkillError::NoSource));
    }

    #[tokio::test]
    async fn status_returns_paused_state() {
        let body = r#"{"is_playing":false,"item":{"name":"Song","artists":[{"name":"Artist"}]}}"#;
        let transport = Arc::new(StubTransport::new(vec![SpotifyHttpResponse {
            status: 200,
            body: Some(body.to_string()),
        }]));
        let skill = SpotifySkill::new(auth(), transport);
        let res = skill.execute(Some("status"), None).await.expect("ok");
        assert_eq!(res.state, "paused");
        assert_eq!(res.now_playing.as_deref(), Some("Song - Artist"));
    }

    #[tokio::test]
    async fn unauthorized_maps_to_auth_error() {
        let transport = Arc::new(StubTransport::new(vec![SpotifyHttpResponse {
            status: 401,
            body: Some("{}".to_string()),
        }]));
        let skill = SpotifySkill::new(auth(), transport);
        let err = skill.execute(Some("pause"), None).await.expect_err("err");
        assert!(matches!(err, MediaSkillError::Auth(_)));
    }

    #[tokio::test]
    async fn pause_404_maps_to_no_source() {
        let transport = Arc::new(StubTransport::new(vec![SpotifyHttpResponse {
            status: 404,
            body: Some("{}".to_string()),
        }]));
        let skill = SpotifySkill::new(auth(), transport);
        let err = skill.execute(Some("pause"), None).await.expect_err("err");
        assert!(matches!(err, MediaSkillError::NoSource));
    }

    #[tokio::test]
    async fn unsupported_action_maps_to_unsupported() {
        let transport = Arc::new(StubTransport::new(vec![]));
        let skill = SpotifySkill::new(auth(), transport);
        let err = skill
            .execute(Some("teleport"), None)
            .await
            .expect_err("err");
        assert!(matches!(err, MediaSkillError::UnsupportedAction(_)));
    }
}
