//! Google Calendar provider (cross-platform Web API + OAuth).
//!
//! The OAuth flow is owned by the runtime; this module consumes a small
//! `GoogleCalendarAuth` trait that returns a fresh access token. The HTTP
//! layer is abstracted behind `GoogleCalendarTransport` so the skill can be
//! tested without making network calls.

use crate::types::{
    CalendarAction, CalendarError, CalendarEvent, CalendarQuery, CalendarResult, CalendarSkill,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use metrics::{counter, histogram};
use std::sync::Arc;
use std::time::Instant;

const GOOGLE_API_BASE: &str = "https://www.googleapis.com/calendar/v3";
const SKILL_EXECUTE_TOTAL: &str = "calendar_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "calendar_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "google_calendar";

#[derive(Clone, Debug)]
pub struct GoogleCalendarRequest {
    pub method: String,
    pub path: String,
    pub access_token: String,
    pub body: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GoogleCalendarResponse {
    pub status: u16,
    pub body: Option<String>,
}

#[async_trait]
pub trait GoogleCalendarAuth: Send + Sync {
    async fn access_token(&self) -> Result<String, String>;
}

pub struct StaticGoogleCalendarAuth {
    token: String,
}

impl StaticGoogleCalendarAuth {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

#[async_trait]
impl GoogleCalendarAuth for StaticGoogleCalendarAuth {
    async fn access_token(&self) -> Result<String, String> {
        Ok(self.token.clone())
    }
}

#[async_trait]
pub trait GoogleCalendarTransport: Send + Sync {
    async fn execute(
        &self,
        req: GoogleCalendarRequest,
    ) -> Result<GoogleCalendarResponse, CalendarError>;
}

#[derive(Clone)]
pub struct HttpGoogleCalendarTransport {
    client: reqwest::Client,
}

impl HttpGoogleCalendarTransport {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for HttpGoogleCalendarTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GoogleCalendarTransport for HttpGoogleCalendarTransport {
    async fn execute(
        &self,
        req: GoogleCalendarRequest,
    ) -> Result<GoogleCalendarResponse, CalendarError> {
        let url = if req.path.starts_with("http") {
            req.path.clone()
        } else {
            format!("{GOOGLE_API_BASE}{}", req.path)
        };
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|e| CalendarError::ProviderUnavailable(e.to_string()))?;
        let mut builder = self
            .client
            .request(method, &url)
            .bearer_auth(&req.access_token);
        if let Some(body) = req.body.as_ref() {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone());
        }
        let resp = builder
            .send()
            .await
            .map_err(|e| CalendarError::ProviderUnavailable(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| CalendarError::ProviderUnavailable(e.to_string()))?;
        Ok(GoogleCalendarResponse {
            status,
            body: if body.is_empty() { None } else { Some(body) },
        })
    }
}

#[derive(Clone)]
pub struct GoogleCalendarSkill {
    auth: Arc<dyn GoogleCalendarAuth>,
    transport: Arc<dyn GoogleCalendarTransport>,
}

impl GoogleCalendarSkill {
    pub fn new(
        auth: Arc<dyn GoogleCalendarAuth>,
        transport: Arc<dyn GoogleCalendarTransport>,
    ) -> Self {
        Self { auth, transport }
    }

    fn error_kind(error: &CalendarError) -> &'static str {
        match error {
            CalendarError::InvalidQuery(_) => "invalid_query",
            CalendarError::Auth(_) => "auth",
            CalendarError::ProviderUnavailable(_) => "provider_unavailable",
            CalendarError::NotFound(_) => "not_found",
            CalendarError::Unsupported(_) => "unsupported",
        }
    }

    async fn token(&self) -> Result<String, CalendarError> {
        self.auth.access_token().await.map_err(CalendarError::Auth)
    }

    fn day_bounds_today() -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();
        let start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|n| Utc.from_utc_datetime(&n))
            .unwrap_or(now);
        (start, start + Duration::days(1))
    }

    fn day_bounds_tomorrow() -> (DateTime<Utc>, DateTime<Utc>) {
        let (s, _e) = Self::day_bounds_today();
        let start = s + Duration::days(1);
        (start, start + Duration::days(1))
    }

    fn upcoming_bounds(days: u32) -> (DateTime<Utc>, DateTime<Utc>) {
        let start = Utc::now();
        (start, start + Duration::days(days as i64))
    }

    async fn list_events(
        &self,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
        calendar_id: Option<&str>,
    ) -> Result<Vec<CalendarEvent>, CalendarError> {
        let token = self.token().await?;
        let calendar = calendar_id.unwrap_or("primary");
        let calendar_enc =
            url::form_urlencoded::byte_serialize(calendar.as_bytes()).collect::<String>();
        let path = format!(
            "/calendars/{calendar_enc}/events?singleEvents=true&orderBy=startTime\
             &timeMin={}&timeMax={}",
            time_min.to_rfc3339(),
            time_max.to_rfc3339()
        );
        let resp = self
            .transport
            .execute(GoogleCalendarRequest {
                method: "GET".to_string(),
                path,
                access_token: token,
                body: None,
            })
            .await?;
        if resp.status == 401 {
            return Err(CalendarError::Auth("expired or invalid token".to_string()));
        }
        if !(200..300).contains(&resp.status) {
            return Err(CalendarError::ProviderUnavailable(format!(
                "google calendar list returned {}",
                resp.status
            )));
        }
        let body = resp.body.unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            CalendarError::ProviderUnavailable(format!("invalid list payload: {e}"))
        })?;
        let items = v
            .get("items")
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            if let Some(ev) = parse_event(&item) {
                out.push(ev);
            }
        }
        Ok(out)
    }

    async fn create_event(&self, query: &CalendarQuery) -> Result<CalendarEvent, CalendarError> {
        let title = query
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| CalendarError::InvalidQuery("missing event title".to_string()))?;
        let start = query
            .start
            .ok_or_else(|| CalendarError::InvalidQuery("missing start time".to_string()))?;
        let end = query.end.unwrap_or(start + Duration::hours(1));
        if end <= start {
            return Err(CalendarError::InvalidQuery(
                "end must be after start".to_string(),
            ));
        }
        let calendar = query.calendar_name.as_deref().unwrap_or("primary");
        let calendar_enc =
            url::form_urlencoded::byte_serialize(calendar.as_bytes()).collect::<String>();
        let path = format!("/calendars/{calendar_enc}/events");
        let mut payload = serde_json::json!({
            "summary": title,
            "start": { "dateTime": start.to_rfc3339() },
            "end": { "dateTime": end.to_rfc3339() },
        });
        if let Some(loc) = query.location.as_deref().filter(|s| !s.is_empty()) {
            payload["location"] = serde_json::Value::String(loc.to_string());
        }
        let token = self.token().await?;
        let resp = self
            .transport
            .execute(GoogleCalendarRequest {
                method: "POST".to_string(),
                path,
                access_token: token,
                body: Some(payload.to_string()),
            })
            .await?;
        if resp.status == 401 {
            return Err(CalendarError::Auth("expired or invalid token".to_string()));
        }
        if !(200..300).contains(&resp.status) {
            return Err(CalendarError::ProviderUnavailable(format!(
                "create returned {}: {}",
                resp.status,
                resp.body.unwrap_or_default()
            )));
        }
        let body = resp.body.unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            CalendarError::ProviderUnavailable(format!("invalid create payload: {e}"))
        })?;
        parse_event(&v).ok_or_else(|| {
            CalendarError::ProviderUnavailable("create returned invalid event".to_string())
        })
    }
}

