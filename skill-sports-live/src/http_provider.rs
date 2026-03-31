use crate::types::{SportsEvent, SportsLiveError, SportsLiveQuery, SportsLiveResult};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::Deserialize;
use std::time::{Duration, SystemTime};
use urlencoding::encode;

#[async_trait]
pub trait SportsDataProvider: Send + Sync {
    async fn fetch(&self, query: &SportsLiveQuery) -> Result<SportsLiveResult, SportsLiveError>;
}

#[derive(Clone)]
pub struct HttpSportsDataProvider {
    client: reqwest::Client,
    base_url: String,
}

impl HttpSportsDataProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://www.thesportsdb.com".to_string(),
        }
    }
}

impl Default for HttpSportsDataProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct SportsResponse {
    #[serde(rename = "event")]
    events: Option<Vec<EventItem>>,
}

#[derive(Debug, Deserialize)]
struct EventItem {
    #[serde(rename = "strEvent")]
    event: Option<String>,
    #[serde(rename = "strLeague")]
    league: Option<String>,
    #[serde(rename = "strHomeTeam")]
    home_team: Option<String>,
    #[serde(rename = "strAwayTeam")]
    away_team: Option<String>,
    #[serde(rename = "strTimestamp")]
    timestamp: Option<String>,
    #[serde(rename = "strStatus")]
    status: Option<String>,
    #[serde(rename = "intHomeScore")]
    home_score: Option<String>,
    #[serde(rename = "intAwayScore")]
    away_score: Option<String>,
    #[serde(rename = "strHomeGoalDetails")]
    home_goal_details: Option<String>,
    #[serde(rename = "strAwayGoalDetails")]
    away_goal_details: Option<String>,
}

#[async_trait]
impl SportsDataProvider for HttpSportsDataProvider {
    async fn fetch(&self, query: &SportsLiveQuery) -> Result<SportsLiveResult, SportsLiveError> {
        let normalized = query.query.trim();
        if normalized.is_empty() {
            return Err(SportsLiveError::InvalidQuery("query is empty".to_string()));
        }
        let url = format!(
            "{}/api/v1/json/123/searchevents.php?e={}",
            self.base_url,
            encode(normalized)
        );
        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    SportsLiveError::UpstreamTimeout
                } else {
                    SportsLiveError::ProviderUnavailable(err.to_string())
                }
            })?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(SportsLiveResult {
                events: vec![],
                as_of: SystemTime::now(),
            });
        }
        if !response.status().is_success() {
            return Err(SportsLiveError::ProviderUnavailable(format!(
                "status {}",
                response.status()
            )));
        }

        let payload = response
            .json::<SportsResponse>()
            .await
            .map_err(|err| SportsLiveError::UpstreamParse(err.to_string()))?;
        let mut events = Vec::new();
        for item in payload.events.unwrap_or_default() {
            let mut scorers = Vec::new();
            if let Some(home) = item.home_goal_details {
                scorers.extend(home.split(";").map(|e| e.trim().to_string()));
            }
            if let Some(away) = item.away_goal_details {
                scorers.extend(away.split(";").map(|e| e.trim().to_string()));
            }
            scorers.retain(|e| !e.is_empty());
            events.push(SportsEvent {
                league: item.league,
                event: item.event.unwrap_or_else(|| "Unknown event".to_string()),
                home_team: item.home_team,
                away_team: item.away_team,
                start_time: item.timestamp,
                status: item.status,
                home_score: item.home_score.and_then(|x| x.parse::<i32>().ok()),
                away_score: item.away_score.and_then(|x| x.parse::<i32>().ok()),
                scorers,
            });
        }
        Ok(SportsLiveResult {
            events,
            as_of: SystemTime::now(),
        })
    }
}
