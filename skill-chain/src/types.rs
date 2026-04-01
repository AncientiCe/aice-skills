use async_trait::async_trait;
use core_skills::ENABLED_SKILL_IDS;
use metrics::{counter, histogram};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::time::Instant;

const CHAIN_INVOCATIONS_TOTAL: &str = "skill_chain_invocations_total";
const CHAIN_DURATION_SECONDS: &str = "skill_chain_execution_duration_seconds";
const CHAIN_PLANNER_ERRORS_TOTAL: &str = "skill_chain_planner_errors_total";
const CHAIN_STEP_ERRORS_TOTAL: &str = "skill_chain_step_errors_total";
const CHAIN_TTS_ERRORS_TOTAL: &str = "skill_chain_tts_errors_total";
const CHAIN_CONFIRMATION_TOTAL: &str = "skill_chain_confirmation_total";

const CORE_COMMON_SKILLS: &[&str] = &[
    "skill_time",
    "skill_weather",
    "skill_distance",
    "skill_sports_live",
    "skill_holiday_lookup",
    "skill_fuel_price_lookup",
    "skill_horoscope_daily",
    "skill_news_headlines",
];

const MACOS_SKILLS: &[&str] = &[
    "skill_smart_home",
    "skill_memory",
    "skill_assistant",
    "skill_media",
    "skill_computer",
    "skill_record_window",
    "skill_screenshot",
    "skill_app_switcher",
    "skill_reminder",
    "skill_timer",
    "skill_shopping_list",
    "skill_message",
    "skill_volume",
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveCapabilities {
    pub frontend_enabled_skills: Vec<String>,
    pub backend_enabled_skills: Vec<String>,
    pub classifier_enabled_skills: Vec<String>,
}

impl EffectiveCapabilities {
    pub fn from_handshake(
        frontend_enabled_skills: Vec<String>,
        backend_enabled_skills: Vec<String>,
    ) -> Self {
        let frontend_set: BTreeSet<String> = frontend_enabled_skills.into_iter().collect();
        let backend_set: BTreeSet<String> = backend_enabled_skills.into_iter().collect();
        let backend_core: BTreeSet<String> = backend_set
            .iter()
            .filter(|skill_id| CORE_COMMON_SKILLS.contains(&skill_id.as_str()))
            .cloned()
            .collect();

        let mut classifier = frontend_set.clone();
        classifier.extend(backend_core);

        Self {
            frontend_enabled_skills: frontend_set.into_iter().collect(),
            backend_enabled_skills: backend_set.into_iter().collect(),
            classifier_enabled_skills: classifier.into_iter().collect(),
        }
    }

    pub fn has_backend_skill(&self, skill_id: &str) -> bool {
        self.backend_enabled_skills
            .iter()
            .any(|value| value == skill_id)
    }

    pub fn has_frontend_skill(&self, skill_id: &str) -> bool {
        self.frontend_enabled_skills
            .iter()
            .any(|value| value == skill_id)
    }
}

#[derive(Clone, Debug)]
pub struct PlannerRequest {
    pub user_phrase: String,
    pub context: BTreeMap<String, String>,
    pub capabilities: EffectiveCapabilities,
}

#[derive(Clone, Debug)]
pub struct ChainPlan {
    pub steps: Vec<ChainStep>,
}

#[derive(Clone, Debug)]
pub enum ChainStep {
    RunSkill(RunSkillStep),
    SpeakUpdate(SpeakUpdateStep),
    RequestConfirmation(RequestConfirmationStep),
    Done(DonePayload),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepTarget {
    Backend,
    Frontend,
}

#[derive(Clone, Debug)]
pub struct RunSkillStep {
    pub skill_id: String,
    pub input: String,
    pub target: StepTarget,
    pub side_effect: bool,
    pub retryable: bool,
    pub on_error: Vec<ChainStep>,
}

#[derive(Clone, Debug)]
pub struct SpeakUpdateStep {
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct RequestConfirmationStep {
    pub prompt: String,
    pub on_confirm: Vec<ChainStep>,
    pub on_reject: Vec<ChainStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DoneStatus {
    Success,
    Error,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DonePayload {
    pub status: DoneStatus,
    pub user_summary: String,
    pub structured_result: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainExecutionResult {
    pub done: DonePayload,
}
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ChainPlannerError {
    #[error("invalid intent: {0}")]
    InvalidIntent(String),
    #[error("planner unavailable: {0}")]
    Provider(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ChainExecutorError {
    #[error(transparent)]
    Planner(#[from] ChainPlannerError),
}

#[async_trait]
pub trait ChainPlanner: Send + Sync {
    async fn plan(&self, request: &PlannerRequest) -> Result<ChainPlan, ChainPlannerError>;
}

#[derive(Clone, Debug)]
pub struct SkillRunRequest {
    pub skill_id: String,
    pub input: String,
    pub target: StepTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillRunResult {
    pub summary: String,
    pub output: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillRunErrorKind {
    Transient,
    Permanent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillRunError {
    pub kind: SkillRunErrorKind,
    pub message: String,
}

#[async_trait]
pub trait SkillRunner: Send + Sync {
    async fn run(&self, request: &SkillRunRequest) -> Result<SkillRunResult, SkillRunError>;
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TtsError {
    #[error("tts unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait TtsNotifier: Send + Sync {
    async fn speak(&self, message: &str) -> Result<(), TtsError>;
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfirmationError {
    #[error("confirmation unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait ConfirmationHandler: Send + Sync {
    async fn confirm(&self, prompt: &str) -> Result<bool, ConfirmationError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SkillFamily {
    CoreCommon,
    Macos,
    Windows,
    Ios,
}

pub fn validate_plan_against_enabled_skills(
    plan: &ChainPlan,
    enabled_skills: &[String],
) -> Result<(), ChainPlannerError> {
    let capabilities = EffectiveCapabilities::from_handshake(Vec::new(), enabled_skills.to_vec());
    validate_plan_against_capabilities(plan, &capabilities)
}

pub fn validate_plan_against_capabilities(
    plan: &ChainPlan,
    capabilities: &EffectiveCapabilities,
) -> Result<(), ChainPlannerError> {
    let mut run_steps = Vec::new();
    collect_run_steps(&plan.steps, &mut run_steps);

    let mut os_families = HashSet::new();

    for run_step in run_steps {
        let allowed = match run_step.target {
            StepTarget::Backend => capabilities.has_backend_skill(&run_step.skill_id),
            StepTarget::Frontend => capabilities.has_frontend_skill(&run_step.skill_id),
        };
        if !allowed {
            return Err(ChainPlannerError::InvalidIntent(format!(
                "skill_not_enabled:{}",
                run_step.skill_id
            )));
        }

        match classify_skill_family(&run_step.skill_id) {
            Some(SkillFamily::CoreCommon) => {}
            Some(SkillFamily::Macos) => {
                os_families.insert(SkillFamily::Macos);
            }
            Some(SkillFamily::Windows) => {
                os_families.insert(SkillFamily::Windows);
            }
            Some(SkillFamily::Ios) => {
                os_families.insert(SkillFamily::Ios);
            }
            None => {
                return Err(ChainPlannerError::InvalidIntent(format!(
                    "unknown_skill_family:{}",
                    run_step.skill_id
                )));
            }
        }
    }

    if os_families.len() > 1 {
        return Err(ChainPlannerError::InvalidIntent(
            "mixed_os_skill_chain_not_allowed".to_string(),
        ));
    }

    Ok(())
}

fn collect_run_steps(steps: &[ChainStep], out: &mut Vec<RunSkillStep>) {
    for step in steps {
        match step {
            ChainStep::RunSkill(run) => {
                out.push(run.clone());
                collect_run_steps(&run.on_error, out);
            }
            ChainStep::RequestConfirmation(confirm) => {
                collect_run_steps(&confirm.on_confirm, out);
                collect_run_steps(&confirm.on_reject, out);
            }
            ChainStep::SpeakUpdate(_) | ChainStep::Done(_) => {}
        }
    }
}

fn classify_skill_family(skill_id: &str) -> Option<SkillFamily> {
    if CORE_COMMON_SKILLS.contains(&skill_id) {
        return Some(SkillFamily::CoreCommon);
    }
    if MACOS_SKILLS.contains(&skill_id) {
        return Some(SkillFamily::Macos);
    }
    if skill_id.contains("windows") {
        return Some(SkillFamily::Windows);
    }
    if skill_id.contains("ios") {
        return Some(SkillFamily::Ios);
    }
    None
}

pub struct ChainExecutor<P, R, T, C> {
    planner: P,
    runner: R,
    tts: T,
    confirmation: C,
    max_steps: usize,
}

impl<P, R, T, C> ChainExecutor<P, R, T, C> {
    pub fn new(planner: P, runner: R, tts: T, confirmation: C) -> Self {
        Self {
            planner,
            runner,
            tts,
            confirmation,
            max_steps: 12,
        }
    }

    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps.max(1);
        self
    }
}

impl<P, R, T, C> ChainExecutor<P, R, T, C>
where
    P: ChainPlanner,
    R: SkillRunner,
    T: TtsNotifier,
    C: ConfirmationHandler,
{
    pub async fn execute(
        &self,
        user_phrase: &str,
        context: BTreeMap<String, String>,
    ) -> Result<ChainExecutionResult, ChainExecutorError> {
        let capabilities = EffectiveCapabilities::from_handshake(
            Vec::new(),
            ENABLED_SKILL_IDS.iter().map(|id| id.to_string()).collect(),
        );
        self.execute_with_capabilities(user_phrase, context, capabilities)
            .await
    }

    pub async fn execute_with_capabilities(
        &self,
        user_phrase: &str,
        context: BTreeMap<String, String>,
        capabilities: EffectiveCapabilities,
    ) -> Result<ChainExecutionResult, ChainExecutorError> {
        let started_at = Instant::now();
        counter!(CHAIN_INVOCATIONS_TOTAL, 1);

        let request = PlannerRequest {
            user_phrase: user_phrase.to_string(),
            context,
            capabilities,
        };

        let plan = self.planner.plan(&request).await.map_err(|error| {
            counter!(CHAIN_PLANNER_ERRORS_TOTAL, 1, "kind" => error.to_string());
            ChainExecutorError::Planner(error)
        })?;

        validate_plan_against_capabilities(&plan, &request.capabilities).map_err(|error| {
            counter!(CHAIN_PLANNER_ERRORS_TOTAL, 1, "kind" => error.to_string());
            ChainExecutorError::Planner(error)
        })?;

        let mut queue = VecDeque::from(plan.steps);
        let mut step_count = 0usize;
        let mut structured = BTreeMap::new();
        let mut allow_side_effect_next = false;

        loop {
            if step_count >= self.max_steps {
                histogram!(CHAIN_DURATION_SECONDS, started_at.elapsed().as_secs_f64());
                return Ok(ChainExecutionResult {
                    done: DonePayload {
                        status: DoneStatus::Error,
                        user_summary: "error_loop_guard".to_string(),
                        structured_result: structured,
                    },
                });
            }

            let Some(step) = queue.pop_front() else {
                histogram!(CHAIN_DURATION_SECONDS, started_at.elapsed().as_secs_f64());
                return Ok(ChainExecutionResult {
                    done: DonePayload {
                        status: DoneStatus::Error,
                        user_summary: "error_loop_guard".to_string(),
                        structured_result: structured,
                    },
                });
            };

            step_count += 1;
            match step {
                ChainStep::RunSkill(run_step) => {
                    if run_step.side_effect && !allow_side_effect_next {
                        return Ok(ChainExecutionResult {
                            done: DonePayload {
                                status: DoneStatus::Cancelled,
                                user_summary: "cancelled_confirmation_required".to_string(),
                                structured_result: structured,
                            },
                        });
                    }
                    allow_side_effect_next = false;

                    let request = SkillRunRequest {
                        skill_id: run_step.skill_id.clone(),
                        input: run_step.input.clone(),
                        target: run_step.target,
                    };
                    let mut result = self.runner.run(&request).await;
                    if run_step.retryable {
                        if let Err(error) = &result {
                            if error.kind == SkillRunErrorKind::Transient {
                                result = self.runner.run(&request).await;
                            }
                        }
                    }

                    match result {
                        Ok(skill_result) => {
                            structured
                                .insert(run_step.skill_id.clone(), skill_result.summary.clone());
                            if let Err(error) = self
                                .tts
                                .speak(&format!(
                                    "Completed {}. {}",
                                    run_step.skill_id, skill_result.summary
                                ))
                                .await
                            {
                                counter!(CHAIN_TTS_ERRORS_TOTAL, 1, "kind" => error.to_string());
                            }
                        }
                        Err(error) => {
                            counter!(CHAIN_STEP_ERRORS_TOTAL, 1, "skill_id" => run_step.skill_id.clone(), "kind" => format!("{:?}", error.kind));
                            if run_step.on_error.is_empty() {
                                histogram!(
                                    CHAIN_DURATION_SECONDS,
                                    started_at.elapsed().as_secs_f64()
                                );
                                return Ok(ChainExecutionResult {
                                    done: DonePayload {
                                        status: DoneStatus::Error,
                                        user_summary: format!(
                                            "error_skill_failed:{}",
                                            run_step.skill_id
                                        ),
                                        structured_result: structured,
                                    },
                                });
                            }
                            for fallback in run_step.on_error.into_iter().rev() {
                                queue.push_front(fallback);
                            }
                        }
                    }
                }
                ChainStep::SpeakUpdate(step) => {
                    if let Err(error) = self.tts.speak(&step.message).await {
                        counter!(CHAIN_TTS_ERRORS_TOTAL, 1, "kind" => error.to_string());
                    }
                }
                ChainStep::RequestConfirmation(step) => {
                    let confirmed = self
                        .confirmation
                        .confirm(&step.prompt)
                        .await
                        .unwrap_or(false);
                    counter!(CHAIN_CONFIRMATION_TOTAL, 1, "result" => if confirmed { "confirmed" } else { "rejected" });
                    if confirmed {
                        allow_side_effect_next = true;
                        for next in step.on_confirm.into_iter().rev() {
                            queue.push_front(next);
                        }
                    } else {
                        for next in step.on_reject.into_iter().rev() {
                            queue.push_front(next);
                        }
                    }
                }
                ChainStep::Done(mut done) => {
                    for (key, value) in structured {
                        done.structured_result.entry(key).or_insert(value);
                    }
                    histogram!(CHAIN_DURATION_SECONDS, started_at.elapsed().as_secs_f64());
                    return Ok(ChainExecutionResult { done });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_mixed_os_skills() {
        let plan = ChainPlan {
            steps: vec![
                ChainStep::RunSkill(RunSkillStep {
                    skill_id: "skill_weather".to_string(),
                    input: "Berlin".to_string(),
                    target: StepTarget::Backend,
                    side_effect: false,
                    retryable: false,
                    on_error: vec![],
                }),
                ChainStep::RunSkill(RunSkillStep {
                    skill_id: "skill_app_switcher".to_string(),
                    input: "mail".to_string(),
                    target: StepTarget::Frontend,
                    side_effect: false,
                    retryable: false,
                    on_error: vec![],
                }),
                ChainStep::RunSkill(RunSkillStep {
                    skill_id: "skill_windows_fake".to_string(),
                    input: "x".to_string(),
                    target: StepTarget::Frontend,
                    side_effect: false,
                    retryable: false,
                    on_error: vec![],
                }),
            ],
        };

        let capabilities = EffectiveCapabilities::from_handshake(
            vec![
                "skill_app_switcher".to_string(),
                "skill_windows_fake".to_string(),
            ],
            vec!["skill_weather".to_string()],
        );

        let result = validate_plan_against_capabilities(&plan, &capabilities);
        assert!(matches!(
            result,
            Err(ChainPlannerError::InvalidIntent(msg)) if msg == "mixed_os_skill_chain_not_allowed"
        ));
    }

    #[test]
    fn handshake_classifier_contains_frontend_and_backend_core_only() {
        let capabilities = EffectiveCapabilities::from_handshake(
            vec![
                "skill_shopping_list".to_string(),
                "skill_app_switcher".to_string(),
            ],
            vec![
                "skill_weather".to_string(),
                "skill_time".to_string(),
                "skill_message".to_string(),
            ],
        );

        assert!(capabilities
            .classifier_enabled_skills
            .contains(&"skill_shopping_list".to_string()));
        assert!(capabilities
            .classifier_enabled_skills
            .contains(&"skill_app_switcher".to_string()));
        assert!(capabilities
            .classifier_enabled_skills
            .contains(&"skill_weather".to_string()));
        assert!(capabilities
            .classifier_enabled_skills
            .contains(&"skill_time".to_string()));
        assert!(!capabilities
            .classifier_enabled_skills
            .contains(&"skill_message".to_string()));
    }
}
