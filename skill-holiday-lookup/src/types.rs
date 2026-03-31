use async_trait::async_trait;
use chrono::NaiveDate;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct HolidayQuery {
    pub holiday_name: Option<String>,
    pub date: Option<NaiveDate>,
    pub country_code: String,
    pub region_code: Option<String>,
    pub year: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HolidayMatch {
    pub name: String,
    pub date: NaiveDate,
    pub is_public_holiday: bool,
    pub local_name: Option<String>,
    pub observed_date: Option<NaiveDate>,
}

#[derive(Clone, Debug)]
pub struct HolidayLookupResult {
    pub country_code: String,
    pub region_code: Option<String>,
    pub matches: Vec<HolidayMatch>,
    pub as_of: SystemTime,
}

impl HolidayLookupResult {
    pub fn to_prompt_context(&self) -> String {
        if self.matches.is_empty() {
            return format!("No holiday matches found for {}.", self.country_code);
        }

        let summary = self
            .matches
            .iter()
            .map(|entry| format!("{} on {}", entry.name, entry.date.format("%Y-%m-%d")))
            .collect::<Vec<_>>()
            .join("; ");

        format!("Holiday matches in {}: {}.", self.country_code, summary)
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum HolidayLookupError {
    #[error("invalid country code: {0}")]
    InvalidCountry(String),
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
pub trait HolidayLookupSkill: Send + Sync {
    async fn execute(
        &self,
        query: &HolidayQuery,
    ) -> Result<HolidayLookupResult, HolidayLookupError>;
}
