//! Unit conversion skill: local conversion tables, no network.

mod parse;
mod table;
mod types;

pub use types::{ConversionResult, Dimension, UnitConversionError, UnitConversionSkill};

use async_trait::async_trait;
use metrics::{counter, histogram};
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "unit_conversion";

#[derive(Default)]
pub struct LocalUnitConversionSkill;

impl LocalUnitConversionSkill {
    pub fn new() -> Self {
        Self
    }

    fn error_kind(error: &UnitConversionError) -> &'static str {
        match error {
            UnitConversionError::UnknownUnit(_) => "unknown_unit",
            UnitConversionError::DimensionMismatch { .. } => "dimension_mismatch",
            UnitConversionError::InvalidValue(_) => "invalid_value",
            UnitConversionError::ParseError(_) => "parse_error",
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

    fn execute_inner(
        &self,
        value: f64,
        from_unit: &str,
        to_unit: &str,
    ) -> Result<ConversionResult, UnitConversionError> {
        let from = table::lookup(from_unit)
            .ok_or_else(|| UnitConversionError::UnknownUnit(from_unit.to_string()))?;
        let to = table::lookup(to_unit)
            .ok_or_else(|| UnitConversionError::UnknownUnit(to_unit.to_string()))?;
        if from.dimension != to.dimension {
            return Err(UnitConversionError::DimensionMismatch {
                from: from_unit.to_string(),
                to: to_unit.to_string(),
            });
        }
        let out =
            table::convert(value, from, to).ok_or(UnitConversionError::DimensionMismatch {
                from: from_unit.to_string(),
                to: to_unit.to_string(),
            })?;
        Ok(ConversionResult {
            input_value: value,
            from_unit: from.canonical.to_string(),
            to_unit: to.canonical.to_string(),
            output_value: out,
            formatted: Self::format_value(out),
            dimension: from.dimension,
        })
    }

    fn record(&self, started_at: Instant, result: &Result<ConversionResult, UnitConversionError>) {
        let mut result_label = "success";
        let mut error_kind = "none";
        if let Err(err) = result {
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
    }
}

#[async_trait]
impl UnitConversionSkill for LocalUnitConversionSkill {
    async fn execute(
        &self,
        value: f64,
        from_unit: &str,
        to_unit: &str,
    ) -> Result<ConversionResult, UnitConversionError> {
        let started_at = Instant::now();
        let result = self.execute_inner(value, from_unit, to_unit);
        self.record(started_at, &result);
        result
    }

    async fn execute_query(&self, query: &str) -> Result<ConversionResult, UnitConversionError> {
        let started_at = Instant::now();
        let parsed = match parse::parse(query) {
            Ok(p) => p,
            Err(err) => {
                let result = Err(err);
                self.record(started_at, &result);
                return result;
            }
        };
        let result = self.execute_inner(parsed.value, &parsed.from_unit, &parsed.to_unit);
        self.record(started_at, &result);
        result
    }
}

pub struct MockUnitConversionSkill {
    pub result: Result<ConversionResult, UnitConversionError>,
}

impl MockUnitConversionSkill {
    pub fn ok(result: ConversionResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(e: UnitConversionError) -> Self {
        Self { result: Err(e) }
    }
}

#[async_trait]
impl UnitConversionSkill for MockUnitConversionSkill {
    async fn execute(
        &self,
        _value: f64,
        _from_unit: &str,
        _to_unit: &str,
    ) -> Result<ConversionResult, UnitConversionError> {
        self.result.clone()
    }

    async fn execute_query(&self, _query: &str) -> Result<ConversionResult, UnitConversionError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn converts_via_explicit_units() {
        let skill = LocalUnitConversionSkill::new();
        let result = skill.execute(5.0, "km", "miles").await.expect("ok");
        assert_eq!(result.from_unit, "kilometer");
        assert_eq!(result.to_unit, "mile");
        assert!((result.output_value - 3.10685596).abs() < 1e-6);
    }

    #[tokio::test]
    async fn converts_via_query_string() {
        let skill = LocalUnitConversionSkill::new();
        let result = skill
            .execute_query("100 grams to ounces")
            .await
            .expect("ok");
        assert!((result.output_value - 3.527396195).abs() < 1e-6);
    }

    #[tokio::test]
    async fn rejects_unknown_unit() {
        let skill = LocalUnitConversionSkill::new();
        let err = skill
            .execute(1.0, "wibble", "meter")
            .await
            .expect_err("err");
        assert_eq!(err, UnitConversionError::UnknownUnit("wibble".into()));
    }

    #[tokio::test]
    async fn rejects_dimension_mismatch() {
        let skill = LocalUnitConversionSkill::new();
        let err = skill.execute(1.0, "meter", "kg").await.expect_err("err");
        assert!(matches!(err, UnitConversionError::DimensionMismatch { .. }));
    }
}
