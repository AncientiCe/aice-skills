//! Hotel concierge skill.
//!
//! `skill-hotel` is a *frontend* skill: the backend classifier picks
//! `skill_hotel` plus a `hotel_intent_kind`, the frontend wraps that into
//! a [`HotelQuery`] and dispatches it through a [`HotelMcpTransport`] to
//! the property's MCP server. The structured [`HotelResult`] is then sent
//! back to the backend so its LLM can compose a spoken answer.
//!
//! Two real implementations of [`HotelSkill`] ship here:
//!
//! - [`McpHotelSkill`] — production. Calls a real Hotel MCP through any
//!   `HotelMcpTransport` implementation.
//! - [`LocalHotelSkill`] — offline demo. Returns deterministic canned
//!   responses without touching the network. Useful for tests and local
//!   development when no MCP is available.
//!
//! [`MockHotelSkill`] is also exposed for orchestrator-side unit tests.

mod types;

pub use types::{
    HotelError, HotelIntentKind, HotelMcpTransport, HotelQuery, HotelResult, HotelSkill,
};

use async_trait::async_trait;
use metrics::{counter, histogram};
use serde_json::{json, Map, Value};
use std::sync::Arc;
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "hotel";

/// Validate per-intent required slots. Forwarded slots beyond the
/// required ones are passed through verbatim — the MCP server is the
/// authority on the full schema.
pub fn validate_slots(kind: HotelIntentKind, slots: &Value) -> Result<(), HotelError> {
    let object: Option<&Map<String, Value>> = match slots {
        Value::Object(map) => Some(map),
        Value::Null => None,
        other => {
            return Err(HotelError::InvalidSlots {
                intent: kind.as_str().to_string(),
                reason: format!("slots must be a JSON object, got {}", value_type(other)),
            });
        }
    };

    for required in required_fields(kind) {
        let present = object
            .and_then(|map| map.get(*required))
            .map(|v| !v.is_null())
            .unwrap_or(false);
        if !present {
            return Err(HotelError::InvalidSlots {
                intent: kind.as_str().to_string(),
                reason: format!("missing required field '{required}'"),
            });
        }
    }

    if matches!(
        kind,
        HotelIntentKind::OrderRoomService | HotelIntentKind::RequestToiletries
    ) {
        let valid = object
            .and_then(|map| map.get("items"))
            .and_then(Value::as_array)
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        if !valid {
            return Err(HotelError::InvalidSlots {
                intent: kind.as_str().to_string(),
                reason: "field 'items' must be a non-empty array".to_string(),
            });
        }
    }

    Ok(())
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn required_fields(kind: HotelIntentKind) -> &'static [&'static str] {
    match kind {
        HotelIntentKind::SetWakeUpCall => &["time"],
        HotelIntentKind::BookRestaurant => &["time"],
        HotelIntentKind::BookSpa => &["service", "time"],
        HotelIntentKind::BookTaxi => &["destination"],
        HotelIntentKind::ConciergeInfo => &["topic"],
        HotelIntentKind::RequestLocalRecommendation => &["topic"],
        HotelIntentKind::ReportComplaint => &["category"],
        HotelIntentKind::ReportLostItem => &["item"],
        HotelIntentKind::LanguageHelp => &["phrase", "target_language"],
        _ => &[],
    }
}

/// Production hotel skill: dispatches each intent to the configured MCP
/// transport.
pub struct McpHotelSkill {
    transport: Arc<dyn HotelMcpTransport>,
}

