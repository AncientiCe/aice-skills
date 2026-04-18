//! Personalized briefing skill: one command produces a single spoken briefing
//! by fanning out to weather, calendar, email, and news headlines in
//! parallel. Any single sub-skill failing yields a `BriefingSection::Error`
//! rather than failing the whole briefing.

use async_trait::async_trait;
use metrics::{counter, histogram};
use skill_calendar::{CalendarAction, CalendarError, CalendarQuery, CalendarResult, CalendarSkill};
use skill_email::{EmailAction, EmailError, EmailQuery, EmailResult, EmailSkill};
use skill_news_headlines::{
    NewsHeadlinesError, NewsHeadlinesQuery, NewsHeadlinesResult, NewsHeadlinesSkill,
};
use skill_weather::{ResolvedLocation, WeatherResult, WeatherSkill, WeatherSkillError};
use std::sync::Arc;
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "briefing";

#[derive(Clone, Debug, PartialEq)]
pub struct BriefingQuery {
    pub greeting: Option<String>,
    pub include_weather: bool,
    pub include_calendar: bool,
    pub include_email: bool,
    pub include_news: bool,
    pub weather_location: Option<String>,
    pub email_limit: usize,
    pub news_topic: String,
    pub news_country: Option<String>,
    pub news_limit: usize,
}

impl Default for BriefingQuery {
    fn default() -> Self {
        Self {
            greeting: None,
            include_weather: true,
            include_calendar: true,
            include_email: true,
            include_news: true,
            weather_location: None,
            email_limit: 5,
            news_topic: "top".to_string(),
            news_country: None,
            news_limit: 5,
        }
    }
}

#[derive(Clone, Debug)]
pub enum BriefingSection {
    Weather(Result<WeatherResult, WeatherSkillError>),
    Calendar(Result<CalendarResult, CalendarError>),
    Email(Result<EmailResult, EmailError>),
    News(Result<NewsHeadlinesResult, NewsHeadlinesError>),
}

#[derive(Clone, Debug)]
pub struct BriefingResult {
    pub greeting: String,
    pub sections: Vec<BriefingSection>,
}

impl BriefingResult {
    pub fn to_prompt_context(&self) -> String {
        let mut out = vec![self.greeting.clone()];
        for s in &self.sections {
            match s {
                BriefingSection::Weather(Ok(w)) => {
                    out.push(format!("Weather: {}", w.to_prompt_context()))
                }
                BriefingSection::Weather(Err(e)) => out.push(format!("Weather unavailable: {e}")),
                BriefingSection::Calendar(Ok(c)) => {
                    out.push(format!("Calendar: {}", c.to_prompt_context()))
                }
                BriefingSection::Calendar(Err(e)) => out.push(format!("Calendar unavailable: {e}")),
                BriefingSection::Email(Ok(e)) => {
                    out.push(format!("Email: {}", e.to_prompt_context()))
                }
                BriefingSection::Email(Err(e)) => out.push(format!("Email unavailable: {e}")),
                BriefingSection::News(Ok(n)) => {
                    out.push(format!("News: {}", n.to_prompt_context()))
                }
                BriefingSection::News(Err(e)) => out.push(format!("News unavailable: {e}")),
            }
        }
        out.join("\n\n")
    }

    pub fn ok_section_count(&self) -> usize {
        self.sections
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    BriefingSection::Weather(Ok(_))
                        | BriefingSection::Calendar(Ok(_))
                        | BriefingSection::Email(Ok(_))
                        | BriefingSection::News(Ok(_))
                )
            })
            .count()
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum BriefingError {
    #[error("no sections enabled")]
    NoSectionsEnabled,
}

#[async_trait]
pub trait BriefingSkill: Send + Sync {
    async fn execute(&self, query: &BriefingQuery) -> Result<BriefingResult, BriefingError>;
}

pub struct ComposedBriefingSkill {
    weather: Option<Arc<dyn WeatherSkill>>,
    default_location: Option<ResolvedLocation>,
    calendar: Option<Arc<dyn CalendarSkill>>,
    email: Option<Arc<dyn EmailSkill>>,
    news: Option<Arc<dyn NewsHeadlinesSkill>>,
}

