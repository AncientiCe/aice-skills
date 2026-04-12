use crate::session::SessionSnapshot;
use anyhow::{bail, Context, Result};
use serde::Deserialize;

const ITUNES_SEARCH_URL: &str = "https://itunes.apple.com/search";

#[derive(Clone, Debug)]
pub struct Track {
    pub id: String,
    pub resolve_id: String,
    pub track_name: String,
    pub artist_name: String,
}

pub struct AppleMusicClient {
    session: SessionSnapshot,
    client: reqwest::Client,
}

impl AppleMusicClient {
    pub fn new(session: SessionSnapshot) -> Self {
        Self {
            session,
            client: reqwest::Client::new(),
        }
    }

    pub async fn effective_storefront(&self) -> Result<String> {
        Ok(self
            .session
            .storefront
            .clone()
            .unwrap_or_else(|| "us".to_string()))
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Track>> {
        let response = self
            .client
            .get(ITUNES_SEARCH_URL)
            .query(&[("term", query), ("entity", "song"), ("limit", "5")])
            .send()
            .await
            .context("failed to query iTunes search")?;

        if !response.status().is_success() {
            bail!("catalog search failed with status {}", response.status());
        }

        let body: SearchResponse = response
            .json()
            .await
            .context("failed to decode iTunes search response")?;

        Ok(body
            .results
            .into_iter()
            .filter_map(|item| {
                if item.track_id == 0 || item.track_name.is_empty() {
                    None
                } else {
                    let id = item.track_id.to_string();
                    Some(Track {
                        id: id.clone(),
                        resolve_id: id,
                        track_name: item.track_name,
                        artist_name: item.artist_name,
                    })
                }
            })
            .collect())
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    #[serde(default, rename = "trackId")]
    track_id: u64,
    #[serde(default, rename = "trackName")]
    track_name: String,
    #[serde(default, rename = "artistName")]
    artist_name: String,
}