impl McpHotelSkill {
    pub fn new(transport: Arc<dyn HotelMcpTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl HotelSkill for McpHotelSkill {
    async fn execute(&self, query: &HotelQuery) -> Result<HotelResult, HotelError> {
        let started_at = Instant::now();
        let result = run_mcp(self.transport.as_ref(), query).await;
        record_metrics(query.kind, &result, started_at);
        result
    }
}

async fn run_mcp(
    transport: &dyn HotelMcpTransport,
    query: &HotelQuery,
) -> Result<HotelResult, HotelError> {
    validate_slots(query.kind, &query.slots)?;
    let args = build_tool_args(query);
    let raw = transport.call_tool(query.kind.as_str(), args).await?;
    parse_tool_response(query.kind, raw)
}

fn build_tool_args(query: &HotelQuery) -> Value {
    let mut object = match &query.slots {
        Value::Object(map) => map.clone(),
        Value::Null => Map::new(),
        other => {
            let mut wrapper = Map::new();
            wrapper.insert("value".to_string(), other.clone());
            wrapper
        }
    };
    if !query.user_text.is_empty() {
        object
            .entry("_user_text".to_string())
            .or_insert_with(|| Value::String(query.user_text.clone()));
    }
    Value::Object(object)
}

fn parse_tool_response(kind: HotelIntentKind, value: Value) -> Result<HotelResult, HotelError> {
    if let Some(Value::Bool(true)) = value.get("isError") {
        let reason = extract_text(&value).unwrap_or_else(|| "tool reported an error".to_string());
        return Err(HotelError::McpRejected(reason));
    }

    let summary = extract_text(&value).unwrap_or_else(|| default_summary(kind).to_string());
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "ok".to_string());
    Ok(HotelResult {
        kind,
        status,
        summary,
        details: value,
    })
}

fn extract_text(value: &Value) -> Option<String> {
    let array = value.get("content")?.as_array()?;
    for entry in array {
        if let Some(text) = entry.get("text").and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn default_summary(kind: HotelIntentKind) -> &'static str {
    match kind {
        HotelIntentKind::SetRoomTemperature => "Room temperature updated.",
        HotelIntentKind::SetLights => "Lights updated.",
        HotelIntentKind::SetCurtains => "Curtains updated.",
        HotelIntentKind::SetTv => "TV updated.",
        HotelIntentKind::SetAmbientMusic => "Ambient music updated.",
        HotelIntentKind::SetDoNotDisturb => "Do-not-disturb updated.",
        HotelIntentKind::OrderRoomService => "Room service order placed.",
        HotelIntentKind::RequestHousekeeping => "Housekeeping request queued.",
        HotelIntentKind::RequestExtraTowels => "Extra towels requested.",
        HotelIntentKind::RequestExtraPillows => "Extra pillows requested.",
        HotelIntentKind::RequestToiletries => "Toiletries requested.",
        HotelIntentKind::RequestLaundryPickup => "Laundry pickup scheduled.",
        HotelIntentKind::RequestIron => "Iron request queued.",
        HotelIntentKind::SetWakeUpCall => "Wake-up call scheduled.",
        HotelIntentKind::RequestLateCheckout => "Late checkout requested.",
        HotelIntentKind::BookRestaurant => "Restaurant booking submitted.",
        HotelIntentKind::BookSpa => "Spa booking submitted.",
        HotelIntentKind::BookTaxi => "Taxi booking submitted.",
        HotelIntentKind::ConciergeInfo => "Concierge information.",
        HotelIntentKind::RequestLocalRecommendation => "Local recommendations.",
        HotelIntentKind::ReportComplaint => "Complaint logged.",
        HotelIntentKind::ReportLostItem => "Lost item report logged.",
        HotelIntentKind::RequestBillingSummary => "Billing summary.",
        HotelIntentKind::RequestCheckout => "Checkout requested.",
        HotelIntentKind::LanguageHelp => "Language help.",
    }
}

fn record_metrics(
    kind: HotelIntentKind,
    result: &Result<HotelResult, HotelError>,
    started_at: Instant,
) {
    let (label, error_kind) = match result {
        Ok(_) => ("success", "none"),
        Err(error) => ("error", error.kind()),
    };
    counter!(
        SKILL_EXECUTE_TOTAL,
        1,
        "skill" => SKILL_LABEL,
        "intent" => kind.as_str(),
        "result" => label,
        "error_kind" => error_kind
    );
    histogram!(
        SKILL_EXECUTE_DURATION_SECONDS,
        started_at.elapsed().as_secs_f64(),
        "skill" => SKILL_LABEL,
        "intent" => kind.as_str()
    );
}

/// Offline demo skill that returns deterministic canned responses without
/// touching the network. Slot validation is still applied, so callers
/// behave the same as in production for invalid payloads.
pub struct LocalHotelSkill;

impl LocalHotelSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalHotelSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HotelSkill for LocalHotelSkill {
    async fn execute(&self, query: &HotelQuery) -> Result<HotelResult, HotelError> {
        let started_at = Instant::now();
        let result = run_local(query);
        record_metrics(query.kind, &result, started_at);
        result
    }
}

fn run_local(query: &HotelQuery) -> Result<HotelResult, HotelError> {
    validate_slots(query.kind, &query.slots)?;
    Ok(HotelResult {
        kind: query.kind,
        status: "ok".to_string(),
        summary: default_summary(query.kind).to_string(),
        details: json!({
            "mode": "local",
            "intent": query.kind.as_str(),
            "echoed_slots": query.slots,
        }),
    })
}

/// Stub implementation for orchestrator unit tests.
pub struct MockHotelSkill {
    pub result: Result<HotelResult, HotelError>,
}

impl MockHotelSkill {
    pub fn ok(result: HotelResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: HotelError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl HotelSkill for MockHotelSkill {
    async fn execute(&self, _query: &HotelQuery) -> Result<HotelResult, HotelError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct StubTransport {
        calls: Mutex<Vec<(String, Value)>>,
        response: Mutex<Option<Result<Value, HotelError>>>,
    }

    impl StubTransport {
        fn ok(response: Value) -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                response: Mutex::new(Some(Ok(response))),
            })
        }

        fn err(error: HotelError) -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                response: Mutex::new(Some(Err(error))),
            })
        }

        fn last_call(&self) -> Option<(String, Value)> {
            self.calls.lock().expect("lock").last().cloned()
        }
    }

    #[async_trait]
    impl HotelMcpTransport for StubTransport {
        async fn call_tool(&self, tool: &str, args: Value) -> Result<Value, HotelError> {
            self.calls
                .lock()
                .expect("lock")
                .push((tool.to_string(), args));
            self.response
                .lock()
                .expect("lock")
                .clone()
                .unwrap_or_else(|| Err(HotelError::Other("no response configured".into())))
        }
    }

    fn ok_response(text: &str) -> Value {
        json!({
            "content": [{ "type": "text", "text": text }],
            "status": "confirmed",
        })
    }

    #[test]
    fn intent_kind_strings_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for kind in HotelIntentKind::ALL {
            assert!(seen.insert(kind.as_str()), "duplicate: {}", kind.as_str());
        }
        assert_eq!(seen.len(), 25);
    }

    #[test]
    fn intent_kind_round_trip() {
        for kind in HotelIntentKind::ALL {
            assert_eq!(HotelIntentKind::parse(kind.as_str()), Some(*kind));
        }
        assert_eq!(HotelIntentKind::parse("not_a_thing"), None);
    }

    #[tokio::test]
    async fn mcp_skill_dispatches_each_intent_to_the_matching_tool() {
        for kind in HotelIntentKind::ALL.iter().copied() {
            let transport = StubTransport::ok(ok_response("done"));
            let skill = McpHotelSkill::new(transport.clone());
            let query = HotelQuery::new(kind)
                .with_slots(canonical_slots(kind))
                .with_user_text("user said something");
            let result = skill
                .execute(&query)
                .await
                .unwrap_or_else(|err| panic!("intent {} should succeed: {err}", kind.as_str()));
            assert_eq!(result.kind, kind);
            assert_eq!(result.status, "confirmed");
            let (tool, args) = transport.last_call().expect("transport call recorded");
            assert_eq!(tool, kind.as_str());
            assert_eq!(args.get("_user_text"), Some(&json!("user said something")));
        }
    }

    fn canonical_slots(kind: HotelIntentKind) -> Value {
        match kind {
            HotelIntentKind::SetRoomTemperature => json!({ "celsius": 22 }),
            HotelIntentKind::SetLights => json!({ "action": "scene", "scene": "relax" }),
            HotelIntentKind::SetCurtains => json!({ "action": "open" }),
            HotelIntentKind::SetTv => json!({ "action": "power_on" }),
            HotelIntentKind::SetAmbientMusic => json!({ "action": "play", "mood": "jazz" }),
            HotelIntentKind::SetDoNotDisturb => json!({ "enabled": true }),
            HotelIntentKind::OrderRoomService => json!({
                "items": [{ "name": "club sandwich", "quantity": 1 }]
            }),
            HotelIntentKind::RequestHousekeeping => json!({ "kind": "turn_down" }),
            HotelIntentKind::RequestExtraTowels => json!({ "count": 2 }),
            HotelIntentKind::RequestExtraPillows => json!({ "count": 2 }),
            HotelIntentKind::RequestToiletries => json!({ "items": ["toothbrush"] }),
            HotelIntentKind::RequestLaundryPickup => json!({ "when": "tomorrow 9am" }),
            HotelIntentKind::RequestIron => json!({ "kind": "both" }),
            HotelIntentKind::SetWakeUpCall => json!({ "time": "07:00" }),
            HotelIntentKind::RequestLateCheckout => json!({ "until": "13:00" }),
            HotelIntentKind::BookRestaurant => json!({ "outlet": "main", "time": "20:00" }),
            HotelIntentKind::BookSpa => json!({ "service": "massage", "time": "16:00" }),
            HotelIntentKind::BookTaxi => json!({ "destination": "airport", "time": "16:00" }),
            HotelIntentKind::ConciergeInfo => json!({ "topic": "pool hours" }),
            HotelIntentKind::RequestLocalRecommendation => {
                json!({ "topic": "italian restaurants" })
            }
            HotelIntentKind::ReportComplaint => json!({ "category": "noise" }),
            HotelIntentKind::ReportLostItem => json!({ "item": "wallet" }),
            HotelIntentKind::RequestBillingSummary => json!({}),
            HotelIntentKind::RequestCheckout => json!({}),
            HotelIntentKind::LanguageHelp => json!({
                "phrase": "where is the train station",
                "target_language": "italian",
            }),
        }
    }

    #[tokio::test]
    async fn missing_required_slot_yields_invalid_slots_error() {
        let cases = [
            (HotelIntentKind::SetWakeUpCall, json!({})),
            (HotelIntentKind::BookSpa, json!({ "service": "massage" })),
            (HotelIntentKind::LanguageHelp, json!({ "phrase": "hello" })),
            (HotelIntentKind::OrderRoomService, json!({ "items": [] })),
        ];
        for (kind, slots) in cases {
            let transport = StubTransport::ok(ok_response("ignored"));
            let skill = McpHotelSkill::new(transport);
            let err = skill
                .execute(&HotelQuery::new(kind).with_slots(slots))
                .await
                .expect_err("should reject missing required slot");
            assert!(matches!(err, HotelError::InvalidSlots { .. }));
        }
    }

    #[tokio::test]
    async fn non_object_slots_are_rejected() {
        let transport = StubTransport::ok(ok_response("ignored"));
        let skill = McpHotelSkill::new(transport);
        let err = skill
            .execute(
                &HotelQuery::new(HotelIntentKind::SetCurtains).with_slots(json!("just a string")),
            )
            .await
            .expect_err("should reject non-object");
        assert!(matches!(err, HotelError::InvalidSlots { .. }));
    }

    #[tokio::test]
    async fn transport_errors_propagate() {
        let cases = [
            HotelError::McpUnavailable("boom".into()),
            HotelError::Timeout,
            HotelError::McpRejected("nope".into()),
        ];
        for original in cases {
            let transport = StubTransport::err(original.clone());
            let skill = McpHotelSkill::new(transport);
            let err = skill
                .execute(
                    &HotelQuery::new(HotelIntentKind::SetCurtains)
                        .with_slots(json!({ "action": "open" })),
                )
                .await
                .expect_err("should propagate transport error");
            assert_eq!(err, original);
        }
    }

    #[tokio::test]
    async fn mcp_is_error_response_maps_to_rejected() {
        let response = json!({
            "isError": true,
            "content": [{ "type": "text", "text": "room not found" }],
        });
        let transport = StubTransport::ok(response);
        let skill = McpHotelSkill::new(transport);
        let err = skill
            .execute(
                &HotelQuery::new(HotelIntentKind::SetCurtains)
                    .with_slots(json!({ "action": "open" })),
            )
            .await
            .expect_err("should map isError=true");
        assert!(
            matches!(err, HotelError::McpRejected(reason) if reason.contains("room not found"))
        );
    }

    #[tokio::test]
    async fn local_skill_returns_canned_responses_for_every_intent() {
        let skill = LocalHotelSkill::new();
        for kind in HotelIntentKind::ALL.iter().copied() {
            let result = skill
                .execute(&HotelQuery::new(kind).with_slots(canonical_slots(kind)))
                .await
                .unwrap_or_else(|err| {
                    panic!("local intent {} should succeed: {err}", kind.as_str())
                });
            assert_eq!(result.kind, kind);
            assert_eq!(result.status, "ok");
            assert!(!result.summary.is_empty());
        }
    }

    #[tokio::test]
    async fn mock_skill_returns_configured_value() {
        let canned = HotelResult {
            kind: HotelIntentKind::ConciergeInfo,
            status: "ok".into(),
            summary: "stub".into(),
            details: json!({}),
        };
        let mock = MockHotelSkill::ok(canned.clone());
        let actual = mock
            .execute(&HotelQuery::new(HotelIntentKind::ConciergeInfo))
            .await
            .expect("ok");
        assert_eq!(actual, canned);
    }

    #[test]
    fn error_kind_labels_are_stable() {
        let cases = [
            (
                HotelError::InvalidSlots {
                    intent: "x".into(),
                    reason: "y".into(),
                },
                "invalid_slots",
            ),
            (HotelError::McpUnavailable("x".into()), "mcp_unavailable"),
            (HotelError::McpRejected("x".into()), "mcp_rejected"),
            (HotelError::Timeout, "timeout"),
            (
                HotelError::UnsupportedIntent("x".into()),
                "unsupported_intent",
            ),
            (HotelError::Other("x".into()), "other"),
        ];
        for (err, expected) in cases {
            assert_eq!(err.kind(), expected);
        }
    }
}
