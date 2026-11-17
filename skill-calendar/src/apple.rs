//! Apple Calendar.app provider via AppleScript (`osascript`).
//!
//! Compiles on every platform but execution always returns
//! `CalendarError::Unsupported` off macOS so the skill can still be linked
//! into cross-platform builds without conditional compilation at the call
//! site.

use crate::types::{
    CalendarAction, CalendarError, CalendarEvent, CalendarQuery, CalendarResult, CalendarSkill,
};
use async_trait::async_trait;
use jiff::{civil, tz::TimeZone, SignedDuration, Timestamp};
use metrics::{counter, histogram};
use std::process::Command;
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "calendar_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "calendar_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "apple_calendar";

#[derive(Clone)]
pub struct AppleCalendarSkill {
    dry_run: bool,
}

impl AppleCalendarSkill {
    pub fn new() -> Self {
        Self { dry_run: false }
    }

    pub fn new_for_tests() -> Self {
        Self { dry_run: true }
    }

    fn escape_applescript_string(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn run_script(&self, script: &str) -> Result<String, CalendarError> {
        if self.dry_run {
            return Ok(String::new());
        }
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| CalendarError::ProviderUnavailable(e.to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(CalendarError::ProviderUnavailable(if stderr.is_empty() {
                "osascript command failed".to_string()
            } else {
                stderr
            }));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn day_bounds_today() -> (Timestamp, Timestamp) {
        let now = Timestamp::now();
        let start = now
            .to_zoned_datetime(TimeZone::UTC)
            .date()
            .to_datetime(civil::Time::midnight())
            .to_zoned_datetime(TimeZone::UTC)
            .map(|z| z.timestamp())
            .unwrap_or(now);
        (
            start,
            start
                .saturating_add(SignedDuration::from_hours(24))
                .unwrap_or(start),
        )
    }

    fn day_bounds_tomorrow() -> (Timestamp, Timestamp) {
        let (s, _) = Self::day_bounds_today();
        let start = s
            .saturating_add(SignedDuration::from_hours(24))
            .unwrap_or(s);
        (
            start,
            start
                .saturating_add(SignedDuration::from_hours(24))
                .unwrap_or(start),
        )
    }

    fn upcoming_bounds(days: u32) -> (Timestamp, Timestamp) {
        let start = Timestamp::now();
        (
            start,
            start
                .saturating_add(SignedDuration::from_hours(i64::from(days) * 24))
                .unwrap_or(start),
        )
    }

    /// AppleScript to list events as one event per line in the form
    /// `YYYY-MM-DDTHH:MM:SS|YYYY-MM-DDTHH:MM:SS|UID|TITLE|LOCATION|CALENDAR`.
    pub(crate) fn build_list_script(start: Timestamp, end: Timestamp) -> String {
        let s = Self::format_for_applescript(start);
        let e = Self::format_for_applescript(end);
        format!(
            "set startDate to (current date)\n\
             set startDate to {s}\n\
             set endDate to (current date)\n\
             set endDate to {e}\n\
             set output to \"\"\n\
             tell application \"Calendar\"\n\
             repeat with c in calendars\n\
             set evs to (every event of c whose start date is greater than or equal to startDate \
             and start date is less than endDate)\n\
             repeat with ev in evs\n\
             set sd to start date of ev\n\
             set ed to end date of ev\n\
             set u to uid of ev\n\
             set t to summary of ev\n\
             try\n\
             set loc to location of ev\n\
             on error\n\
             set loc to \"\"\n\
             end try\n\
             set output to output & sd & \"|\" & ed & \"|\" & u & \"|\" & t & \"|\" & loc & \"|\" & (name of c) & linefeed\n\
             end repeat\n\
             end repeat\n\
             end tell\n\
             return output"
        )
    }

    fn format_for_applescript(dt: Timestamp) -> String {
        let dt = dt.to_zoned_datetime(TimeZone::UTC);
        format!(
            "(date \"{}/{}/{} {}:{}:{}\")",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second()
        )
    }

    pub(crate) fn build_create_script(query: &CalendarQuery) -> Result<String, CalendarError> {
        let title = query
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| CalendarError::InvalidQuery("missing event title".to_string()))?;
        let start = query
            .start
            .ok_or_else(|| CalendarError::InvalidQuery("missing start time".to_string()))?;
        let end = query.end.unwrap_or(
            start
                .saturating_add(SignedDuration::from_hours(1))
                .unwrap_or(start),
        );
        if end <= start {
            return Err(CalendarError::InvalidQuery(
                "end must be after start".to_string(),
            ));
        }
        let calendar = query.calendar_name.as_deref().unwrap_or("");
        let escaped_title = Self::escape_applescript_string(title);
        let calendar_clause = if calendar.is_empty() {
            "set targetCalendar to first calendar".to_string()
        } else {
            let escaped_cal = Self::escape_applescript_string(calendar);
            format!("set targetCalendar to (first calendar whose name is \"{escaped_cal}\")")
        };
        let location_clause = match query.location.as_deref().filter(|s| !s.is_empty()) {
            Some(loc) => format!(
                "set location of newEvent to \"{}\"",
                Self::escape_applescript_string(loc)
            ),
            None => String::new(),
        };
        let s = Self::format_for_applescript(start);
        let e = Self::format_for_applescript(end);
        Ok(format!(
            "tell application \"Calendar\"\n\
             {calendar_clause}\n\
             set startDate to (current date)\n\
             set startDate to {s}\n\
             set endDate to (current date)\n\
             set endDate to {e}\n\
             set newEvent to make new event at end of events of targetCalendar \
             with properties {{summary:\"{escaped_title}\", start date:startDate, end date:endDate}}\n\
             {location_clause}\n\
             return uid of newEvent\n\
             end tell"
        ))
    }
}

impl Default for AppleCalendarSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CalendarSkill for AppleCalendarSkill {
    async fn execute(&self, query: &CalendarQuery) -> Result<CalendarResult, CalendarError> {
        let started_at = Instant::now();
        let result = async {
            if !cfg!(target_os = "macos") && !self.dry_run {
                return Err(CalendarError::Unsupported(
                    "apple calendar provider requires macOS".to_string(),
                ));
            }
            match &query.action {
                CalendarAction::ListToday => {
                    let (a, b) = Self::day_bounds_today();
                    let raw = self.run_script(&Self::build_list_script(a, b))?;
                    let events = parse_event_lines(&raw);
                    Ok(CalendarResult {
                        summary: format!("{} event(s) today", events.len()),
                        events,
                        created_event_id: None,
                    })
                }
                CalendarAction::ListTomorrow => {
                    let (a, b) = Self::day_bounds_tomorrow();
                    let raw = self.run_script(&Self::build_list_script(a, b))?;
                    let events = parse_event_lines(&raw);
                    Ok(CalendarResult {
                        summary: format!("{} event(s) tomorrow", events.len()),
                        events,
                        created_event_id: None,
                    })
                }
                CalendarAction::ListUpcoming { days } => {
                    let (a, b) = Self::upcoming_bounds(*days);
                    let raw = self.run_script(&Self::build_list_script(a, b))?;
                    let events = parse_event_lines(&raw);
                    Ok(CalendarResult {
                        summary: format!("{} event(s) in next {days} day(s)", events.len()),
                        events,
                        created_event_id: None,
                    })
                }
                CalendarAction::Create => {
                    let script = Self::build_create_script(query)?;
                    let uid = self.run_script(&script)?;
                    let title = query.title.clone().unwrap_or_else(|| "(untitled)".into());
                    let start = query.start.unwrap_or_else(Timestamp::now);
                    let end = query.end.unwrap_or(
                        start
                            .saturating_add(SignedDuration::from_hours(1))
                            .unwrap_or(start),
                    );
                    let event = CalendarEvent {
                        id: if uid.is_empty() {
                            "dry-run".to_string()
                        } else {
                            uid
                        },
                        title: title.clone(),
                        start,
                        end,
                        all_day: false,
                        location: query.location.clone(),
                        calendar_name: query.calendar_name.clone(),
                    };
                    Ok(CalendarResult {
                        summary: format!(
                            "Created \"{title}\" at {}",
                            start.strftime("%Y-%m-%d %H:%M UTC")
                        ),
                        created_event_id: Some(event.id.clone()),
                        events: vec![event],
                    })
                }
            }
        }
        .await;

        let mut result_label = "success";
        let mut error_kind = "none";
        if let Err(e) = &result {
            result_label = "error";
            error_kind = match e {
                CalendarError::InvalidQuery(_) => "invalid_query",
                CalendarError::Auth(_) => "auth",
                CalendarError::ProviderUnavailable(_) => "provider_unavailable",
                CalendarError::NotFound(_) => "not_found",
                CalendarError::Unsupported(_) => "unsupported",
            };
        }
        counter!(
            SKILL_EXECUTE_TOTAL,
            1,
            "skill" => SKILL_LABEL,
            "result" => result_label,
            "error_kind" => error_kind
        );
        histogram!(
            SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => SKILL_LABEL
        );
        result
    }
}

/// AppleScript date coercion of `(date as string)` is locale-dependent. We
/// best-effort parse a few common forms and silently drop unparseable lines.
fn parse_event_lines(raw: &str) -> Vec<CalendarEvent> {
    let mut out = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            continue;
        }
        let start = parse_apple_date(parts[0].trim());
        let end = parse_apple_date(parts[1].trim());
        let (start, end) = match (start, end) {
            (Some(s), Some(e)) => (s, e),
            _ => continue,
        };
        out.push(CalendarEvent {
            id: parts[2].trim().to_string(),
            title: parts[3].trim().to_string(),
            start,
            end,
            all_day: false,
            location: parts
                .get(4)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            calendar_name: parts
                .get(5)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        });
    }
    out
}

