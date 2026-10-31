//! `every`: the parameters of the operation `every`, declared once, here. What it does is still done in Python.

use crate::formats::Problem;

/// Which of the events in time order to keep: every `n`th, counting from `offset`.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// Keep one event in this many, 1 to 1000: 1 keeps all, 2 every second one, 3 every third.
    #[param(default = 2, unit = Count, min = 1, max = 1000)]
    pub n: i64,
    /// How many events to skip before the first one kept, 0 to 999 and below `n`: 0 starts with the first event, 1
    /// with the second.
    #[param(default = 0, unit = Count, min = 0, max = 999)]
    pub offset: i64,
}

/// `offset` must lie below `n`: the kept positions are `offset`, `offset + n`, ... and the first lies within the first
/// `n`.
fn rule(params: &Params) -> Vec<Problem> {
    if params.offset < params.n {
        return Vec::new();
    }
    vec![Problem {
        path: "offset".to_owned(),
        rule: format!("must be below n = {} (got {})", params.n, params.offset),
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
    fn the_offset_must_be_below_n() {
        for (id, given, expected) in [
            ("the defaults", json!({}), &[][..]),
            ("one below n", json!({"n": 3, "offset": 2}), &[]),
            ("the first event of many", json!({"n": 7, "offset": 0}), &[]),
            ("equal to n", json!({"n": 3, "offset": 3}), &[("offset", "must be below n = 3 (got 3)")]),
            ("past n", json!({"n": 2, "offset": 5}), &[("offset", "must be below n = 2 (got 5)")]),
        ] {
            check(id, &given, expected);
        }
    }

    #[test]
    fn a_broken_field_rule_hides_the_rule_between_fields() {
        check(
            "offset past its own range",
            &json!({"n": 3, "offset": 1000}),
            &[("offset", "<= 999")],
        );
    }
}
