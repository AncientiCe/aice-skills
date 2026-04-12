use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;

use crate::session::SessionSnapshot;

const DEFAULT_STOREFRONT: &str = "us";

#[derive(Debug, Clone)]
pub struct Track {
    pub id: String,
    pub resolve_id: String,
    pub track_name: String,
    pub artist_name: String,
    pub preview_url: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct StreamCandidate {
    pub url: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct StreamResolveAttempt {
    pub endpoint: String,
    pub status: Option<u16>,
    pub note: String,
    pub stream_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StreamResolveReport {
    pub candidate: Option<StreamCandidate>,
    pub attempts: Vec<StreamResolveAttempt>,
}

pub struct AppleMusicClient {
    http: reqwest::Client,
    session: SessionSnapshot,
}

impl AppleMusicClient {
    pub fn new(session: SessionSnapshot) -> Self {
        Self {
            http: reqwest::Client::new(),
            session,
        }
    }

    pub fn set_session(&mut self, session: SessionSnapshot) {
        self.session = session;
    }

    pub async fn effective_storefront(&self) -> Result<String> {
        self.current_storefront().await
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Track>> {
        if self.session.is_configured() {
            match self.search_amp_api(query).await {
                Ok(tracks) if !tracks.is_empty() => return Ok(tracks),
                Ok(_) => {}
                Err(err) => {
                    eprintln!("Authenticated search failed: {err}");
                }
            }
        }
        self.search_itunes_fallback(query).await
    }

    pub async fn search_library(&self, query: &str) -> Result<Vec<Track>> {
        let encoded = urlencoding::encode(query);
        let url = format!(
            "https://amp-api.music.apple.com/v1/me/library/search?term={encoded}&types=library-songs&limit=25"
        );
        let payload = self
            .authenticated_get(&url)?
            .send()
            .await
            .context("library search request failed")?
            .error_for_status()
            .context("library search rejected request")?
            .json::<LibrarySearchResponse>()
            .await
            .context("failed to parse library search response")?;

        let mut tracks = Vec::new();
        for item in payload
            .results
            .library_songs
            .and_then(|s| s.data)
            .unwrap_or_default()
        {
            let attributes = item.attributes.unwrap_or_default();
            let catalog_from_play_params = attributes
                .play_params
                .as_ref()
                .and_then(|p| p.catalog_id.clone());
            let catalog_from_relationship = item
                .relationships
                .as_ref()
                .and_then(|r| r.catalog.as_ref())
                .and_then(|c| c.data.as_ref())
                .and_then(|data| data.first())
                .and_then(|song| song.id.clone());
            let resolve_id = catalog_from_play_params
                .or(catalog_from_relationship)
                .unwrap_or_else(|| item.id.clone().unwrap_or_default());

            tracks.push(Track {
                id: item.id.unwrap_or_default(),
                resolve_id,
                track_name: attributes.name.unwrap_or_else(|| "Unknown".to_string()),
                artist_name: attributes
                    .artist_name
                    .unwrap_or_else(|| "Unknown".to_string()),
                preview_url: attributes
                    .previews
                    .and_then(|p| p.first().cloned())
                    .and_then(|p| p.url),
                source: "library-search".to_string(),
            });
        }

        Ok(tracks)
    }

    pub async fn resolve_stream_report_with_hints(
        &self,
        resolve_id: &str,
        library_id: Option<&str>,
    ) -> Result<StreamResolveReport> {
        if !self.session.is_configured() {
            return Ok(StreamResolveReport {
                candidate: None,
                attempts: vec![StreamResolveAttempt {
                    endpoint: "session-check".to_string(),
                    status: None,
                    note: "session is not configured (missing tokens)".to_string(),
                    stream_url: None,
                }],
            });
        }

        let storefront = self.current_storefront().await?;
        let mut ids = vec![resolve_id.to_string()];
        if let Some(lib) = library_id {
            if !lib.is_empty() && !ids.iter().any(|v| v == lib) {
                ids.push(lib.to_string());
            }
        }

        let mut attempts = Vec::new();
        for track_id in ids {
            let mut candidates = vec![
                format!("https://amp-api.music.apple.com/v1/catalog/{storefront}/songs/{track_id}"),
                format!(
                    "https://amp-api.music.apple.com/v1/catalog/{storefront}/songs/{track_id}?include=albums,artists"
                ),
                format!(
                    "https://amp-api.music.apple.com/v1/catalog/{storefront}/songs/{track_id}/streams"
                ),
                format!("https://amp-api.music.apple.com/v1/me/library/songs/{track_id}"),
                format!(
                    "https://amp-api.music.apple.com/v1/me/library/songs/{track_id}?include=catalog"
                ),
            ];
            candidates.dedup();

            for endpoint in candidates {
                let attempt = self.try_extract_stream_url(&endpoint).await?;
                if let Some(url) = &attempt.stream_url {
                    return Ok(StreamResolveReport {
                        candidate: Some(StreamCandidate {
                            url: url.clone(),
                            source: endpoint,
                        }),
                        attempts,
                    });
                }
                attempts.push(attempt);
            }

            for attempt in self
                .try_web_playback_resolver(&track_id, library_id, &storefront)
                .await?
            {
                if let Some(url) = &attempt.stream_url {
                    return Ok(StreamResolveReport {
                        candidate: Some(StreamCandidate {
                            url: url.clone(),
                            source: attempt.endpoint.clone(),
                        }),
                        attempts,
                    });
                }
                attempts.push(attempt);
            }
        }

        Ok(StreamResolveReport {
            candidate: None,
            attempts,
        })
    }

    async fn current_storefront(&self) -> Result<String> {
        if self.session.is_configured() {
            let url = "https://amp-api.music.apple.com/v1/me/account?meta=subscription";
            if let Ok(response) = self.authenticated_get(url)?.send().await {
                if let Ok(ok) = response.error_for_status() {
                    if let Ok(value) = ok.json::<Value>().await {
                        if let Some(sf) = value
                            .get("meta")
                            .and_then(|m| m.get("subscription"))
                            .and_then(|s| s.get("storefront"))
                            .and_then(Value::as_str)
                        {
                            return Ok(sf.to_string());
                        }
                    }
                }
            }
        }
        Ok(self
            .session
            .storefront
            .clone()
            .unwrap_or_else(|| DEFAULT_STOREFRONT.to_string()))
    }

    async fn search_amp_api(&self, query: &str) -> Result<Vec<Track>> {
        let storefront = self
            .session
            .storefront
            .as_deref()
            .unwrap_or(DEFAULT_STOREFRONT);

        let encoded = urlencoding::encode(query);
        let url = format!(
            "https://amp-api.music.apple.com/v1/catalog/{storefront}/search?term={encoded}&types=songs&limit=10"
        );
        let req = self.authenticated_get(&url)?;

        let payload = req
            .send()
            .await
            .context("amp-api request failed")?
            .error_for_status()
            .context("amp-api rejected request")?
            .json::<AmpSearchResponse>()
            .await
            .context("failed to parse amp-api response")?;

        let mut tracks = Vec::new();
        for song in payload
            .results
            .songs
            .and_then(|s| s.data)
            .unwrap_or_default()
        {
            let attributes = song.attributes.unwrap_or_default();
            let song_id = song.id.unwrap_or_default();
            tracks.push(Track {
                id: song_id.clone(),
                resolve_id: song_id,
                track_name: attributes.name.unwrap_or_else(|| "Unknown".to_string()),
                artist_name: attributes
                    .artist_name
                    .unwrap_or_else(|| "Unknown".to_string()),
                preview_url: attributes
                    .previews
                    .and_then(|p| p.first().cloned())
                    .and_then(|p| p.url),
                source: "amp-api".to_string(),
            });
        }

        Ok(tracks)
    }

    fn authenticated_get(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let mut headers = HeaderMap::new();
        if let Some(token) = &self.session.developer_token {
            let auth = format!("Bearer {token}");
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&auth).context("invalid authorization header")?,
            );
        }
        if let Some(cookie) = &self.session.raw_cookie_header {
            headers.insert(
                COOKIE,
                HeaderValue::from_str(cookie).context("invalid cookie header")?,
            );
        }

        let mut req = self.http.get(url).headers(headers);
        if let Some(media_user_token) = &self.session.media_user_token {
            req = req.header("media-user-token", media_user_token);
            req = req.header("x-apple-music-user-token", media_user_token);
        }
        req = req
            .header("origin", "https://music.apple.com")
            .header("referer", "https://music.apple.com/")
            .header("accept", "application/json");
        Ok(req)
    }

    async fn try_extract_stream_url(&self, endpoint: &str) -> Result<StreamResolveAttempt> {
        let request = self.authenticated_get(endpoint)?;
        let response = request
            .send()
            .await
            .context("stream probe request failed")?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Ok(StreamResolveAttempt {
                endpoint: endpoint.to_string(),
                status: Some(status),
                note: format!("endpoint returned HTTP {status}"),
                stream_url: None,
            });
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if content_type.contains("application/json") {
            let value = response.json::<Value>().await?;
            let found = find_m3u8_in_json(&value);
            return Ok(StreamResolveAttempt {
                endpoint: endpoint.to_string(),
                status: Some(status),
                note: if found.is_some() {
                    "found m3u8 URL in JSON payload".to_string()
                } else {
                    "JSON payload had no m3u8 URL".to_string()
                },
                stream_url: found,
            });
        }

        let body = response.text().await?;
        if body.contains(".m3u8") {
            if let Some(url) = extract_first_url_with(".m3u8", &body) {
                return Ok(StreamResolveAttempt {
                    endpoint: endpoint.to_string(),
                    status: Some(status),
                    note: "found m3u8 URL in text payload".to_string(),
                    stream_url: Some(url),
                });
            }
        }
        Ok(StreamResolveAttempt {
            endpoint: endpoint.to_string(),
            status: Some(status),
            note: "no usable m3u8 URL found".to_string(),
            stream_url: None,
        })
    }

    async fn try_web_playback_resolver(
        &self,
        track_id: &str,
        library_id: Option<&str>,
        storefront: &str,
    ) -> Result<Vec<StreamResolveAttempt>> {
        let mut attempts = Vec::new();

        let endpoints = [
            "https://play.itunes.apple.com/WebObjects/MZPlay.woa/wa/webPlayback",
            "https://play.itunes.apple.com/WebObjects/MZPlayLocal.woa/wa/webPlayback",
            "https://play.music.apple.com/WebObjects/MZPlay.woa/wa/webPlayback",
        ];

        let payloads = vec![
            json!({
                "salableAdamId": track_id,
                "language": "en-US",
                "storefrontId": storefront
            }),
            json!({
                "adamId": track_id,
                "language": "en-US",
                "storefrontId": storefront
            }),
            json!({
                "id": track_id,
                "type": "songs",
                "storefront": storefront,
                "languageTag": "en-US"
            }),
        ];

        for endpoint in endpoints {
            attempts.push(self.try_extract_mzplay_get(endpoint).await?);
            attempts.push(
                self.try_extract_mzplay_get_with_query(endpoint, &[("adamId", track_id)])
                    .await?,
            );
            attempts.push(
                self.try_extract_mzplay_get_with_query(
                    endpoint,
                    &[("salableAdamId", track_id), ("storefrontId", storefront)],
                )
                .await?,
            );
            if let Some(lib) = library_id {
                attempts.push(
                    self.try_extract_mzplay_get_with_query(
                        endpoint,
                        &[("id", lib), ("isLibrary", "true"), ("kind", "song")],
                    )
                    .await?,
                );
            }
            for payload in &payloads {
                attempts.push(self.try_extract_stream_url_post(endpoint, payload).await?);
            }
            if let Some(lib) = library_id {
                attempts.push(
                    self.try_extract_stream_url_post(
                        endpoint,
                        &json!({
                            "id": lib,
                            "kind": "song",
                            "isLibrary": true,
                            "catalogId": track_id,
                            "reportingId": track_id,
                            "language": "en-US",
                            "storefrontId": storefront
                        }),
                    )
                    .await?,
                );
            }
        }

        Ok(attempts)
    }

    async fn try_extract_mzplay_get(&self, endpoint: &str) -> Result<StreamResolveAttempt> {
        let req = self.authenticated_mzplay_get(endpoint)?;
        self.consume_stream_probe_response(req.send().await?, endpoint)
            .await
    }

    async fn try_extract_mzplay_get_with_query(
        &self,
        endpoint: &str,
        query: &[(&str, &str)],
    ) -> Result<StreamResolveAttempt> {
        let req = self.authenticated_mzplay_get(endpoint)?.query(query);
        let label = format!("{endpoint} [GET query={query:?}]");
        self.consume_stream_probe_response(req.send().await?, &label)
            .await
    }

    async fn try_extract_stream_url_post(
        &self,
        endpoint: &str,
        payload: &Value,
    ) -> Result<StreamResolveAttempt> {
        let mut req = self.authenticated_post(endpoint, payload)?;
        req = req
            .header("accept", "application/json, text/plain, */*")
            .header("content-type", "application/json")
            .header("accept-language", "en-US,en;q=0.9")
            .header(
                "x-apple-store-front",
                self.session
                    .storefront
                    .as_deref()
                    .unwrap_or(DEFAULT_STOREFRONT),
            );

        let response = req
            .send()
            .await
            .context("webPlayback probe request failed")?;
        self.consume_stream_probe_response(
            response,
            &format!("{endpoint} [POST payload={}]", payload),
        )
        .await
    }

    fn authenticated_post(&self, url: &str, payload: &Value) -> Result<reqwest::RequestBuilder> {
        let mut headers = HeaderMap::new();
        if let Some(token) = &self.session.developer_token {
            let auth = format!("Bearer {token}");
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&auth).context("invalid authorization header")?,
            );
        }
        if let Some(cookie) = &self.session.raw_cookie_header {
            headers.insert(
                COOKIE,
                HeaderValue::from_str(cookie).context("invalid cookie header")?,
            );
        }
        let mut req = self.http.post(url).headers(headers).json(payload);
        if let Some(media_user_token) = &self.session.media_user_token {
            req = req.header("media-user-token", media_user_token);
            req = req.header("x-apple-music-user-token", media_user_token);
        }
        req = req
            .header("origin", "https://music.apple.com")
            .header("referer", "https://music.apple.com/");
        Ok(req)
    }

