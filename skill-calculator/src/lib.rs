//! Calculator skill: local expression evaluation, no network.

mod eval;
mod types;

pub use types::{CalculatorResult, CalculatorSkill, CalculatorSkillError};

use async_trait::async_trait;
use metrics::{counter, histogram};
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "calculator";

/// Local calculator implementation; no external dependencies at runtime.
#[derive(Default)]
pub struct LocalCalculatorSkill;

impl LocalCalculatorSkill {
    pub fn new() -> Self {
        Self
    }

    fn error_kind(error: &CalculatorSkillError) -> &'static str {
        match error {
            CalculatorSkillError::EmptyExpression => "empty_expression",
            CalculatorSkillError::ParseError(_) => "parse_error",
            CalculatorSkillError::NonFinite => "non_finite",
        }
    }

    fn format_value(value: f64) -> String {
        if value == value.trunc() && value.abs() < 1e15 {
            format!("{}", value as i64)
        } else {
            let rounded = (value * 1e6).round() / 1e6;
            let mut s = format!("{rounded}");
            if s.contains('.') {
                s = s.trim_end_matches('0').trim_end_matches('.').to_string();
            }
            s
        }
    }
}

#[async_trait]
impl CalculatorSkill for LocalCalculatorSkill {
    async fn execute(&self, expression: &str) -> Result<CalculatorResult, CalculatorSkillError> {
        let started_at = Instant::now();
        let normalized = eval::normalize(expression);
        let result = eval::evaluate(expression).map(|value| CalculatorResult {
            expression: normalized,
            value,
            formatted: Self::format_value(value),
        });

        let mut result_label = "success";
        let mut error_kind = "none";
        if let Err(err) = &result {
            result_label = "error";
            error_kind = Self::error_kind(err);
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

/// Mock implementation for downstream tests.
pub struct MockCalculatorSkill {
    pub result: Result<CalculatorResult, CalculatorSkillError>,
}

impl MockCalculatorSkill {
    pub fn ok(result: CalculatorResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(e: CalculatorSkillError) -> Self {
        Self { result: Err(e) }
    }
}

#[async_trait]
impl CalculatorSkill for MockCalculatorSkill {
    async fn execute(&self, _expression: &str) -> Result<CalculatorResult, CalculatorSkillError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_skill_returns_integer_formatting_for_whole_results() {
        let skill = LocalCalculatorSkill::new();
        let result = skill.execute("2 + 3").await.expect("ok");
        assert_eq!(result.value, 5.0);
        assert_eq!(result.formatted, "5");
    }

    #[tokio::test]
    async fn local_skill_returns_short_decimal_for_fractions() {
        let skill = LocalCalculatorSkill::new();
        let result = skill.execute("1 / 3").await.expect("ok");
        assert!(result.formatted.starts_with("0.333"));
    }

    #[tokio::test]
    async fn local_skill_solves_voice_percent() {
        let skill = LocalCalculatorSkill::new();
        let result = skill.execute("15 percent of 240").await.expect("ok");
        assert_eq!(result.value, 36.0);
        assert_eq!(result.formatted, "36");
    }

    #[tokio::test]
    async fn local_skill_propagates_parse_error() {
        let skill = LocalCalculatorSkill::new();
        let result = skill.execute("definitely not math").await;
        assert!(matches!(result, Err(CalculatorSkillError::ParseError(_))));
    }
}
