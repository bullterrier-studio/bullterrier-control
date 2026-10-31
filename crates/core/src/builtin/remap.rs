//! `remap`: the parameters of the shaper `remap`, declared once, here. What it does is still done in Python.

use crate::easing::Ease;
use crate::formats::Problem;

/// The range the input is put onto, and the ease that bends the way there.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// The output when the input is 0; set above `high` to flip the signal.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub low: f64,
    /// The output when the input is 1.
    #[param(default = 1.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub high: f64,
    /// How the input is bent before it is put onto the range: one of the eases keyframes take (linear, hold, in, out,
    /// in-out, cubic-in, cubic-out, cubic-in-out, overshoot) or cubic-bezier(x1, y1, x2, y2). 'linear' keeps the
    /// shape.
    #[param(default = "linear")]
    pub ease: String,
}

/// `ease` must name a curve ([`Ease`]: one of its names, or `cubic-bezier(x1, y1, x2, y2)`); the refusal is the one
/// `Ease` gives, which lists the valid forms.
fn rule(params: &Params) -> Vec<Problem> {
    match params.ease.parse::<Ease>() {
        Ok(_) => Vec::new(),
        Err(error) => vec![Problem {
            path: "ease".to_owned(),
            rule: error.to_string(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Params;
    use crate::Params as _;

    // whether the ease is a curve is `Ease`'s to decide (and tested there); here, that `remap` asks it, names its field
    // and says what `Ease` says
    #[test]
    fn the_ease_must_name_a_curve() {
        for (ease, refusal) in [
            ("linear", None),
            ("overshoot", None),
            ("cubic-bezier(0.25, 0.1, 0.25, 1)", None),
            ("wobble", Some("unknown ease 'wobble': use linear, hold")),
            ("Linear", Some("unknown ease 'Linear'")),
            ("", Some("unknown ease ''")),
            ("cubic-bezier(1.5,0,1,1)", Some("x1 and x2 must be in 0..1 (got 1.5 and 1)")),
        ] {
            let given = Params::parse(&json!({ "ease": ease }));
            match (&given, refusal) {
                (Ok(params), None) => assert_eq!(params.ease, ease),
                (Err(error), Some(part)) => {
                    let found: Vec<(&str, bool)> = error
                        .problems
                        .iter()
                        .map(|problem| (problem.path.as_str(), problem.rule.contains(part)))
                        .collect();
                    assert_eq!(found, [("ease", true)], "{ease:?}: {:?} lacks {part:?}", error.problems);
                }
                // a verdict that is not the row's
                _ => assert_eq!(given.is_ok(), refusal.is_none(), "{ease:?}: {given:?}, wanted {refusal:?}"),
            }
        }
    }
}