    fn authenticated_mzplay_get(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let mut headers = HeaderMap::new();
        if let Some(token) = &self.session.developer_token {
            let auth = format!("Bearer {token}");
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&auth).context("invalid authorization header")?,
            );
        }
        if let Some(cookie) = &self.session.raw_cookie_header {
            headers.insert(
                COOKIE,
                HeaderValue::from_str(cookie).context("invalid cookie header")?,
            );
        }

        let mut req = self.http.get(url).headers(headers);
        if let Some(media_user_token) = &self.session.media_user_token {
            req = req
                .header("media-user-token", media_user_token)
                .header("x-apple-music-user-token", media_user_token);
        }
        req = req
            .header("accept", "application/json")
            .header("x-apple-renewal", "true")
            .header("origin", "https://music.apple.com")
            .header("referer", "https://music.apple.com/");
        Ok(req)
    }

    async fn consume_stream_probe_response(
        &self,
        response: reqwest::Response,
        endpoint_label: &str,
    ) -> Result<StreamResolveAttempt> {
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Ok(StreamResolveAttempt {
                endpoint: endpoint_label.to_string(),
                status: Some(status),
                note: format!("endpoint returned HTTP {status}"),
                stream_url: None,
            });
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if content_type.contains("application/json") {
            let value = response.json::<Value>().await?;
            let found = find_webplayback_asset_url(&value).or_else(|| find_m3u8_in_json(&value));
            return Ok(StreamResolveAttempt {
                endpoint: endpoint_label.to_string(),
                status: Some(status),
                note: if found.is_some() {
                    "found playable asset URL in JSON payload".to_string()
                } else {
                    "JSON payload had no playable URL".to_string()
                },
                stream_url: found,
            });
        }

        let body = response.text().await?;
        let found = extract_first_url_with(".m3u8", &body);
        Ok(StreamResolveAttempt {
            endpoint: endpoint_label.to_string(),
            status: Some(status),
            note: if found.is_some() {
                "found m3u8 URL in text payload".to_string()
            } else {
                "no usable m3u8 URL found".to_string()
            },
            stream_url: found,
        })
    }

    async fn search_itunes_fallback(&self, query: &str) -> Result<Vec<Track>> {
        let encoded = urlencoding::encode(query);
        let url = format!("https://itunes.apple.com/search?entity=song&limit=10&term={encoded}");
        let payload = self
            .http
            .get(url)
            .send()
            .await
            .context("itunes search request failed")?
            .error_for_status()
            .context("itunes search rejected request")?
            .json::<ItunesSearchResponse>()
            .await
            .context("failed to parse itunes response")?;

        let tracks = payload
            .results
            .into_iter()
            .map(|item| Track {
                id: item.track_id.map(|v| v.to_string()).unwrap_or_default(),
                resolve_id: item.track_id.map(|v| v.to_string()).unwrap_or_default(),
                track_name: item.track_name.unwrap_or_else(|| "Unknown".to_string()),
                artist_name: item.artist_name.unwrap_or_else(|| "Unknown".to_string()),
                preview_url: item.preview_url,
                source: "itunes-fallback".to_string(),
            })
            .collect();
        Ok(tracks)
    }
}