impl ComposedBriefingSkill {
    pub fn new() -> Self {
        Self {
            weather: None,
            default_location: None,
            calendar: None,
            email: None,
            news: None,
        }
    }

    pub fn with_weather(
        mut self,
        weather: Arc<dyn WeatherSkill>,
        default_location: Option<ResolvedLocation>,
    ) -> Self {
        self.weather = Some(weather);
        self.default_location = default_location;
        self
    }

    pub fn with_calendar(mut self, calendar: Arc<dyn CalendarSkill>) -> Self {
        self.calendar = Some(calendar);
        self
    }

    pub fn with_email(mut self, email: Arc<dyn EmailSkill>) -> Self {
        self.email = Some(email);
        self
    }

    pub fn with_news(mut self, news: Arc<dyn NewsHeadlinesSkill>) -> Self {
        self.news = Some(news);
        self
    }

    fn default_greeting() -> String {
        let hour = chrono_hour();
        match hour {
            Some(h) if h < 12 => "Good morning.".to_string(),
            Some(h) if h < 18 => "Good afternoon.".to_string(),
            Some(_) => "Good evening.".to_string(),
            None => "Hello.".to_string(),
        }
    }
}

impl Default for ComposedBriefingSkill {
    fn default() -> Self {
        Self::new()
    }
}

fn chrono_hour() -> Option<u32> {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?;
    Some(((now.as_secs() / 3600) % 24) as u32)
}

