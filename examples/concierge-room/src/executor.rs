//! Frontend skill executor: turns a `FrontendSkillIntent` from the
//! backend into a Hotel MCP call and produces a
//! `FrontendSkillResultRequest`.

use crate::protocol::{FrontendSkillIntent, FrontendSkillResultRequest};
use crate::stages::{MonoClock, VizEvent};
use crate::viz::VizHub;
use async_trait::async_trait;
use serde_json::Value;
use skill_hotel::{
    HotelError, HotelIntentKind, HotelQuery, HotelResult, HotelSkill, McpHotelSkill,
};
use std::sync::Arc;
use std::time::Instant;

#[async_trait]
pub trait FrontendSkillExecutor: Send + Sync {
    async fn execute(&self, intent: &FrontendSkillIntent) -> FrontendSkillResultRequest;
}

pub struct HotelExecutor {
    skill: Arc<McpHotelSkill>,
    hub: VizHub,
    clock: MonoClock,
}

impl HotelExecutor {
    pub fn new(skill: Arc<McpHotelSkill>, hub: VizHub, clock: MonoClock) -> Self {
        Self { skill, hub, clock }
    }
}

#[async_trait]
impl FrontendSkillExecutor for HotelExecutor {
    async fn execute(&self, intent: &FrontendSkillIntent) -> FrontendSkillResultRequest {
        if intent.intent != "skill_hotel" {
            return FrontendSkillResultRequest {
                status: "error".to_string(),
                user_text: format!("I can't run intent '{}' from this device.", intent.intent),
                structured_result_context: None,
                error: Some(format!("unsupported_intent:{}", intent.intent)),
            };
        }

        let kind_str = intent
            .slots
            .get("hotel_intent_kind")
            .and_then(Value::as_str);
        let Some(kind) = kind_str.and_then(HotelIntentKind::parse) else {
            let message = format!(
                "Missing or unknown hotel_intent_kind: {}",
                kind_str.unwrap_or("<absent>")
            );
            return FrontendSkillResultRequest {
                status: "error".to_string(),
                user_text: message.clone(),
                structured_result_context: None,
                error: Some("unknown_hotel_intent".to_string()),
            };
        };

        let slots = intent
            .slots
            .get("hotel_slots")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));
        let query = HotelQuery::new(kind)
            .with_slots(slots.clone())
            .with_user_text(intent.user_text.clone());

        let started = Instant::now();
        self.hub.emit(VizEvent::McpCallStarted {
            turn_id: intent.turn_id.clone(),
            t_ms: self.clock.elapsed_ms(),
            tool: kind.as_str().to_string(),
            args: slots,
        });

        match self.skill.execute(&query).await {
            Ok(result) => {
                self.hub.emit(VizEvent::McpCallDone {
                    turn_id: intent.turn_id.clone(),
                    t_ms: self.clock.elapsed_ms(),
                    tool: kind.as_str().to_string(),
                    latency_ms: started.elapsed().as_millis() as u64,
                    response: result.details.clone(),
                });
                build_success(result)
            }
            Err(error) => {
                self.hub.emit(VizEvent::McpCallError {
                    turn_id: intent.turn_id.clone(),
                    t_ms: self.clock.elapsed_ms(),
                    tool: kind.as_str().to_string(),
                    error: error.to_string(),
                    latency_ms: started.elapsed().as_millis() as u64,
                });
                build_error(kind, error)
            }
        }
    }
}

fn build_success(result: HotelResult) -> FrontendSkillResultRequest {
    let context = result.to_prompt_context();
    FrontendSkillResultRequest {
        status: result.status.clone(),
        user_text: result.summary.clone(),
        structured_result_context: Some(context),
        error: None,
    }
}