fn parse_event(v: &serde_json::Value) -> Option<CalendarEvent> {
    let id = v.get("id")?.as_str()?.to_string();
    let title = v
        .get("summary")
        .and_then(|s| s.as_str())
        .unwrap_or("(no title)")
        .to_string();
    let location = v
        .get("location")
        .and_then(|s| s.as_str())
        .map(str::to_string);
    let calendar_name = v
        .get("organizer")
        .and_then(|o| o.get("displayName"))
        .and_then(|s| s.as_str())
        .map(str::to_string);
    let start_obj = v.get("start")?;
    let end_obj = v.get("end")?;
    let (start, all_day) = parse_time(start_obj)?;
    let (end, _) = parse_time(end_obj)?;
    Some(CalendarEvent {
        id,
        title,
        start,
        end,
        all_day,
        location,
        calendar_name,
    })
}

fn parse_time(v: &serde_json::Value) -> Option<(DateTime<Utc>, bool)> {
    if let Some(dt) = v.get("dateTime").and_then(|s| s.as_str()) {
        let parsed = DateTime::parse_from_rfc3339(dt).ok()?;
        return Some((parsed.with_timezone(&Utc), false));
    }
    if let Some(d) = v.get("date").and_then(|s| s.as_str()) {
        let date = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
        let naive = date.and_hms_opt(0, 0, 0)?;
        return Some((Utc.from_utc_datetime(&naive), true));
    }
    None
}

