//! Public types and traits for the Spotify skill.

use async_trait::async_trait;

pub const SPOTIFY_API_BASE: &str = "https://api.spotify.com/v1";

#[derive(Clone, Debug, PartialEq)]
pub struct SpotifyTrack {
    pub uri: String,
    pub name: String,
    pub artist: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpotifyPlayState {
    pub state: String,
    pub now_playing: Option<String>,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum SpotifyError {
    #[error("auth error: {0}")]
    Auth(String),
    #[error("no active spotify device or empty target")]
    NoActiveDevice,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("spotify api error: {0}")]
    Api(String),
    #[error("unsupported action: {0}")]
    Unsupported(String),
}

/// Returns a fresh OAuth access token for the Spotify Web API. Implementations
/// own the OAuth flow and refresh-token rotation.
#[async_trait]
pub trait SpotifyAuth: Send + Sync {
    async fn access_token(&self) -> Result<String, String>;
}

/// Static auth provider intended for tests and short-lived experiments where
/// the token is supplied externally.
pub struct StaticSpotifyAuth {
    token: String,
}

impl StaticSpotifyAuth {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

#[async_trait]
impl SpotifyAuth for StaticSpotifyAuth {
    async fn access_token(&self) -> Result<String, String> {
        Ok(self.token.clone())
    }
}