fn build_error(kind: HotelIntentKind, error: HotelError) -> FrontendSkillResultRequest {
    FrontendSkillResultRequest {
        status: "error".to_string(),
        user_text: format!(
            "I couldn't complete the {} request right now.",
            kind.as_str().replace('_', " ")
        ),
        structured_result_context: Some(format!(
            "Hotel concierge intent={} status=error: {}",
            kind.as_str(),
            error
        )),
        error: Some(error.kind().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use skill_hotel::HotelMcpTransport;
    use std::sync::Mutex;

    #[derive(Default)]
    struct StubTransport {
        calls: Mutex<Vec<(String, serde_json::Value)>>,
    }

    #[async_trait]
    impl HotelMcpTransport for StubTransport {
        async fn call_tool(
            &self,
            tool: &str,
            args: serde_json::Value,
        ) -> Result<serde_json::Value, HotelError> {
            self.calls
                .lock()
                .expect("lock")
                .push((tool.to_string(), args));
            Ok(json!({
                "content": [{ "type": "text", "text": format!("{} confirmed", tool) }],
                "status": "confirmed",
            }))
        }
    }

    #[tokio::test]
    async fn dispatches_each_canonical_intent() {
        let transport = Arc::new(StubTransport::default());
        let skill = Arc::new(McpHotelSkill::new(transport.clone()));
        let executor = HotelExecutor::new(skill, VizHub::new(), MonoClock::new());

        for kind in HotelIntentKind::ALL.iter().copied() {
            let slots = match kind {
                HotelIntentKind::SetWakeUpCall => json!({ "time": "07:00" }),
                HotelIntentKind::BookRestaurant => json!({ "time": "20:00" }),
                HotelIntentKind::BookSpa => {
                    json!({ "service": "massage", "time": "16:00" })
                }
                HotelIntentKind::BookTaxi => json!({ "destination": "airport" }),
                HotelIntentKind::ConciergeInfo => json!({ "topic": "pool" }),
                HotelIntentKind::RequestLocalRecommendation => {
                    json!({ "topic": "italian" })
                }
                HotelIntentKind::ReportComplaint => json!({ "category": "noise" }),
                HotelIntentKind::ReportLostItem => json!({ "item": "wallet" }),
                HotelIntentKind::LanguageHelp => {
                    json!({ "phrase": "hello", "target_language": "italian" })
                }
                HotelIntentKind::OrderRoomService => {
                    json!({ "items": [{ "name": "tea" }] })
                }
                HotelIntentKind::RequestToiletries => {
                    json!({ "items": ["soap"] })
                }
                _ => json!({}),
            };
            let intent = FrontendSkillIntent {
                turn_id: "T".to_string(),
                intent: "skill_hotel".to_string(),
                slots: json!({
                    "hotel_intent_kind": kind.as_str(),
                    "hotel_slots": slots,
                }),
                user_text: "demo".to_string(),
            };
            let result = executor.execute(&intent).await;
            assert!(!result.user_text.is_empty(), "intent {}", kind.as_str());
            assert_eq!(result.status, "confirmed", "intent {}", kind.as_str());
        }
        let calls = transport.calls.lock().expect("lock");
        assert_eq!(calls.len(), HotelIntentKind::ALL.len());
    }

    #[tokio::test]
    async fn unknown_intent_kind_yields_error() {
        let transport = Arc::new(StubTransport::default());
        let skill = Arc::new(McpHotelSkill::new(transport));
        let executor = HotelExecutor::new(skill, VizHub::new(), MonoClock::new());
        let intent = FrontendSkillIntent {
            turn_id: "T".to_string(),
            intent: "skill_hotel".to_string(),
            slots: json!({ "hotel_intent_kind": "fly_to_moon" }),
            user_text: String::new(),
        };
        let result = executor.execute(&intent).await;
        assert_eq!(result.status, "error");
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn non_hotel_intent_is_rejected_locally() {
        let transport = Arc::new(StubTransport::default());
        let skill = Arc::new(McpHotelSkill::new(transport));
        let executor = HotelExecutor::new(skill, VizHub::new(), MonoClock::new());
        let intent = FrontendSkillIntent {
            turn_id: "T".to_string(),
            intent: "skill_weather".to_string(),
            slots: json!({}),
            user_text: String::new(),
        };
        let result = executor.execute(&intent).await;
        assert_eq!(result.status, "error");
        assert!(result.user_text.contains("weather"));
    }
}
