//! `clip`: the parameters of the shaper `clip`, declared once, here. What it does is still done in Python.

use crate::formats::Problem;

/// The range a clipped signal stays inside.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// The lowest value the output takes; lower values are raised to it.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub low: f64,
    /// The highest value the output takes; higher values are lowered to it.
    #[param(default = 1.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub high: f64,
}

/// `high` must not lie below `low`: the range the output stays inside is empty otherwise.
fn rule(params: &Params) -> Vec<Problem> {
    if params.high >= params.low {
        return Vec::new();
    }
    vec![Problem {
        path: "high".to_owned(),
        rule: format!("must not be below low = {:?} (got {:?})", params.low, params.high),
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
    fn the_high_end_must_not_be_below_the_low_end() {
        for (id, given, expected) in [
            ("the defaults", json!({}), &[][..]),
            ("high above low", json!({"low": 0.25, "high": 0.75}), &[]),
            ("high equal to low", json!({"low": 0.5, "high": 0.5}), &[]),
            ("high below low", json!({"low": 0.5, "high": 0.25}), &[("high", "must not be below low = 0.5 (got 0.25)")]),
            ("high far below low", json!({"low": 1000, "high": -3}), &[("high", "low = 1000.0 (got -3.0)")]),
        ] {
            check(id, &given, expected);
        }
    }

    #[test]
    fn a_typed_struct_is_checked_by_the_rule_between_its_fields_too() {
        let refused = Params { low: 0.5, high: 0.25 }.check();
        assert!(refused.is_err(), "accepted {refused:?}");
        if let Err(error) = refused {
            check_problems("typed", &error.problems, &[("high", "must not be below low = 0.5")]);
        }
    }
}
