//! Template skill — copy this whole directory when adding a new skill.
//!
//! Replace every "template" with your skill name and remove what you
//! don't need. The conventions modeled here:
//!
//! - public trait `<Name>Skill` returning `<Name>Result` / `<Name>Error`;
//! - `to_prompt_context` on the result so the LLM can speak it;
//! - a real implementation and a `Mock<Name>Skill` for tests;
//! - errors as `thiserror` enums with a `kind()` helper for metrics;
//! - `cfg!(target_os = ...)` runtime gating for OS-specific logic.

mod types;

pub use types::{TemplateError, TemplateQuery, TemplateResult, TemplateSkill};

use async_trait::async_trait;
use metrics::{counter, histogram};
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";

/// Real implementation of the template skill.
pub struct LocalTemplateSkill;

impl LocalTemplateSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalTemplateSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TemplateSkill for LocalTemplateSkill {
    async fn execute(&self, query: &TemplateQuery) -> Result<TemplateResult, TemplateError> {
        let started_at = Instant::now();
        let result = run(query);
        record_metrics(&result, started_at);
        result
    }
}

fn run(query: &TemplateQuery) -> Result<TemplateResult, TemplateError> {
    let trimmed = query.input.trim();
    if trimmed.is_empty() {
        return Err(TemplateError::InvalidQuery("input is empty".to_string()));
    }
    Ok(TemplateResult {
        echoed: trimmed.to_string(),
    })
}

fn record_metrics(result: &Result<TemplateResult, TemplateError>, started_at: Instant) {
    let (label, kind) = match result {
        Ok(_) => ("success", "none"),
        Err(error) => ("error", error.kind()),
    };
    counter!(
        SKILL_EXECUTE_TOTAL,
        1,
        "skill" => "template",
        "result" => label,
        "error_kind" => kind
    );
    histogram!(
        SKILL_EXECUTE_DURATION_SECONDS,
        started_at.elapsed().as_secs_f64(),
        "skill" => "template"
    );
}

/// Stub implementation for orchestrator tests.
pub struct MockTemplateSkill {
    pub result: Result<TemplateResult, TemplateError>,
}

impl MockTemplateSkill {
    pub fn ok(result: TemplateResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: TemplateError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl TemplateSkill for MockTemplateSkill {
    async fn execute(&self, _query: &TemplateQuery) -> Result<TemplateResult, TemplateError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(input: &str) -> TemplateQuery {
        TemplateQuery {
            input: input.to_string(),
        }
    }

    #[tokio::test]
    async fn echoes_trimmed_input() {
        let skill = LocalTemplateSkill::new();
        let result = skill.execute(&q("  hello ")).await.expect("ok");
        assert_eq!(result.echoed, "hello");
    }

    #[tokio::test]
    async fn rejects_empty_input() {
        let skill = LocalTemplateSkill::new();
        let err = skill.execute(&q("   ")).await.expect_err("err");
        assert!(matches!(err, TemplateError::InvalidQuery(_)));
    }

    #[tokio::test]
    async fn mock_returns_configured_value() {
        let skill = MockTemplateSkill::ok(TemplateResult {
            echoed: "stub".to_string(),
        });
        let result = skill.execute(&q("ignored")).await.expect("ok");
        assert_eq!(result.echoed, "stub");
    }
}
