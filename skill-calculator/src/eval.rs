//! Natural-language pre-processor + meval evaluator.
//!
//! Converts voice-friendly phrases like `"15 percent of 240"` and `"32 fahrenheit
//! to celsius"` into a numeric value, falling back to a direct expression evaluator
//! for plain math like `"2 * (3 + 4)"`.

use crate::types::CalculatorSkillError;

/// Evaluate a natural-language or symbolic math expression.
pub fn evaluate(raw: &str) -> Result<f64, CalculatorSkillError> {
    let normalized = normalize(raw);
    if normalized.trim().is_empty() {
        return Err(CalculatorSkillError::EmptyExpression);
    }

    if let Some(value) = try_temperature_conversion(&normalized) {
        return finite(value);
    }
    if let Some(value) = try_percent_of(&normalized) {
        return finite(value);
    }
    let expr = symbolic(&normalized);
    let value: f64 =
        meval::eval_str(&expr).map_err(|err| CalculatorSkillError::ParseError(err.to_string()))?;
    finite(value)
}

fn finite(value: f64) -> Result<f64, CalculatorSkillError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(CalculatorSkillError::NonFinite)
    }
}

/// Lower-case + collapse whitespace + strip trivia ("what is", trailing "?").
pub(crate) fn normalize(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();
    for prefix in [
        "what is ",
        "what's ",
        "whats ",
        "calculate ",
        "compute ",
        "tell me ",
        "how much is ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }
    s = s.trim_end_matches(['?', '.', '!']).to_string();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Replace voice tokens with symbolic operators that meval can parse.
///
/// Longer phrases must be replaced before shorter ones to avoid the shorter
/// match consuming a substring of a longer one (e.g. " power " inside
/// " to the power of ").
fn symbolic(s: &str) -> String {
    s.replace(" to the power of ", " ^ ")
        .replace(" multiplied by ", " * ")
        .replace(" divided by ", " / ")
        .replace(" plus ", " + ")
        .replace(" minus ", " - ")
        .replace(" times ", " * ")
        .replace(" over ", " / ")
        .replace(" mod ", " % ")
        .replace(" power ", " ^ ")
        .replace(" squared", " ^ 2")
        .replace(" cubed", " ^ 3")
        .replace('×', "*")
        .replace('÷', "/")
}

/// Match patterns like "15 percent of 240" or "15% of 240".
fn try_percent_of(s: &str) -> Option<f64> {
    let s = s.replace(" percent of ", "% of ");
    let (left, right) = s.split_once("% of ")?;
    let pct: f64 = left.trim().parse().ok()?;
    let base: f64 = right.trim().parse().ok()?;
    Some(pct / 100.0 * base)
}

/// Match patterns like "32 fahrenheit to celsius" / "100 celsius to kelvin".
fn try_temperature_conversion(s: &str) -> Option<f64> {
    let (value_part, unit_part) = s.split_once(' ')?;
    let value: f64 = value_part.parse().ok()?;
    let unit_part = unit_part.replace(" in ", " to ");
    let parts: Vec<&str> = unit_part.split(" to ").collect();
    if parts.len() != 2 {
        return None;
    }
    let from = canonical_temp_unit(parts[0].trim())?;
    let to = canonical_temp_unit(parts[1].trim())?;
    let kelvin = to_kelvin(value, from);
    Some(from_kelvin(kelvin, to))
}

fn canonical_temp_unit(unit: &str) -> Option<&'static str> {
    match unit {
        "f" | "fahrenheit" | "degrees fahrenheit" | "degrees f" => Some("F"),
        "c" | "celsius" | "centigrade" | "degrees celsius" | "degrees c" => Some("C"),
        "k" | "kelvin" | "degrees kelvin" => Some("K"),
        _ => None,
    }
}

fn to_kelvin(value: f64, unit: &str) -> f64 {
    match unit {
        "F" => (value - 32.0) * 5.0 / 9.0 + 273.15,
        "C" => value + 273.15,
        "K" => value,
        _ => value,
    }
}

fn from_kelvin(kelvin: f64, unit: &str) -> f64 {
    match unit {
        "F" => (kelvin - 273.15) * 9.0 / 5.0 + 32.0,
        "C" => kelvin - 273.15,
        "K" => kelvin,
        _ => kelvin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "expected {} got {}", b, a);
    }

    #[test]
    fn evaluates_plain_math() {
        approx(evaluate("2 + 3 * 4").unwrap(), 14.0);
        approx(evaluate("(2 + 3) * 4").unwrap(), 20.0);
    }

    #[test]
    fn evaluates_voice_operators() {
        approx(evaluate("2 plus 3").unwrap(), 5.0);
        approx(evaluate("10 divided by 4").unwrap(), 2.5);
        approx(evaluate("4 squared").unwrap(), 16.0);
        approx(evaluate("3 to the power of 4").unwrap(), 81.0);
    }

    #[test]
    fn rejects_word_numbers() {
        assert!(evaluate("two plus three").is_err());
    }

    #[test]
    fn evaluates_percent_of() {
        approx(evaluate("15 percent of 240").unwrap(), 36.0);
        approx(evaluate("15% of 240").unwrap(), 36.0);
    }

    #[test]
    fn evaluates_temperature_conversion() {
        approx(evaluate("32 fahrenheit to celsius").unwrap(), 0.0);
        approx(evaluate("100 celsius to fahrenheit").unwrap(), 212.0);
        approx(evaluate("0 celsius to kelvin").unwrap(), 273.15);
    }

    #[test]
    fn strips_voice_prefixes() {
        approx(evaluate("what is 2 + 2?").unwrap(), 4.0);
        approx(evaluate("calculate 10 * 5").unwrap(), 50.0);
    }

    #[test]
    fn rejects_empty_expression() {
        assert_eq!(evaluate("   "), Err(CalculatorSkillError::EmptyExpression));
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(
            evaluate("not math at all"),
            Err(CalculatorSkillError::ParseError(_))
        ));
    }

    #[test]
    fn rejects_division_by_zero() {
        assert_eq!(evaluate("1 / 0"), Err(CalculatorSkillError::NonFinite));
    }
}
