//! Conversion tables. Most dimensions use a base-unit factor; temperature
//! requires affine conversion through Kelvin.

use crate::types::Dimension;

/// (canonical_name, factor_to_base_unit, [aliases])
pub(crate) struct UnitDef {
    pub canonical: &'static str,
    pub factor: f64,
    pub aliases: &'static [&'static str],
    pub dimension: Dimension,
}

/// Length base unit: meter.
const LENGTH: &[UnitDef] = &[
    UnitDef {
        canonical: "meter",
        factor: 1.0,
        aliases: &["m", "meters", "metre", "metres"],
        dimension: Dimension::Length,
    },
    UnitDef {
        canonical: "kilometer",
        factor: 1000.0,
        aliases: &["km", "kilometers", "kilometre", "kilometres"],
        dimension: Dimension::Length,
    },
    UnitDef {
        canonical: "centimeter",
        factor: 0.01,
        aliases: &["cm", "centimeters", "centimetre", "centimetres"],
        dimension: Dimension::Length,
    },
    UnitDef {
        canonical: "millimeter",
        factor: 0.001,
        aliases: &["mm", "millimeters", "millimetre", "millimetres"],
        dimension: Dimension::Length,
    },
    UnitDef {
        canonical: "mile",
        factor: 1609.344,
        aliases: &["mi", "miles"],
        dimension: Dimension::Length,
    },
    UnitDef {
        canonical: "yard",
        factor: 0.9144,
        aliases: &["yd", "yards"],
        dimension: Dimension::Length,
    },
    UnitDef {
        canonical: "foot",
        factor: 0.3048,
        aliases: &["ft", "feet"],
        dimension: Dimension::Length,
    },
    UnitDef {
        canonical: "inch",
        factor: 0.0254,
        aliases: &["in", "inches"],
        dimension: Dimension::Length,
    },
];

/// Mass base unit: gram.
const MASS: &[UnitDef] = &[
    UnitDef {
        canonical: "gram",
        factor: 1.0,
        aliases: &["g", "grams"],
        dimension: Dimension::Mass,
    },
    UnitDef {
        canonical: "kilogram",
        factor: 1000.0,
        aliases: &["kg", "kilograms", "kilo", "kilos"],
        dimension: Dimension::Mass,
    },
    UnitDef {
        canonical: "milligram",
        factor: 0.001,
        aliases: &["mg", "milligrams"],
        dimension: Dimension::Mass,
    },
    UnitDef {
        canonical: "pound",
        factor: 453.59237,
        aliases: &["lb", "lbs", "pounds"],
        dimension: Dimension::Mass,
    },
    UnitDef {
        canonical: "ounce",
        factor: 28.349523125,
        aliases: &["oz", "ounces"],
        dimension: Dimension::Mass,
    },
    UnitDef {
        canonical: "ton",
        factor: 1_000_000.0,
        aliases: &["tons", "tonne", "tonnes", "metric_ton"],
        dimension: Dimension::Mass,
    },
];

/// Volume base unit: liter.
const VOLUME: &[UnitDef] = &[
    UnitDef {
        canonical: "liter",
        factor: 1.0,
        aliases: &["l", "liters", "litre", "litres"],
        dimension: Dimension::Volume,
    },
    UnitDef {
        canonical: "milliliter",
        factor: 0.001,
        aliases: &["ml", "milliliters", "millilitre", "millilitres"],
        dimension: Dimension::Volume,
    },
    UnitDef {
        canonical: "cup",
        factor: 0.2365882365,
        aliases: &["cups"],
        dimension: Dimension::Volume,
    },
    UnitDef {
        canonical: "tablespoon",
        factor: 0.01478676478125,
        aliases: &["tbsp", "tablespoons"],
        dimension: Dimension::Volume,
    },
    UnitDef {
        canonical: "teaspoon",
        factor: 0.00492892159375,
        aliases: &["tsp", "teaspoons"],
        dimension: Dimension::Volume,
    },
    UnitDef {
        canonical: "gallon",
        factor: 3.785411784,
        aliases: &["gal", "gallons"],
        dimension: Dimension::Volume,
    },
    UnitDef {
        canonical: "fluid_ounce",
        factor: 0.0295735295625,
        aliases: &["fl_oz", "fluid_ounces", "floz"],
        dimension: Dimension::Volume,
    },
    UnitDef {
        canonical: "pint",
        factor: 0.473176473,
        aliases: &["pints", "pt"],
        dimension: Dimension::Volume,
    },
    UnitDef {
        canonical: "quart",
        factor: 0.946352946,
        aliases: &["quarts", "qt"],
        dimension: Dimension::Volume,
    },
];

