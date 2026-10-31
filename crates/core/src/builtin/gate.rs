//! `gate`: the parameters of the shaper `gate`, declared once, here. What it does is still done in Python.

use crate::formats::Problem;

/// The two levels the gate opens and closes at.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// The level at which a closed gate opens: the input at or above it. Raising it asks for a stronger input.
    #[param(default = 0.5, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub open: f64,
    /// The level an open gate closes below: the input under it. At most `open`; the gap between the two is the band
    /// in which the gate keeps its state, which stops a flicker. Equal to `open` is a plain threshold.
    #[param(default = 0.4, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub close: f64,
    /// Internal samples per second; the gate changes on one of them. Raising it is more exact and slower.
    #[param(default = 1000.0, unit = Hertz, min = 10, max = 48000, advanced)]
    pub rate: f64,
}

/// `close` must not lie above `open`: the gate would close at a level above the one that opens it.
fn rule(params: &Params) -> Vec<Problem> {
    if params.close <= params.open {
        return Vec::new();
    }
    vec![Problem {
        path: "close".to_owned(),
        rule: format!("must not be above open = {:?} (got {:?})", params.open, params.close),
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
    fn the_closing_level_must_not_be_above_the_opening_one() {
        for (id, given, expected) in [
            ("the defaults", json!({}), &[][..]),
            ("close below open", json!({"open": 0.6, "close": 0.2}), &[]),
            ("close equal to open", json!({"open": 0.5, "close": 0.5}), &[]),
            ("close above open", json!({"open": 0.2, "close": 0.6}), &[("close", "must not be above open = 0.2 (got 0.6)")]),
        ] {
            check(id, &given, expected);
        }
    }
}
