//! Calendar skill types and traits shared across providers (Apple Calendar,
//! Google Calendar, ...).

use async_trait::async_trait;
use jiff::Timestamp;

#[derive(Clone, Debug, PartialEq)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: Timestamp,
    pub end: Timestamp,
    pub all_day: bool,
    pub location: Option<String>,
    pub calendar_name: Option<String>,
}

impl CalendarEvent {
    pub fn duration_minutes(&self) -> i64 {
        (self.end - self.start).as_mins()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CalendarAction {
    ListToday,
    ListTomorrow,
    ListUpcoming { days: u32 },
    Create,
}

impl CalendarAction {
    pub fn parse(action: &str, days: Option<u32>) -> Option<Self> {
        match action.trim().to_lowercase().as_str() {
            "list_today" | "today" => Some(Self::ListToday),
            "list_tomorrow" | "tomorrow" => Some(Self::ListTomorrow),
            "list_upcoming" | "upcoming" | "list" => Some(Self::ListUpcoming {
                days: days.unwrap_or(7).clamp(1, 31),
            }),
            "create" | "add" | "schedule" => Some(Self::Create),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalendarQuery {
    pub action: CalendarAction,
    pub title: Option<String>,
    pub start: Option<Timestamp>,
    pub end: Option<Timestamp>,
    pub location: Option<String>,
    pub calendar_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalendarResult {
    pub summary: String,
    pub events: Vec<CalendarEvent>,
    pub created_event_id: Option<String>,
}

impl CalendarResult {
    pub fn to_prompt_context(&self) -> String {
        if self.events.is_empty() {
            return self.summary.clone();
        }
        let mut lines = Vec::with_capacity(self.events.len() + 1);
        lines.push(self.summary.clone());
        for ev in &self.events {
            let when = if ev.all_day {
                ev.start.strftime("%Y-%m-%d (all day)").to_string()
            } else {
                format!(
                    "{} - {}",
                    ev.start.strftime("%Y-%m-%d %H:%M"),
                    ev.end.strftime("%H:%M")
                )
            };
            let loc = ev
                .location
                .as_deref()
                .map(|l| format!(" @ {l}"))
                .unwrap_or_default();
            lines.push(format!("- {when}: {}{loc}", ev.title));
        }
        lines.join("\n")
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum CalendarError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("auth error: {0}")]
    Auth(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unsupported action: {0}")]
    Unsupported(String),
}

#[async_trait]
pub trait CalendarSkill: Send + Sync {
    async fn execute(&self, query: &CalendarQuery) -> Result<CalendarResult, CalendarError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_handles_aliases() {
        assert_eq!(
            CalendarAction::parse("today", None),
            Some(CalendarAction::ListToday)
        );
        assert_eq!(
            CalendarAction::parse("upcoming", Some(3)),
            Some(CalendarAction::ListUpcoming { days: 3 })
        );
        assert_eq!(
            CalendarAction::parse("schedule", None),
            Some(CalendarAction::Create)
        );
        assert_eq!(CalendarAction::parse("nope", None), None);
    }

    #[test]
    fn upcoming_clamps_days() {
        assert_eq!(
            CalendarAction::parse("upcoming", Some(0)),
            Some(CalendarAction::ListUpcoming { days: 1 })
        );
        assert_eq!(
            CalendarAction::parse("upcoming", Some(99)),
            Some(CalendarAction::ListUpcoming { days: 31 })
        );
    }
}