/// Time base unit: second.
const TIME: &[UnitDef] = &[
    UnitDef {
        canonical: "second",
        factor: 1.0,
        aliases: &["s", "seconds", "sec", "secs"],
        dimension: Dimension::Time,
    },
    UnitDef {
        canonical: "minute",
        factor: 60.0,
        aliases: &["min", "minutes", "mins"],
        dimension: Dimension::Time,
    },
    UnitDef {
        canonical: "hour",
        factor: 3600.0,
        aliases: &["h", "hr", "hours", "hrs"],
        dimension: Dimension::Time,
    },
    UnitDef {
        canonical: "day",
        factor: 86400.0,
        aliases: &["d", "days"],
        dimension: Dimension::Time,
    },
    UnitDef {
        canonical: "week",
        factor: 604800.0,
        aliases: &["wk", "weeks"],
        dimension: Dimension::Time,
    },
    UnitDef {
        canonical: "millisecond",
        factor: 0.001,
        aliases: &["ms", "milliseconds"],
        dimension: Dimension::Time,
    },
];

const TEMPERATURE: &[UnitDef] = &[
    UnitDef {
        canonical: "celsius",
        factor: 1.0,
        aliases: &["c", "centigrade"],
        dimension: Dimension::Temperature,
    },
    UnitDef {
        canonical: "fahrenheit",
        factor: 1.0,
        aliases: &["f"],
        dimension: Dimension::Temperature,
    },
    UnitDef {
        canonical: "kelvin",
        factor: 1.0,
        aliases: &["k"],
        dimension: Dimension::Temperature,
    },
];

const ALL_TABLES: &[&[UnitDef]] = &[LENGTH, MASS, VOLUME, TIME, TEMPERATURE];

/// Look up a unit by canonical name or alias (case-insensitive).
pub(crate) fn lookup(name: &str) -> Option<&'static UnitDef> {
    let needle = name.trim().to_lowercase();
    let needle = needle.replace(' ', "_");
    for table in ALL_TABLES {
        for unit in *table {
            if unit.canonical == needle {
                return Some(unit);
            }
            if unit.aliases.iter().any(|alias| *alias == needle) {
                return Some(unit);
            }
        }
    }
    None
}

/// Convert a value from one unit to another. Returns None when the units have
/// different dimensions; affine conversion is required for temperature.
pub(crate) fn convert(value: f64, from: &UnitDef, to: &UnitDef) -> Option<f64> {
    if from.dimension != to.dimension {
        return None;
    }
    if from.dimension == Dimension::Temperature {
        let kelvin = match from.canonical {
            "celsius" => value + 273.15,
            "fahrenheit" => (value - 32.0) * 5.0 / 9.0 + 273.15,
            "kelvin" => value,
            _ => return None,
        };
        let out = match to.canonical {
            "celsius" => kelvin - 273.15,
            "fahrenheit" => (kelvin - 273.15) * 9.0 / 5.0 + 32.0,
            "kelvin" => kelvin,
            _ => return None,
        };
        return Some(out);
    }
    let base = value * from.factor;
    Some(base / to.factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "expected {} got {}", b, a);
    }

    #[test]
    fn looks_up_aliases_case_insensitive() {
        assert_eq!(lookup("KM").unwrap().canonical, "kilometer");
        assert_eq!(lookup("Cups").unwrap().canonical, "cup");
        assert_eq!(lookup("fluid ounce").unwrap().canonical, "fluid_ounce");
    }

    #[test]
    fn converts_length() {
        let km = lookup("km").unwrap();
        let mi = lookup("mile").unwrap();
        approx(convert(5.0, km, mi).unwrap(), 3.10685596);
    }

    #[test]
    fn converts_volume_cups_in_liter() {
        let liter = lookup("liter").unwrap();
        let cup = lookup("cup").unwrap();
        approx(convert(1.0, liter, cup).unwrap(), 4.226752838);
    }

    #[test]
    fn converts_temperature_celsius_to_fahrenheit() {
        let c = lookup("celsius").unwrap();
        let f = lookup("fahrenheit").unwrap();
        approx(convert(100.0, c, f).unwrap(), 212.0);
        approx(convert(0.0, c, f).unwrap(), 32.0);
    }

    #[test]
    fn rejects_dimension_mismatch() {
        let m = lookup("meter").unwrap();
        let kg = lookup("kg").unwrap();
        assert!(convert(1.0, m, kg).is_none());
    }
}
