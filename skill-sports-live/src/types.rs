use async_trait::async_trait;
use chrono::NaiveDate;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct SportsLiveQuery {
    pub query: String,
    pub date: Option<NaiveDate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SportsEvent {
    pub league: Option<String>,
    pub event: String,
    pub home_team: Option<String>,
    pub away_team: Option<String>,
    pub start_time: Option<String>,
    pub status: Option<String>,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub scorers: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SportsLiveResult {
    pub events: Vec<SportsEvent>,
    pub as_of: SystemTime,
}

impl SportsLiveResult {
    pub fn to_prompt_context(&self) -> String {
        if self.events.is_empty() {
            return "No sports events found.".to_string();
        }
        let summary = self
            .events
            .iter()
            .map(|event| event.event.clone())
            .collect::<Vec<_>>()
            .join("; ");
        format!("Sports events: {}.", summary)
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum SportsLiveError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("provider timeout")]
    UpstreamTimeout,
    #[error("provider response parse failed: {0}")]
    UpstreamParse(String),
}

#[async_trait]
pub trait SportsLiveSkill: Send + Sync {
    async fn execute(&self, query: &SportsLiveQuery) -> Result<SportsLiveResult, SportsLiveError>;
}
