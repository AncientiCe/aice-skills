//! Behavioral tests for `skill-hotel`.
//!
//! These run end-to-end through the public API to keep coverage honest
//! per AGENTS rule 9: integration-style behavior, not implementation
//! introspection.

use async_trait::async_trait;
use serde_json::{json, Value};
use skill_hotel::{
    HotelError, HotelIntentKind, HotelMcpTransport, HotelQuery, HotelSkill, McpHotelSkill,
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingTransport {
    calls: Mutex<Vec<(String, Value)>>,
}

impl RecordingTransport {
    fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn last(&self) -> (String, Value) {
        self.calls
            .lock()
            .expect("lock")
            .last()
            .cloned()
            .expect("at least one call")
    }
}

#[async_trait]
impl HotelMcpTransport for RecordingTransport {
    async fn call_tool(&self, tool: &str, args: Value) -> Result<Value, HotelError> {
        self.calls
            .lock()
            .expect("lock")
            .push((tool.to_string(), args));
        Ok(json!({
            "content": [{ "type": "text", "text": format!("{} done", tool) }],
            "status": "confirmed",
        }))
    }
}

#[tokio::test]
async fn forwards_user_text_into_tool_args() {
    let transport = RecordingTransport::shared();
    let skill = McpHotelSkill::new(transport.clone());
    let query = HotelQuery::new(HotelIntentKind::SetCurtains)
        .with_slots(json!({ "action": "open" }))
        .with_user_text("please open the curtains");
    let result = skill.execute(&query).await.expect("ok");
    assert_eq!(result.kind, HotelIntentKind::SetCurtains);
    assert!(result.summary.contains("set_curtains done"));
    let (tool, args) = transport.last();
    assert_eq!(tool, "set_curtains");
    assert_eq!(args.get("action"), Some(&json!("open")));
    assert_eq!(
        args.get("_user_text"),
        Some(&json!("please open the curtains"))
    );
}

#[tokio::test]
async fn null_slots_are_treated_as_empty_object() {
    let transport = RecordingTransport::shared();
    let skill = McpHotelSkill::new(transport.clone());
    let query = HotelQuery::new(HotelIntentKind::RequestBillingSummary).with_slots(Value::Null);
    let result = skill.execute(&query).await.expect("ok");
    assert_eq!(result.kind, HotelIntentKind::RequestBillingSummary);
    let (_, args) = transport.last();
    assert!(args.is_object());
}

#[tokio::test]
async fn prompt_context_is_compact_and_deterministic() {
    let transport = RecordingTransport::shared();
    let skill = McpHotelSkill::new(transport);
    let result = skill
        .execute(
            &HotelQuery::new(HotelIntentKind::RequestExtraTowels).with_slots(json!({ "count": 2 })),
        )
        .await
        .expect("ok");
    let context = result.to_prompt_context();
    assert!(context.contains("intent=request_extra_towels"));
    assert!(context.contains("status=confirmed"));
    assert!(context.contains("request_extra_towels done"));
}