fn parse_apple_date(s: &str) -> Option<Timestamp> {
    // Expected coercion (en_US locale): "Saturday, April 18, 2026 at 9:00:00 AM"
    let cleaned = s.replace(" at ", " ");
    for fmt in [
        "%A, %B %d, %Y %I:%M:%S %p",
        "%a %b %d %H:%M:%S %Y",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(datetime) =
            jiff::fmt::strtime::parse(fmt, &cleaned).and_then(|tm| tm.to_datetime())
        {
            return datetime
                .to_zoned_datetime(TimeZone::UTC)
                .ok()
                .map(|z| z.timestamp());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> Timestamp {
        s.parse().expect("timestamp")
    }

    #[test]
    fn parse_apple_date_reads_every_supported_layout_as_utc() {
        let expected = ts("2026-04-18T09:05:00Z");
        for raw in [
            "Saturday, April 18, 2026 at 9:05:00 AM",
            "Sat Apr 18 09:05:00 2026",
            "2026-04-18 09:05:00",
            "2026-04-18T09:05:00",
        ] {
            assert_eq!(parse_apple_date(raw), Some(expected), "{raw}");
        }
        assert_eq!(
            parse_apple_date("Saturday, April 18, 2026 at 9:05:00 PM"),
            Some(ts("2026-04-18T21:05:00Z"))
        );
        assert_eq!(parse_apple_date("not a date"), None);
    }

    #[test]
    fn format_for_applescript_is_unpadded_utc() {
        assert_eq!(
            AppleCalendarSkill::format_for_applescript(ts("2026-04-08T09:05:07Z")),
            "(date \"2026/4/8 9:5:7\")"
        );
    }

    #[tokio::test]
    async fn list_today_returns_zero_in_dry_run() {
        let skill = AppleCalendarSkill::new_for_tests();
        let q = CalendarQuery {
            action: CalendarAction::ListToday,
            title: None,
            start: None,
            end: None,
            location: None,
            calendar_name: None,
        };
        let res = skill.execute(&q).await.expect("ok");
        assert_eq!(res.events.len(), 0);
        assert!(res.summary.contains("today"));
    }

    #[tokio::test]
    async fn create_dry_run_returns_event() {
        let skill = AppleCalendarSkill::new_for_tests();
        let start = Timestamp::now();
        let q = CalendarQuery {
            action: CalendarAction::Create,
            title: Some("Lunch".to_string()),
            start: Some(start),
            end: Some(
                start
                    .saturating_add(SignedDuration::from_hours(1))
                    .unwrap_or(start),
            ),
            location: Some("Cafeteria".to_string()),
            calendar_name: None,
        };
        let res = skill.execute(&q).await.expect("ok");
        assert_eq!(res.created_event_id.as_deref(), Some("dry-run"));
        assert_eq!(res.events.len(), 1);
        assert_eq!(res.events[0].title, "Lunch");
    }

    #[tokio::test]
    async fn create_requires_title() {
        let skill = AppleCalendarSkill::new_for_tests();
        let q = CalendarQuery {
            action: CalendarAction::Create,
            title: None,
            start: Some(Timestamp::now()),
            end: None,
            location: None,
            calendar_name: None,
        };
        let err = skill.execute(&q).await.expect_err("err");
        assert!(matches!(err, CalendarError::InvalidQuery(_)));
    }

    #[test]
    fn build_create_script_includes_calendar_lookup() {
        let start = Timestamp::now();
        let q = CalendarQuery {
            action: CalendarAction::Create,
            title: Some("Standup".to_string()),
            start: Some(start),
            end: None,
            location: None,
            calendar_name: Some("Work".to_string()),
        };
        let script = AppleCalendarSkill::build_create_script(&q).expect("script");
        assert!(script.contains("first calendar whose name is \"Work\""));
        assert!(script.contains("Standup"));
        assert!(script.contains("return uid of newEvent"));
    }

    #[test]
    fn escape_applescript_string_escapes_quotes() {
        let s = AppleCalendarSkill::escape_applescript_string("Lunch \"with team\"");
        assert_eq!(s, "Lunch \\\"with team\\\"");
    }
}