#[async_trait]
impl CalendarSkill for GoogleCalendarSkill {
    async fn execute(&self, query: &CalendarQuery) -> Result<CalendarResult, CalendarError> {
        let started_at = Instant::now();
        let result = async {
            match &query.action {
                CalendarAction::ListToday => {
                    let (a, b) = Self::day_bounds_today();
                    let events = self
                        .list_events(a, b, query.calendar_name.as_deref())
                        .await?;
                    Ok(CalendarResult {
                        summary: format!("{} event(s) today", events.len()),
                        events,
                        created_event_id: None,
                    })
                }
                CalendarAction::ListTomorrow => {
                    let (a, b) = Self::day_bounds_tomorrow();
                    let events = self
                        .list_events(a, b, query.calendar_name.as_deref())
                        .await?;
                    Ok(CalendarResult {
                        summary: format!("{} event(s) tomorrow", events.len()),
                        events,
                        created_event_id: None,
                    })
                }
                CalendarAction::ListUpcoming { days } => {
                    let (a, b) = Self::upcoming_bounds(*days);
                    let events = self
                        .list_events(a, b, query.calendar_name.as_deref())
                        .await?;
                    Ok(CalendarResult {
                        summary: format!("{} event(s) in next {days} day(s)", events.len()),
                        events,
                        created_event_id: None,
                    })
                }
                CalendarAction::Create => {
                    let event = self.create_event(query).await?;
                    Ok(CalendarResult {
                        summary: format!(
                            "Created \"{}\" at {}",
                            event.title,
                            event.start.format("%Y-%m-%d %H:%M UTC")
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
            error_kind = Self::error_kind(e);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct StubTransport {
        calls: Mutex<Vec<GoogleCalendarRequest>>,
        responses: Mutex<Vec<GoogleCalendarResponse>>,
    }

    impl StubTransport {
        fn new(responses: Vec<GoogleCalendarResponse>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                responses: Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl GoogleCalendarTransport for StubTransport {
        async fn execute(
            &self,
            req: GoogleCalendarRequest,
        ) -> Result<GoogleCalendarResponse, CalendarError> {
            let mut responses = self.responses.lock().expect("responses mutex poisoned");
            let resp = responses
                .pop()
                .expect("no stubbed response remaining for transport call");
            self.calls.lock().expect("calls mutex poisoned").push(req);
            Ok(resp)
        }
    }

    fn auth() -> Arc<dyn GoogleCalendarAuth> {
        Arc::new(StaticGoogleCalendarAuth::new("test-token"))
    }

    fn list_query(action: CalendarAction) -> CalendarQuery {
        CalendarQuery {
            action,
            title: None,
            start: None,
            end: None,
            location: None,
            calendar_name: None,
        }
    }

    #[tokio::test]
    async fn list_today_parses_events() {
        let body = r#"{"items":[{
            "id":"evt1","summary":"Standup",
            "start":{"dateTime":"2026-04-18T09:00:00Z"},
            "end":{"dateTime":"2026-04-18T09:15:00Z"}
        }]}"#;
        let transport = Arc::new(StubTransport::new(vec![GoogleCalendarResponse {
            status: 200,
            body: Some(body.to_string()),
        }]));
        let skill = GoogleCalendarSkill::new(auth(), transport.clone());
        let res = skill
            .execute(&list_query(CalendarAction::ListToday))
            .await
            .expect("ok");
        assert_eq!(res.events.len(), 1);
        assert_eq!(res.events[0].title, "Standup");
        let calls = transport.calls.lock().expect("calls");
        assert_eq!(calls[0].method, "GET");
        assert!(calls[0].path.contains("/calendars/primary/events"));
    }

    #[tokio::test]
    async fn create_requires_title_and_start() {
        let transport = Arc::new(StubTransport::new(vec![]));
        let skill = GoogleCalendarSkill::new(auth(), transport);
        let q = CalendarQuery {
            action: CalendarAction::Create,
            title: None,
            start: None,
            end: None,
            location: None,
            calendar_name: None,
        };
        let err = skill.execute(&q).await.expect_err("err");
        assert!(matches!(err, CalendarError::InvalidQuery(_)));
    }

    #[tokio::test]
    async fn create_posts_event_payload() {
        let body = r#"{
            "id":"new1","summary":"Lunch",
            "start":{"dateTime":"2026-04-18T12:00:00Z"},
            "end":{"dateTime":"2026-04-18T13:00:00Z"}
        }"#;
        let transport = Arc::new(StubTransport::new(vec![GoogleCalendarResponse {
            status: 200,
            body: Some(body.to_string()),
        }]));
        let skill = GoogleCalendarSkill::new(auth(), transport.clone());
        let start = DateTime::parse_from_rfc3339("2026-04-18T12:00:00Z")
            .expect("rfc3339")
            .with_timezone(&Utc);
        let q = CalendarQuery {
            action: CalendarAction::Create,
            title: Some("Lunch".to_string()),
            start: Some(start),
            end: Some(start + Duration::hours(1)),
            location: Some("Cafeteria".to_string()),
            calendar_name: None,
        };
        let res = skill.execute(&q).await.expect("ok");
        assert_eq!(res.created_event_id.as_deref(), Some("new1"));
        let calls = transport.calls.lock().expect("calls");
        assert_eq!(calls[0].method, "POST");
        assert!(calls[0].body.as_deref().unwrap_or("").contains("Lunch"));
        assert!(calls[0].body.as_deref().unwrap_or("").contains("Cafeteria"));
    }

    #[tokio::test]
    async fn unauthorized_maps_to_auth_error() {
        let transport = Arc::new(StubTransport::new(vec![GoogleCalendarResponse {
            status: 401,
            body: Some("{}".to_string()),
        }]));
        let skill = GoogleCalendarSkill::new(auth(), transport);
        let err = skill
            .execute(&list_query(CalendarAction::ListToday))
            .await
            .expect_err("err");
        assert!(matches!(err, CalendarError::Auth(_)));
    }
}
