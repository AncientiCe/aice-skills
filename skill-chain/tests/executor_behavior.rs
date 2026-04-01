use async_trait::async_trait;
use skill_chain::{
    ChainExecutor, ChainPlan, ChainPlanner, ChainPlannerError, ChainStep, ConfirmationError,
    ConfirmationHandler, DonePayload, DoneStatus, EffectiveCapabilities, PlannerRequest,
    RunSkillStep, SkillRunError, SkillRunErrorKind, SkillRunRequest, SkillRunResult, SkillRunner,
    SpeakUpdateStep, StepTarget, TtsError, TtsNotifier,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

struct StubPlanner {
    plan: ChainPlan,
}

#[async_trait]
impl ChainPlanner for StubPlanner {
    async fn plan(&self, _request: &PlannerRequest) -> Result<ChainPlan, ChainPlannerError> {
        Ok(self.plan.clone())
    }
}

struct StubRunner {
    responses: Arc<Mutex<Vec<Result<SkillRunResult, SkillRunError>>>>,
    seen_targets: Arc<Mutex<Vec<StepTarget>>>,
}

impl StubRunner {
    fn new(responses: Vec<Result<SkillRunResult, SkillRunError>>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
            seen_targets: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl SkillRunner for StubRunner {
    async fn run(&self, request: &SkillRunRequest) -> Result<SkillRunResult, SkillRunError> {
        self.seen_targets.lock().expect("lock").push(request.target);
        self.responses.lock().expect("lock").remove(0)
    }
}

struct StubTts {
    fail: bool,
    spoken: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl TtsNotifier for StubTts {
    async fn speak(&self, message: &str) -> Result<(), TtsError> {
        self.spoken.lock().expect("lock").push(message.to_string());
        if self.fail {
            return Err(TtsError::Unavailable("no speaker".to_string()));
        }
        Ok(())
    }
}

struct StubConfirmation {
    response: bool,
}

#[async_trait]
impl ConfirmationHandler for StubConfirmation {
    async fn confirm(&self, _prompt: &str) -> Result<bool, ConfirmationError> {
        Ok(self.response)
    }
}

fn ok_result(summary: &str) -> Result<SkillRunResult, SkillRunError> {
    Ok(SkillRunResult {
        summary: summary.to_string(),
        output: BTreeMap::new(),
    })
}

#[tokio::test]
async fn executes_steps_and_speaks_updates() {
    let plan = ChainPlan {
        steps: vec![
            ChainStep::RunSkill(RunSkillStep {
                skill_id: "skill_time".to_string(),
                input: "x".to_string(),
                target: StepTarget::Backend,
                side_effect: false,
                retryable: true,
                on_error: vec![],
            }),
            ChainStep::RunSkill(RunSkillStep {
                skill_id: "skill_weather".to_string(),
                input: "x".to_string(),
                target: StepTarget::Backend,
                side_effect: false,
                retryable: true,
                on_error: vec![],
            }),
            ChainStep::Done(DonePayload {
                status: DoneStatus::Success,
                user_summary: "done".to_string(),
                structured_result: BTreeMap::new(),
            }),
        ],
    };
    let planner = StubPlanner { plan };
    let runner = StubRunner::new(vec![ok_result("time"), ok_result("weather")]);
    let seen_targets = runner.seen_targets.clone();
    let spoken = Arc::new(Mutex::new(Vec::new()));
    let tts = StubTts {
        fail: false,
        spoken: spoken.clone(),
    };
    let confirmation = StubConfirmation { response: true };
    let executor = ChainExecutor::new(planner, runner, tts, confirmation);

    let capabilities = EffectiveCapabilities::from_handshake(
        vec![],
        vec!["skill_time".to_string(), "skill_weather".to_string()],
    );
    let result = executor
        .execute_with_capabilities("morning", BTreeMap::new(), capabilities)
        .await
        .expect("ok");

    assert_eq!(result.done.status, DoneStatus::Success);
    assert_eq!(spoken.lock().expect("lock").len(), 2);
    assert_eq!(
        seen_targets.lock().expect("lock").as_slice(),
        [StepTarget::Backend, StepTarget::Backend]
    );
}

#[tokio::test]
async fn forwards_frontend_target_to_runner() {
    let plan = ChainPlan {
        steps: vec![
            ChainStep::RunSkill(RunSkillStep {
                skill_id: "skill_shopping_list".to_string(),
                input: "add:strawberries".to_string(),
                target: StepTarget::Frontend,
                side_effect: false,
                retryable: true,
                on_error: vec![],
            }),
            ChainStep::Done(DonePayload {
                status: DoneStatus::Success,
                user_summary: "done".to_string(),
                structured_result: BTreeMap::new(),
            }),
        ],
    };
    let planner = StubPlanner { plan };
    let runner = StubRunner::new(vec![ok_result("added")]);
    let seen_targets = runner.seen_targets.clone();
    let tts = StubTts {
        fail: false,
        spoken: Arc::new(Mutex::new(Vec::new())),
    };
    let confirmation = StubConfirmation { response: true };
    let executor = ChainExecutor::new(planner, runner, tts, confirmation);

    let capabilities = EffectiveCapabilities::from_handshake(
        vec!["skill_shopping_list".to_string()],
        vec!["skill_weather".to_string()],
    );
    let result = executor
        .execute_with_capabilities("shopping", BTreeMap::new(), capabilities)
        .await
        .expect("ok");
    assert_eq!(result.done.status, DoneStatus::Success);
    assert_eq!(
        seen_targets.lock().expect("lock").as_slice(),
        [StepTarget::Frontend]
    );
}

#[tokio::test]
async fn loop_guard_triggers_without_done() {
    let plan = ChainPlan {
        steps: vec![ChainStep::SpeakUpdate(SpeakUpdateStep {
            message: "still working".to_string(),
        })],
    };
    let planner = StubPlanner { plan };
    let runner = StubRunner::new(vec![]);
    let tts = StubTts {
        fail: false,
        spoken: Arc::new(Mutex::new(Vec::new())),
    };
    let confirmation = StubConfirmation { response: true };
    let executor = ChainExecutor::new(planner, runner, tts, confirmation).with_max_steps(1);

    let capabilities =
        EffectiveCapabilities::from_handshake(vec![], vec!["skill_time".to_string()]);
    let result = executor
        .execute_with_capabilities("test", BTreeMap::new(), capabilities)
        .await
        .expect("ok");
    assert_eq!(result.done.user_summary, "error_loop_guard");
}

#[tokio::test]
async fn rejects_when_step_target_skill_not_in_capabilities() {
    let plan = ChainPlan {
        steps: vec![
            ChainStep::RunSkill(RunSkillStep {
                skill_id: "skill_shopping_list".to_string(),
                input: "x".to_string(),
                target: StepTarget::Frontend,
                side_effect: false,
                retryable: true,
                on_error: vec![],
            }),
            ChainStep::Done(DonePayload {
                status: DoneStatus::Success,
                user_summary: "done".to_string(),
                structured_result: BTreeMap::new(),
            }),
        ],
    };
    let planner = StubPlanner { plan };
    let runner = StubRunner::new(vec![ok_result("ok")]);
    let tts = StubTts {
        fail: false,
        spoken: Arc::new(Mutex::new(Vec::new())),
    };
    let confirmation = StubConfirmation { response: true };
    let executor = ChainExecutor::new(planner, runner, tts, confirmation);

    let capabilities =
        EffectiveCapabilities::from_handshake(vec![], vec!["skill_weather".to_string()]);
    let result = executor
        .execute_with_capabilities("test", BTreeMap::new(), capabilities)
        .await;
    assert!(result.is_err());
}
