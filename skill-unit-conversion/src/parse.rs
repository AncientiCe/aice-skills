//! Parse free-form unit-conversion queries like "5km to miles", "cups in a
//! liter", "convert 100 grams to ounces".

use crate::types::UnitConversionError;

#[derive(Debug, PartialEq)]
pub(crate) struct ParsedQuery {
    pub value: f64,
    pub from_unit: String,
    pub to_unit: String,
}

const TRIM_PREFIXES: &[&str] = &[
    "convert ",
    "how many ",
    "how much is ",
    "what is ",
    "what's ",
];

pub(crate) fn parse(query: &str) -> Result<ParsedQuery, UnitConversionError> {
    let mut s = query.trim().to_lowercase();
    s = s.trim_end_matches(['?', '.', '!']).to_string();
    for prefix in TRIM_PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }
    s = s.replace(" into ", " to ");

    // Detect "[X] [unit_a] in (a|an|the)? [unit_b]" where the article on the
    // right side signals "how many [unit_a] are in 1 [unit_b]". In that case
    // we swap left/right so the explicit article side becomes the source.
    let s = if let Some((left, right_raw)) = s.split_once(" in ") {
        let right = right_raw.trim();
        let starts_with_article =
            right.starts_with("a ") || right.starts_with("an ") || right.starts_with("the ");
        if starts_with_article {
            format!("1 {} to {}", strip_articles(right), left.trim())
        } else {
            format!("{} to {}", left.trim(), right.trim())
        }
    } else {
        s
    };

    let (left, right) = s
        .split_once(" to ")
        .ok_or_else(|| UnitConversionError::ParseError(query.to_string()))?;

    let (value, from_unit) = split_value_and_unit(left.trim())?;
    let to_unit = strip_articles(right.trim()).to_string();

    if to_unit.is_empty() {
        return Err(UnitConversionError::ParseError(query.to_string()));
    }

    Ok(ParsedQuery {
        value,
        from_unit,
        to_unit,
    })
}

fn split_value_and_unit(left: &str) -> Result<(f64, String), UnitConversionError> {
    let glued: Option<(f64, String)> = split_at_alpha_boundary(left);
    if let Some((value, unit)) = glued {
        return Ok((value, unit));
    }
    let mut parts = left.split_whitespace();
    let first = parts
        .next()
        .ok_or_else(|| UnitConversionError::ParseError(left.to_string()))?;
    if let Ok(value) = first.parse::<f64>() {
        let rest: Vec<&str> = parts.collect();
        let unit = strip_articles(&rest.join(" "));
        if unit.is_empty() {
            return Err(UnitConversionError::ParseError(left.to_string()));
        }
        return Ok((value, unit));
    }
    let unit = strip_articles(left);
    Ok((1.0, unit))
}

fn split_at_alpha_boundary(s: &str) -> Option<(f64, String)> {
    let chars: Vec<char> = s.chars().collect();
    let mut split_at = None;
    for (idx, ch) in chars.iter().enumerate() {
        if ch.is_alphabetic() {
            split_at = Some(idx);
            break;
        }
    }
    let split = split_at?;
    if split == 0 {
        return None;
    }
    let head: String = chars[..split].iter().collect();
    let tail: String = chars[split..].iter().collect();
    let value: f64 = head.trim().parse().ok()?;
    let unit = strip_articles(tail.trim());
    if unit.is_empty() {
        return None;
    }
    Some((value, unit))
}

fn strip_articles(s: &str) -> String {
    let mut out = s.trim().to_string();
    for article in ["a ", "an ", "the "] {
        if let Some(rest) = out.strip_prefix(article) {
            out = rest.to_string();
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_glued_unit() {
        let p = parse("5km to miles").unwrap();
        assert_eq!(
            p,
            ParsedQuery {
                value: 5.0,
                from_unit: "km".into(),
                to_unit: "miles".into(),
            }
        );
    }

    #[test]
    fn parses_separated_value_and_unit() {
        let p = parse("convert 100 grams to ounces").unwrap();
        assert_eq!(p.value, 100.0);
        assert_eq!(p.from_unit, "grams");
        assert_eq!(p.to_unit, "ounces");
    }

    #[test]
    fn cups_in_a_liter_means_one_liter_to_cups() {
        let p = parse("cups in a liter").unwrap();
        assert_eq!(p.value, 1.0);
        assert_eq!(p.from_unit, "liter");
        assert_eq!(p.to_unit, "cups");
    }

    #[test]
    fn ml_in_a_tablespoon_means_one_tablespoon_to_ml() {
        let p = parse("how many ml in a tablespoon?").unwrap();
        assert_eq!(p.value, 1.0);
        assert_eq!(p.from_unit, "tablespoon");
        assert_eq!(p.to_unit, "ml");
    }

    #[test]
    fn explicit_value_in_unit_keeps_direction() {
        let p = parse("5 km in miles").unwrap();
        assert_eq!(p.value, 5.0);
        assert_eq!(p.from_unit, "km");
        assert_eq!(p.to_unit, "miles");
    }

    #[test]
    fn rejects_query_without_to() {
        assert!(matches!(
            parse("5 km"),
            Err(UnitConversionError::ParseError(_))
        ));
    }
}
