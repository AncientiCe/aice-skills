//! Unit conversion skill types and trait.

use async_trait::async_trait;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dimension {
    Length,
    Mass,
    Volume,
    Time,
    Temperature,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversionResult {
    pub input_value: f64,
    pub from_unit: String,
    pub to_unit: String,
    pub output_value: f64,
    pub formatted: String,
    pub dimension: Dimension,
}

impl ConversionResult {
    pub fn to_prompt_context(&self) -> String {
        format!(
            "{} {} = {} {}",
            self.input_value, self.from_unit, self.formatted, self.to_unit
        )
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum UnitConversionError {
    #[error("unknown unit: {0}")]
    UnknownUnit(String),
    #[error("dimension mismatch: cannot convert {from} to {to}")]
    DimensionMismatch { from: String, to: String },
    #[error("invalid value: {0}")]
    InvalidValue(String),
    #[error("could not parse query: {0}")]
    ParseError(String),
}

/// Unit conversion skill: convert values between compatible units locally.
#[async_trait]
pub trait UnitConversionSkill: Send + Sync {
    async fn execute(
        &self,
        value: f64,
        from_unit: &str,
        to_unit: &str,
    ) -> Result<ConversionResult, UnitConversionError>;

    /// Parse a free-form query like "cups in a liter" or "5km to miles".
    async fn execute_query(&self, query: &str) -> Result<ConversionResult, UnitConversionError>;
}
