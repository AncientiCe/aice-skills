//! Calendar skill: cross-platform interface with multiple providers.
//!
//! - `AppleCalendarSkill` — macOS-only, AppleScript bridge to Calendar.app.
//! - `GoogleCalendarSkill` — cross-platform Google Calendar Web API + OAuth.
//!
//! Both implement the shared `CalendarSkill` trait so the runtime can pick
//! one (or compose several) based on configuration.

mod apple;
mod google;
mod types;

pub use apple::AppleCalendarSkill;
pub use google::{
    GoogleCalendarAuth, GoogleCalendarRequest, GoogleCalendarResponse, GoogleCalendarSkill,
    GoogleCalendarTransport, HttpGoogleCalendarTransport, StaticGoogleCalendarAuth,
};
pub use types::{
    CalendarAction, CalendarError, CalendarEvent, CalendarQuery, CalendarResult, CalendarSkill,
};

use async_trait::async_trait;

/// Mock skill returning a canned result. Useful for wiring tests.
pub struct MockCalendarSkill {
    pub result: Result<CalendarResult, CalendarError>,
}

impl MockCalendarSkill {
    pub fn ok(result: CalendarResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: CalendarError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl CalendarSkill for MockCalendarSkill {
    async fn execute(&self, _query: &CalendarQuery) -> Result<CalendarResult, CalendarError> {
        self.result.clone()
    }
}
