//! `normalise`: the parameters of the operation `normalise`, declared once, here. What it does is still done in Python.

use crate::formats::Problem;

/// The percentiles of the series that become 0 and 1.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// The share of the values, 0 to 0.99, at or below the level that becomes 0: 0.05 puts the 5th percentile at 0,
    /// and lower values are clipped to 0. Raising it clips more of the quiet end; below `upper`.
    #[param(default = 0.05, unit = Fraction, min = 0, max = 0.99)]
    pub lower: f64,
    /// The share of the values, 0.01 to 1, at or below the level that becomes 1: 0.98 puts the 98th percentile at 1,
    /// and higher values are clipped to 1. Lowering it clips more of the loud end; above `lower`.
    #[param(default = 0.98, unit = Fraction, min = 0.01, max = 1)]
    pub upper: f64,
}

/// `upper` must lie above `lower`: the two percentiles that become 1 and 0 are in order.
fn rule(params: &Params) -> Vec<Problem> {
    if params.upper > params.lower {
        return Vec::new();
    }
    vec![Problem {
        path: "upper".to_owned(),
        rule: format!("must be above lower = {:?} (got {:?})", params.lower, params.upper),
    }]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Params;
    use crate::Params as _;
    use crate::formats::check_problems;

    fn check(id: &str, given: &serde_json::Value, expected: &'static [(&'static str, &'static str)]) {
        match Params::parse(given) {
            Ok(params) => assert!(expected.is_empty(), "{id}: accepted {params:?}, wanted {expected:?}"),
            Err(error) => check_problems(id, &error.problems, expected),
        }
    }

    #[test]
    fn the_upper_percentile_must_be_above_the_lower_one() {
        for (id, given, expected) in [
            ("the defaults", json!({}), &[][..]),
            ("upper above lower", json!({"lower": 0.2, "upper": 0.5}), &[]),
            ("upper equal to lower", json!({"lower": 0.3, "upper": 0.3}), &[("upper", "must be above lower = 0.3 (got 0.3)")]),
            ("upper below lower", json!({"lower": 0.5, "upper": 0.2}), &[("upper", "must be above lower = 0.5 (got 0.2)")]),
            ("lower past its own range", json!({"lower": 2.0, "upper": 1.0}), &[("lower", "<= 0.99")]),
        ] {
            check(id, &given, expected);
        }
    }
}
