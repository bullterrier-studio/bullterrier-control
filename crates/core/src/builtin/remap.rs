//! `remap`: the parameters of the shaper `remap`, declared once, here. What it does is still done in Python.

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

/// The named eases, as Python's `bullterrier_control.easing.EASES` names them (a test there keeps the two equal; R3
/// moves easing to Rust and this copy goes).
const EASES: [&str; 9] = [
    "linear",
    "hold",
    "in",
    "out",
    "in-out",
    "cubic-in",
    "cubic-out",
    "cubic-in-out",
    "overshoot",
];

/// `ease` must name a curve: one of [`EASES`] exactly, or `cubic-bezier(x1, y1, x2, y2)` with `x1` and `x2` in 0 to 1 and
/// finite `y1` and `y2` (spaces allowed around the whole and around each number).
fn rule(params: &Params) -> Vec<Problem> {
    if EASES.contains(&params.ease.as_str()) || is_bezier(&params.ease) {
        return Vec::new();
    }
    vec![Problem {
        path: "ease".to_owned(),
        rule: format!(
            "must be one of {}, or cubic-bezier(x1, y1, x2, y2) with x1 and x2 in 0..1 and finite y1 and y2 (got {:?})",
            EASES.join(", "),
            params.ease
        ),
    }]
}

/// Whether `text` is `cubic-bezier(x1, y1, x2, y2)` with four numbers (`1`, `-.5`, `2.`, `1e-3`), `x1` and `x2` in 0 to
/// 1 and `y1` and `y2` finite.
fn is_bezier(text: &str) -> bool {
    let Some(inside) = text
        .trim()
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    let numbers: Vec<Option<f64>> = inside.split(',').map(|part| number(part.trim())).collect();
    let [Some(x1), Some(y1), Some(x2), Some(y2)] = numbers[..] else {
        return false;
    };
    (0.0..=1.0).contains(&x1) && (0.0..=1.0).contains(&x2) && y1.is_finite() && y2.is_finite()
}

/// The number `text` writes, if it is one: an optional sign, digits with an optional point (`2.`, `.5`, `1.5`), an
/// optional exponent. (Not `inf`, `nan` or a hexadecimal, which `f64::from_str` would take.)
fn number(text: &str) -> Option<f64> {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = unsigned.split_once(['e', 'E']).map_or((unsigned, None), |(m, e)| (m, Some(e)));
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    let (whole, fraction) = mantissa.split_once('.').map_or((mantissa, None), |(w, f)| (w, Some(f)));
    let mantissa_ok = match fraction {
        None => digits(whole),
        Some(fraction) => (digits(whole) && (fraction.is_empty() || digits(fraction))) || (whole.is_empty() && digits(fraction)),
    };
    let exponent_ok = exponent.is_none_or(|exponent| digits(exponent.strip_prefix(['+', '-']).unwrap_or(exponent)));
    if mantissa_ok && exponent_ok { text.parse().ok() } else { None }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Params;
    use crate::Params as _;
    use crate::formats::check_problems;

    fn accepted(ease: &str) -> bool {
        match Params::parse(&json!({ "ease": ease })) {
            Ok(_) => true,
            Err(error) => {
                check_problems(ease, &error.problems, &[("ease", "must be one of linear, hold")]);
                false
            }
        }
    }

    #[test]
    fn the_ease_must_name_a_curve() {
        // each row's verdict is what Python's `curve(name)` gives (checked there when the rows were written)
        for (ease, ok) in [
            ("linear", true),
            ("hold", true),
            ("in", true),
            ("out", true),
            ("in-out", true),
            ("cubic-in", true),
            ("cubic-out", true),
            ("cubic-in-out", true),
            ("overshoot", true),
            ("cubic-bezier(0.25, 0.1, 0.25, 1)", true),
            ("cubic-bezier(0.1,-2,0.9,3)", true),
            ("  cubic-bezier( .5 , 1e0,+0.5,2. )  ", true),
            ("cubic-bezier(1.,0,1,1)", true),
            ("cubic-bezier(0,0,1,1)", true),
            ("cubic-bezier(1.5,0,1,1)", false),
            ("cubic-bezier(0,0,-0.1,1)", false),
            ("cubic-bezier(0,1e999,1,1)", false),
            ("cubic-bezier(1e+1,0,1,1)", false),
            ("cubic-bezier(0,0,1)", false),
            ("cubic-bezier(0,0,1,1,1)", false),
            ("cubic-bezier(a,0,1,1)", false),
            ("cubic-bezier(.,0,1,1)", false),
            ("cubic-bezier(1e,0,1,1)", false),
            ("cubic-bezier(0,,1,1)", false),
            ("cubic-bezier(0,0,1,1) x", false),
            ("cubic-bezier (0,0,1,1)", false),
            ("cubic-bezier(0,0,1,1", false),
            ("cubic-bezier(0,0,1,1))", false),
            ("nonsense", false),
            ("", false),
            ("Linear", false),
            ("linear ", false),
        ] {
            assert_eq!(accepted(ease), ok, "{ease:?}");
        }
    }

    #[test]
    fn a_refused_ease_is_listed_with_the_text_given() {
        let result = Params::parse(&json!({"ease": "wobble"}));
        assert!(
            matches!(&result, Err(error) if error.problems[0].rule.ends_with("(got \"wobble\")")),
            "{result:?}"
        );
    }
}