#[derive(Debug, Deserialize)]
struct AmpSearchResponse {
    #[serde(default)]
    results: AmpResults,
}

#[derive(Debug, Deserialize, Default)]
struct AmpResults {
    songs: Option<AmpSongs>,
}

#[derive(Debug, Deserialize)]
struct AmpSongs {
    data: Option<Vec<AmpSong>>,
}

#[derive(Debug, Deserialize)]
struct AmpSong {
    id: Option<String>,
    attributes: Option<AmpAttributes>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AmpAttributes {
    name: Option<String>,
    artist_name: Option<String>,
    previews: Option<Vec<AmpPreview>>,
}

#[derive(Debug, Clone, Deserialize)]
struct AmpPreview {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItunesSearchResponse {
    results: Vec<ItunesTrack>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItunesTrack {
    track_id: Option<u64>,
    track_name: Option<String>,
    artist_name: Option<String>,
    preview_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LibrarySearchResponse {
    #[serde(default)]
    results: LibrarySearchResults,
}

#[derive(Debug, Deserialize, Default)]
struct LibrarySearchResults {
    #[serde(rename = "library-songs")]
    library_songs: Option<LibrarySongsBucket>,
}

#[derive(Debug, Deserialize)]
struct LibrarySongsBucket {
    data: Option<Vec<LibrarySong>>,
}

#[derive(Debug, Deserialize)]
struct LibrarySong {
    id: Option<String>,
    attributes: Option<LibrarySongAttributes>,
    relationships: Option<LibrarySongRelationships>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LibrarySongAttributes {
    name: Option<String>,
    artist_name: Option<String>,
    play_params: Option<LibraryPlayParams>,
    previews: Option<Vec<AmpPreview>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryPlayParams {
    catalog_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LibrarySongRelationships {
    catalog: Option<LibraryCatalogRelationship>,
}

#[derive(Debug, Deserialize)]
struct LibraryCatalogRelationship {
    data: Option<Vec<LibraryCatalogSong>>,
}

#[derive(Debug, Deserialize)]
struct LibraryCatalogSong {
    id: Option<String>,
}

fn find_m3u8_in_json(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            if s.contains(".m3u8") {
                Some(s.clone())
            } else {
                None
            }
        }
        Value::Array(items) => items.iter().find_map(find_m3u8_in_json),
        Value::Object(map) => map.values().find_map(find_m3u8_in_json),
        _ => None,
    }
}

fn extract_first_url_with(marker: &str, body: &str) -> Option<String> {
    for token in body.split_whitespace() {
        if token.contains(marker) && token.starts_with("http") {
            return Some(
                token
                    .trim_matches(|c| c == '"' || c == '\'' || c == ',' || c == ';')
                    .to_string(),
            );
        }
    }
    None
}

fn find_webplayback_asset_url(value: &Value) -> Option<String> {
    let song_list = value.get("songList")?.as_array()?;
    for song in song_list {
        let assets = song.get("assets").and_then(Value::as_array)?;
        for asset in assets {
            if let Some(url) = asset.get("URL").and_then(Value::as_str) {
                if url.contains(".m3u8") {
                    return Some(url.to_string());
                }
            }
        }
    }
    None
}
