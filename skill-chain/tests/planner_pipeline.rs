use async_trait::async_trait;
use skill_chain::{
    CanonicalIntent, ChainCompiler, ChainPlanner, ChainPlannerError, EffectiveCapabilities,
    LlmPipelinePlanner, PlannerRequest, SemanticNormalizer,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

struct StubNormalizer {
    seen: Arc<Mutex<Vec<String>>>,
    intent: Result<CanonicalIntent, ChainPlannerError>,
}

#[async_trait]
impl SemanticNormalizer for StubNormalizer {
    async fn normalize(&self, phrase: &str) -> Result<CanonicalIntent, ChainPlannerError> {
        self.seen.lock().expect("seen").push(phrase.to_string());
        self.intent.clone()
    }
}

struct StubCompiler {
    intent_log: Arc<Mutex<Vec<String>>>,
    result: Result<skill_chain::ChainPlan, ChainPlannerError>,
}

#[async_trait]
impl ChainCompiler for StubCompiler {
    async fn compile(
        &self,
        intent: &CanonicalIntent,
        _request: &PlannerRequest,
    ) -> Result<skill_chain::ChainPlan, ChainPlannerError> {
        self.intent_log
            .lock()
            .expect("intent_log")
            .push(intent.name.clone());
        self.result.clone()
    }
}

#[tokio::test]
async fn planner_uses_normalizer_then_compiler() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let intent_log = Arc::new(Mutex::new(Vec::new()));
    let planner = LlmPipelinePlanner::new(
        StubNormalizer {
            seen: seen.clone(),
            intent: Ok(CanonicalIntent {
                name: "shopping_list.add".to_string(),
                slots: BTreeMap::from([("item".to_string(), "strawberries".to_string())]),
            }),
        },
        StubCompiler {
            intent_log: intent_log.clone(),
            result: Ok(skill_chain::ChainPlan {
                steps: vec![skill_chain::ChainStep::Done(skill_chain::DonePayload {
                    status: skill_chain::DoneStatus::Success,
                    user_summary: "ok".to_string(),
                    structured_result: BTreeMap::new(),
                })],
            }),
        },
    );

    let request = PlannerRequest {
        user_phrase: "i'd eat some strawberries".to_string(),
        context: BTreeMap::new(),
        capabilities: EffectiveCapabilities::from_handshake(
            vec!["skill_shopping_list".to_string()],
            vec!["skill_weather".to_string()],
        ),
    };
    let result = planner.plan(&request).await.expect("plan");

    assert_eq!(result.steps.len(), 1);
    assert_eq!(seen.lock().expect("seen").len(), 1);
    assert_eq!(
        intent_log.lock().expect("intent_log").as_slice(),
        ["shopping_list.add"]
    );
}

#[tokio::test]
async fn planner_returns_normalizer_error_without_manual_fallback() {
    let planner = LlmPipelinePlanner::new(
        StubNormalizer {
            seen: Arc::new(Mutex::new(Vec::new())),
            intent: Err(ChainPlannerError::InvalidIntent("no_intent".to_string())),
        },
        StubCompiler {
            intent_log: Arc::new(Mutex::new(Vec::new())),
            result: Err(ChainPlannerError::InvalidIntent(
                "should_not_run".to_string(),
            )),
        },
    );
    let request = PlannerRequest {
        user_phrase: "mystery phrase".to_string(),
        context: BTreeMap::new(),
        capabilities: EffectiveCapabilities::from_handshake(vec![], vec![]),
    };
    let result = planner.plan(&request).await;
    assert!(matches!(
        result,
        Err(ChainPlannerError::InvalidIntent(msg)) if msg == "no_intent"
    ));
}
