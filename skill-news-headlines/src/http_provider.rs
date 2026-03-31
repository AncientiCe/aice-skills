use crate::types::{NewsHeadline, NewsHeadlinesError, NewsHeadlinesQuery, NewsHeadlinesResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::time::{Duration, SystemTime};
use urlencoding::encode;

#[async_trait]
pub trait NewsDataProvider: Send + Sync {
    async fn fetch(
        &self,
        query: &NewsHeadlinesQuery,
    ) -> Result<NewsHeadlinesResult, NewsHeadlinesError>;
}

#[derive(Clone)]
pub struct HttpNewsDataProvider {
    client: reqwest::Client,
    base_url: String,
}

impl HttpNewsDataProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.gdeltproject.org".to_string(),
        }
    }
}
impl Default for HttpNewsDataProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct GdeltResponse {
    articles: Option<Vec<GdeltArticle>>,
}
#[derive(Debug, Deserialize)]
struct GdeltArticle {
    title: Option<String>,
    domain: Option<String>,
    url: Option<String>,
    #[serde(rename = "seendate")]
    seen_date: Option<String>,
}

#[async_trait]
impl NewsDataProvider for HttpNewsDataProvider {
    async fn fetch(
        &self,
        query: &NewsHeadlinesQuery,
    ) -> Result<NewsHeadlinesResult, NewsHeadlinesError> {
        let topic = query.topic.trim();
        if topic.is_empty() {
            return Err(NewsHeadlinesError::InvalidQuery(
                "topic is empty".to_string(),
            ));
        }
        let limit = query.limit.unwrap_or(10).clamp(1, 20);
        let url = format!(
            "{}/api/v2/doc/doc?query={}&mode=ArtList&maxrecords={}&format=json",
            self.base_url,
            encode(topic),
            limit
        );
        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    NewsHeadlinesError::UpstreamTimeout
                } else {
                    NewsHeadlinesError::ProviderUnavailable(err.to_string())
                }
            })?;
        if !response.status().is_success() {
            return Err(NewsHeadlinesError::ProviderUnavailable(format!(
                "status {}",
                response.status()
            )));
        }
        let payload = response
            .json::<GdeltResponse>()
            .await
            .map_err(|err| NewsHeadlinesError::UpstreamParse(err.to_string()))?;
        let mut headlines = Vec::new();
        for item in payload.articles.unwrap_or_default() {
            if let Some(title) = item.title {
                headlines.push(NewsHeadline {
                    title,
                    source: item.domain,
                    url: item.url,
                    published_at: item.seen_date,
                });
            }
        }
        Ok(NewsHeadlinesResult {
            headlines,
            as_of: SystemTime::now(),
        })
    }
}