#[async_trait]
impl BriefingSkill for ComposedBriefingSkill {
    async fn execute(&self, query: &BriefingQuery) -> Result<BriefingResult, BriefingError> {
        if !(query.include_weather
            || query.include_calendar
            || query.include_email
            || query.include_news)
        {
            return Err(BriefingError::NoSectionsEnabled);
        }
        let started_at = Instant::now();
        let weather_fut = async {
            if !query.include_weather {
                return None;
            }
            let Some(skill) = &self.weather else {
                return None;
            };
            Some(BriefingSection::Weather(
                skill
                    .execute(
                        query.weather_location.as_deref(),
                        self.default_location.as_ref(),
                    )
                    .await,
            ))
        };
        let calendar_fut = async {
            if !query.include_calendar {
                return None;
            }
            let Some(skill) = &self.calendar else {
                return None;
            };
            Some(BriefingSection::Calendar(
                skill
                    .execute(&CalendarQuery {
                        action: CalendarAction::ListToday,
                        title: None,
                        start: None,
                        end: None,
                        location: None,
                        calendar_name: None,
                    })
                    .await,
            ))
        };
        let email_fut = async {
            if !query.include_email {
                return None;
            }
            let Some(skill) = &self.email else {
                return None;
            };
            Some(BriefingSection::Email(
                skill
                    .execute(&EmailQuery {
                        action: EmailAction::ListUnread {
                            limit: query.email_limit,
                        },
                        mailbox: None,
                    })
                    .await,
            ))
        };
        let news_fut = async {
            if !query.include_news {
                return None;
            }
            let Some(skill) = &self.news else {
                return None;
            };
            Some(BriefingSection::News(
                skill
                    .execute(&NewsHeadlinesQuery {
                        topic: query.news_topic.clone(),
                        country_code: query.news_country.clone(),
                        limit: Some(query.news_limit),
                    })
                    .await,
            ))
        };

        let (w, c, e, n) = tokio::join!(weather_fut, calendar_fut, email_fut, news_fut);
        let mut sections = Vec::new();
        if let Some(s) = w {
            sections.push(s);
        }
        if let Some(s) = c {
            sections.push(s);
        }
        if let Some(s) = e {
            sections.push(s);
        }
        if let Some(s) = n {
            sections.push(s);
        }

        let result = BriefingResult {
            greeting: query
                .greeting
                .clone()
                .unwrap_or_else(Self::default_greeting),
            sections,
        };

        counter!(
            SKILL_EXECUTE_TOTAL,
            1,
            "skill" => SKILL_LABEL,
            "result" => "success",
            "error_kind" => "none"
        );
        histogram!(
            SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => SKILL_LABEL
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_calendar::{CalendarEvent, CalendarResult, MockCalendarSkill};
    use skill_email::{EmailMessage, EmailResult, MockEmailSkill};
    use skill_news_headlines::{MockNewsHeadlinesSkill, NewsHeadline, NewsHeadlinesResult};
    use skill_weather::{MockWeatherSkill, WeatherResult};

    fn weather_ok() -> Arc<dyn WeatherSkill> {
        Arc::new(MockWeatherSkill::ok(WeatherResult {
            location_display: "Paris".into(),
            temp_c: 18.0,
            humidity_pct: Some(60),
            weather_code: 0,
            description: "Clear".into(),
            daily_forecast: Vec::new(),
            alerts: Vec::new(),
        }))
    }

    fn calendar_ok() -> Arc<dyn CalendarSkill> {
        Arc::new(MockCalendarSkill::ok(CalendarResult {
            summary: "1 event today".into(),
            events: vec![CalendarEvent {
                id: "ev1".into(),
                title: "Standup".into(),
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
                all_day: false,
                location: None,
                calendar_name: None,
            }],
            created_event_id: None,
        }))
    }

    fn email_ok() -> Arc<dyn EmailSkill> {
        Arc::new(MockEmailSkill::ok(EmailResult {
            summary: "2 unread".into(),
            messages: vec![EmailMessage {
                id: "1".into(),
                sender: "alice".into(),
                subject: "hi".into(),
                snippet: "hello".into(),
                received_at: None,
                unread: true,
                flagged: false,
                mailbox: None,
            }],
            triage_buckets: Vec::new(),
        }))
    }

    fn news_ok() -> Arc<dyn NewsHeadlinesSkill> {
        Arc::new(MockNewsHeadlinesSkill::ok(NewsHeadlinesResult {
            headlines: vec![NewsHeadline {
                title: "Headline A".into(),
                source: None,
                url: None,
                published_at: None,
            }],
            as_of: std::time::SystemTime::now(),
        }))
    }

    #[tokio::test]
    async fn full_briefing_runs_all_sections() {
        let skill = ComposedBriefingSkill::new()
            .with_weather(weather_ok(), None)
            .with_calendar(calendar_ok())
            .with_email(email_ok())
            .with_news(news_ok());
        let res = skill.execute(&BriefingQuery::default()).await.expect("ok");
        assert_eq!(res.sections.len(), 4);
        assert_eq!(res.ok_section_count(), 4);
    }

    #[tokio::test]
    async fn missing_skill_skips_section() {
        let skill = ComposedBriefingSkill::new()
            .with_weather(weather_ok(), None)
            .with_news(news_ok());
        let res = skill.execute(&BriefingQuery::default()).await.expect("ok");
        assert_eq!(res.sections.len(), 2);
    }

    #[tokio::test]
    async fn no_sections_enabled_errors() {
        let skill = ComposedBriefingSkill::new();
        let q = BriefingQuery {
            include_weather: false,
            include_calendar: false,
            include_email: false,
            include_news: false,
            ..Default::default()
        };
        let err = skill.execute(&q).await.expect_err("err");
        assert!(matches!(err, BriefingError::NoSectionsEnabled));
    }

    #[tokio::test]
    async fn failed_section_does_not_fail_briefing() {
        let failing_email: Arc<dyn EmailSkill> = Arc::new(MockEmailSkill::err(
            EmailError::ProviderUnavailable("imap down".into()),
        ));
        let skill = ComposedBriefingSkill::new()
            .with_weather(weather_ok(), None)
            .with_email(failing_email);
        let res = skill.execute(&BriefingQuery::default()).await.expect("ok");
        assert_eq!(res.sections.len(), 2);
        assert_eq!(res.ok_section_count(), 1);
    }
}
