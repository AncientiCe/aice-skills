//! Calculator skill types and trait.

use async_trait::async_trait;

/// Structured calculator result the LLM can render as a spoken answer.
#[derive(Clone, Debug, PartialEq)]
pub struct CalculatorResult {
    /// Original normalized expression as evaluated (e.g. `"15% of 240"`).
    pub expression: String,
    /// Final numeric value.
    pub value: f64,
    /// Display-formatted value (avoids scientific notation when reasonable).
    pub formatted: String,
}

impl CalculatorResult {
    /// Format for inclusion in an LLM prompt.
    pub fn to_prompt_context(&self) -> String {
        format!("{} = {}", self.expression, self.formatted)
    }
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum CalculatorSkillError {
    #[error("empty expression")]
    EmptyExpression,
    #[error("could not parse expression: {0}")]
    ParseError(String),
    #[error("evaluation produced non-finite value")]
    NonFinite,
}

/// Calculator skill: evaluate math expressions locally with no network.
#[async_trait]
pub trait CalculatorSkill: Send + Sync {
    async fn execute(&self, expression: &str) -> Result<CalculatorResult, CalculatorSkillError>;
}
