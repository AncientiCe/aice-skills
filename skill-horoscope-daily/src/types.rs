use async_trait::async_trait;
use chrono::NaiveDate;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct HoroscopeDailyQuery {
    pub sign: String,
    pub date: Option<NaiveDate>,
}

#[derive(Clone, Debug)]
pub struct HoroscopeDailyResult {
    pub sign: String,
    pub day: String,
    pub summary: String,
    pub mood: Option<String>,
    pub color: Option<String>,
    pub lucky_number: Option<String>,
    pub as_of: SystemTime,
}

impl HoroscopeDailyResult {
    pub fn to_prompt_context(&self) -> String {
        format!("{} horoscope for {}: {}", self.sign, self.day, self.summary)
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum HoroscopeDailyError {
    #[error("invalid sign: {0}")]
    InvalidSign(String),
    #[error("unsupported date: {0}")]
    UnsupportedDate(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("provider timeout")]
    UpstreamTimeout,
    #[error("provider response parse failed: {0}")]
    UpstreamParse(String),
}

#[async_trait]
pub trait HoroscopeDailySkill: Send + Sync {
    async fn execute(
        &self,
        query: &HoroscopeDailyQuery,
    ) -> Result<HoroscopeDailyResult, HoroscopeDailyError>;
}
