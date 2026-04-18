//! HTTP transport abstraction so the skill can be tested without network access
//! and so callers can swap implementations (e.g. for instrumentation).

use crate::types::{SpotifyError, SPOTIFY_API_BASE};
use async_trait::async_trait;

#[derive(Clone, Debug)]
pub struct SpotifyRequest {
    pub method: String,
    pub path: String,
    pub access_token: String,
    pub body: Option<String>,
}

impl SpotifyRequest {
    pub fn get(path: &str, token: &str) -> Self {
        Self::new("GET", path, token, None)
    }

    pub fn put(path: &str, token: &str, body: Option<String>) -> Self {
        Self::new("PUT", path, token, body)
    }

    pub fn post(path: &str, token: &str, body: Option<String>) -> Self {
        Self::new("POST", path, token, body)
    }

    fn new(method: &str, path: &str, token: &str, body: Option<String>) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            access_token: token.to_string(),
            body,
        }
    }

    pub fn url(&self) -> String {
        if self.path.starts_with("http") {
            self.path.clone()
        } else {
            format!("{}{}", SPOTIFY_API_BASE, self.path)
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpotifyHttpResponse {
    pub status: u16,
    pub body: Option<String>,
}

#[async_trait]
pub trait SpotifyTransport: Send + Sync {
    async fn execute(&self, req: SpotifyRequest) -> Result<SpotifyHttpResponse, SpotifyError>;
}

/// Live `reqwest`-backed transport.
#[derive(Clone)]
pub struct HttpSpotifyTransport {
    client: reqwest::Client,
}

impl HttpSpotifyTransport {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for HttpSpotifyTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SpotifyTransport for HttpSpotifyTransport {
    async fn execute(&self, req: SpotifyRequest) -> Result<SpotifyHttpResponse, SpotifyError> {
        let url = req.url();
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|e| SpotifyError::Transport(e.to_string()))?;
        let mut builder = self
            .client
            .request(method, &url)
            .bearer_auth(&req.access_token);
        if let Some(body) = req.body.as_ref() {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone());
        }
        let resp = builder
            .send()
            .await
            .map_err(|e| SpotifyError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| SpotifyError::Transport(e.to_string()))?;
        Ok(SpotifyHttpResponse {
            status,
            body: if body.is_empty() { None } else { Some(body) },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_appends_base_for_relative_path() {
        let req = SpotifyRequest::get("/me/player", "tok");
        assert_eq!(req.url(), format!("{SPOTIFY_API_BASE}/me/player"));
    }

    #[test]
    fn url_passes_through_absolute_url() {
        let req = SpotifyRequest::get("https://example.com/x", "tok");
        assert_eq!(req.url(), "https://example.com/x");
    }
}
