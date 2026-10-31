//! Easing curves: how progress from 0 to 1 is reshaped, by name or as a cubic Bezier.
//!
//! ```
//! use bullterrier_control::easing::Ease;
//!
//! let ease: Ease = "in-out".parse()?;
//! assert_eq!(ease.apply(0.25), 0.15625);
//! assert_eq!(ease.apply(1.0), 1.0);
//!
//! // CSS `cubic-bezier(x1, y1, x2, y2)`: the handles of a curve from (0, 0) to (1, 1)
//! let css: Ease = "cubic-bezier(0.25, 0.1, 0.25, 1)".parse()?;
//! assert!((css.apply(0.5) - 0.8024).abs() < 1e-3);
//! # Ok::<(), bullterrier_control::Error>(())
//! ```
//!
//! The named curves are Robert Penner's easing equations; the Bezier form and its meaning are CSS Easing Level 1.
//!
//! Promises:
//! - [`Ease::from_str`](std::str::FromStr) takes a name in [`Ease::NAMES`] or a `cubic-bezier(x1, y1, x2, y2)`
//!   (whitespace is free; `x1` and `x2` in 0..1, `y1` and `y2` any finite numbers). Anything else is an
//!   [`Error::Ease`] listing the valid forms.
//! - Progress is clipped to 0..1 (an infinity too; NaN is undefined, but never a panic). The ends are exact for every
//!   curve: 0 gives 0.0 and 1 gives 1.0.
//! - The same bits on every target: the curves use only `+ - * /` (no `powi`, `powf` or libm call).

use std::str::FromStr;

use crate::Error;

/// How progress from 0 to 1 is reshaped: one of nine named curves, or a cubic Bezier.
///
/// ```
/// use bullterrier_control::easing::Ease;
///
/// assert_eq!(Ease::CubicIn.apply(0.5), 0.125);
/// assert_eq!(Ease::Hold.apply(0.999), 0.0);
/// assert_eq!(Ease::Overshoot.apply(2.0), 1.0); // progress is clipped to 1 first
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Ease {
    /// `p`: the identity.
    Linear,
    /// 0 until progress reaches 1, then 1.
    Hold,
    /// `p` squared: starts slow.
    In,
    /// `1 - (1 - p)` squared: ends slow.
    Out,
    /// Smoothstep, `p * p * (3 - 2p)`: slow at both ends.
    InOut,
    /// `p` cubed.
    CubicIn,
    /// `1 - (1 - p)` cubed.
    CubicOut,
    /// `4 p` cubed below the middle, `1 - (2 - 2p)` cubed `/ 2` above it.
    CubicInOut,
    /// Penner's back-out: rises about 10 % past 1 and settles back to it.
    Overshoot,
    /// The curve from (0, 0) to (1, 1) with handles (`x1`, `y1`) and (`x2`, `y2`); `x1` and `x2` lie in 0..1.
    Bezier {
        /// The x of the first handle, in 0..1.
        x1: f64,
        /// The y of the first handle.
        y1: f64,
        /// The x of the second handle, in 0..1.
        x2: f64,
        /// The y of the second handle.
        y2: f64,
    },
}

/// Penner's overshoot constant: it makes the back-out rise about 10 % past the end before settling.
const PENNER_OVERSHOOT: f64 = 1.70158;

/// Halvings of the Bezier inversion: the parameter is then known to 2^-50 (about 9e-16), below float rounding.
const BISECTIONS: u32 = 50;

/// The named curves: the one place a name and its curve are paired.
const NAMED: [(&str, Ease); 9] = [
    ("linear", Ease::Linear),
    ("hold", Ease::Hold),
    ("in", Ease::In),
    ("out", Ease::Out),
    ("in-out", Ease::InOut),
    ("cubic-in", Ease::CubicIn),
    ("cubic-out", Ease::CubicOut),
    ("cubic-in-out", Ease::CubicInOut),
    ("overshoot", Ease::Overshoot),
];

impl Ease {
    /// The names of the nine named curves, in the order of the variants.
    pub const NAMES: [&'static str; 9] = {
        let mut names = [""; 9];
        let mut index = 0;
        while index < NAMED.len() {
            names[index] = NAMED[index].0;
            index += 1;
        }
        names
    };

    /// `progress` reshaped: clipped to 0..1 first, then bent; 0 gives exactly 0.0 and 1 exactly 1.0.
    ///
    /// ```
    /// use bullterrier_control::easing::Ease;
    ///
    /// assert_eq!(Ease::Out.apply(0.5), 0.75);
    /// assert_eq!(Ease::Out.apply(-3.0), 0.0);
    /// ```
    #[must_use]
    pub fn apply(self, progress: f64) -> f64 {
        // progress outside 0..1 is clipped to an end. A Bezier's bisection and the overshoot constant miss the ends
        // by about 1e-16, so the ends are forced for every curve.
        if progress <= 0.0 {
            0.0
        } else if progress >= 1.0 {
            1.0
        } else {
            self.shape(progress)
        }
    }

    /// Each of `progress` reshaped as [`Ease::apply`] does.
    ///
    /// ```
    /// use bullterrier_control::easing::Ease;
    ///
    /// assert_eq!(Ease::In.apply_all(&[0.0, 0.5, 1.0]), [0.0, 0.25, 1.0]);
    /// ```
    #[must_use]
    pub fn apply_all(self, progress: &[f64]) -> Vec<f64> {
        progress.iter().map(|&p| self.apply(p)).collect()
    }

    /// The curve at `p`, which lies strictly between 0 and 1. Cubes are written out as products on purpose.
    fn shape(self, p: f64) -> f64 {
        match self {
            Self::Linear => p,
            Self::Hold => 0.0, // p < 1 here
            Self::In => p * p,
            Self::Out => 1.0 - (1.0 - p) * (1.0 - p),
            Self::InOut => p * p * (3.0 - 2.0 * p),
            Self::CubicIn => p * p * p,
            Self::CubicOut => {
                let q = 1.0 - p;
                1.0 - q * q * q
            }
            Self::CubicInOut => {
                if p < 0.5 {
                    4.0 * p * p * p
                } else {
                    let q = -2.0 * p + 2.0;
                    1.0 - q * q * q / 2.0
                }
            }
            Self::Overshoot => {
                let q = p - 1.0;
                1.0 + (PENNER_OVERSHOOT + 1.0) * (q * q * q) + PENNER_OVERSHOOT * (q * q)
            }
            Self::Bezier { x1, y1, x2, y2 } => {
                // x(t) rises with t (its handles lie in 0..1), so bisection finds the t whose x is the progress
                let (mut low, mut high) = (0.0_f64, 1.0_f64);
                for _ in 0..BISECTIONS {
                    let middle = f64::midpoint(low, high);
                    if bezier_coordinate(x1, x2, middle) < p {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                bezier_coordinate(y1, y2, f64::midpoint(low, high))
            }
        }
    }
}

/// One coordinate of the Bezier from 0 to 1 with handles `a` and `b`, at parameter `t`.
fn bezier_coordinate(a: f64, b: f64, t: f64) -> f64 {
    let s = 1.0 - t;
    3.0 * a * (s * s) * t + 3.0 * b * s * (t * t) + t * t * t
}

impl FromStr for Ease {
    type Err = Error;

    /// The curve called `name`: a name in [`Ease::NAMES`] or a `cubic-bezier(x1, y1, x2, y2)`.
    ///
    /// ```
    /// use bullterrier_control::easing::Ease;
    ///
    /// assert_eq!("out".parse(), Ok(Ease::Out));
    /// assert_eq!(
    ///     " cubic-bezier( .5 ,0,1e0, 1.) ".parse(),
    ///     Ok(Ease::Bezier { x1: 0.5, y1: 0.0, x2: 1.0, y2: 1.0 })
    /// );
    /// let refused = "wobble".parse::<Ease>().map_err(|error| error.to_string());
    /// assert!(matches!(refused, Err(text) if text.starts_with("unknown ease 'wobble': use linear, hold")));
    /// ```
    ///
    /// # Errors
    /// [`Error::Ease`] listing the valid forms: for an unknown name or a Bezier that is not four numbers (or a `y` that
    /// is not finite), and, for a Bezier that is well formed but has `x1` or `x2` outside 0..1, saying so.
    fn from_str(name: &str) -> Result<Self, Error> {
        if let Some(&(_, curve)) = NAMED.iter().find(|(known, _)| *known == name) {
            return Ok(curve);
        }
        match bezier(name) {
            Some((x1, y1, x2, y2)) if (0.0..=1.0).contains(&x1) && (0.0..=1.0).contains(&x2) => {
                Ok(Self::Bezier { x1, y1, x2, y2 })
            }
            Some((x1, _, x2, _)) => Err(Error::Ease {
                message: format!(
                    "bad ease '{name}': x1 and x2 must be in 0..1 (got {x1} and {x2}); use {}",
                    forms()
                ),
            }),
            None => Err(Error::Ease {
                message: format!("unknown ease '{name}': use {}", forms()),
            }),
        }
    }
}

/// The valid forms, as an error lists them.
fn forms() -> String {
    format!(
        "{}, or cubic-bezier(x1, y1, x2, y2) with x1 and x2 in 0..1 and finite y1 and y2",
        Ease::NAMES.join(", ")
    )
}

/// The four numbers of `cubic-bezier(x1, y1, x2, y2)` (spaces allowed around the whole and each number), or `None` when
/// `text` is not of that form or `y1` or `y2` is not finite.
fn bezier(text: &str) -> Option<(f64, f64, f64, f64)> {
    let inside = text
        .trim()
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'))?;
    let numbers: Vec<Option<f64>> = inside.split(',').map(|part| number(part.trim())).collect();
    let [Some(x1), Some(y1), Some(x2), Some(y2)] = numbers[..] else {
        return None;
    };
    (y1.is_finite() && y2.is_finite()).then_some((x1, y1, x2, y2))
}

/// The number `text` writes, if it is one: an optional sign, digits with an optional point (`2.`, `.5`, `1.5`), an
/// optional exponent. (Not `inf`, `nan` or a hexadecimal, which `f64::from_str` would take.)
fn number(text: &str) -> Option<f64> {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(m, e)| (m, Some(e)));
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    let (whole, fraction) = mantissa.split_once('.').map_or((mantissa, None), |(w, f)| (w, Some(f)));
    let mantissa_ok = match fraction {
        None => digits(whole),
        Some(fraction) => {
            (digits(whole) && (fraction.is_empty() || digits(fraction))) || (whole.is_empty() && digits(fraction))
        }
    };
    let exponent_ok = exponent.is_none_or(|exponent| digits(exponent.strip_prefix(['+', '-']).unwrap_or(exponent)));
    if mantissa_ok && exponent_ok {
        text.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::Ease;
    use crate::Error;

    const THIRD: f64 = 1.0 / 3.0;

    fn bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> Ease {
        Ease::Bezier { x1, y1, x2, y2 }
    }

    /// The nine named curves, in the order of `Ease::NAMES`.
    fn named() -> [Ease; 9] {
        super::NAMED.map(|(_, curve)| curve)
    }

    #[test]
    fn the_names_are_these_nine_and_each_parses_to_its_curve() {
        assert_eq!(
            Ease::NAMES,
            [
                "linear",
                "hold",
                "in",
                "out",
                "in-out",
                "cubic-in",
                "cubic-out",
                "cubic-in-out",
                "overshoot"
            ]
        );
        for (name, curve) in Ease::NAMES.iter().zip(named()) {
            assert_eq!(name.parse::<Ease>(), Ok(curve), "{name}");
        }
    }

    // each value is worked out by hand from the formulas in the docs, at a progress with a non-symmetric result
    #[test]
    fn the_named_curves_take_their_published_values() {
        for (curve, progress, expected, rule) in [
            (Ease::Linear, 0.3, 0.3, "linear is the identity"),
            (Ease::In, 0.5, 0.25, "in is p squared"),
            (Ease::Out, 0.5, 0.75, "out is 1 - (1-p) squared"),
            (Ease::InOut, 0.5, 0.5, "in-out passes through the middle"),
            (Ease::InOut, 0.2, 0.104, "in-out is smoothstep: 0.04 * (3 - 0.4)"),
            (Ease::CubicIn, 0.5, 0.125, "cubic-in is p cubed"),
            (Ease::CubicOut, 0.5, 0.875, "cubic-out is 1 - (1-p) cubed"),
            (Ease::CubicInOut, 0.25, 0.0625, "below the middle: 4 p cubed"),
            (Ease::CubicInOut, 0.75, 0.9375, "above the middle: 1 - (2-2p) cubed / 2"),
            (Ease::CubicInOut, 0.45, 0.3645, "just below the middle: 4 * 0.091125"),
            (Ease::CubicInOut, 0.55, 0.6355, "just above the middle: 1 - 0.729 / 2"),
            (Ease::Out, 1.2, 1.0, "progress above 1 is 1 (not 1 - 0.04)"),
            (Ease::Out, -0.1, 0.0, "progress below 0 is 0 (not 1 - 1.21)"),
            (
                Ease::Overshoot,
                0.5,
                1.087_697_5,
                "1 + 2.70158 * -0.125 + 1.70158 * 0.25",
            ),
            (Ease::Hold, 0.999, 0.0, "hold stays at 0 until the end"),
            (Ease::Hold, 1.0, 1.0, "hold is 1 at the end"),
        ] {
            let got = curve.apply(progress);
            assert!(
                (got - expected).abs() <= 1e-12,
                "{rule}: {curve:?} at {progress} gave {got}"
            );
        }
    }

    // CSS `ease` is quoted as about 0.8024 in the middle
    #[test]
    fn a_bezier_inverts_x_and_reads_y() {
        for (curve, progress, expected, tolerance, rule) in [
            // x(t) = t here, so the parameter is the progress: y(0.25) = 3 * 0.5625 * 0.25 + 0.25 ** 3
            (
                bezier(THIRD, 1.0, 2.0 * THIRD, 0.0),
                0.25,
                0.4375,
                1e-9,
                "x is linear, y is hand-summed",
            ),
            (
                bezier(THIRD, THIRD, 2.0 * THIRD, 2.0 * THIRD),
                0.6,
                0.6,
                1e-9,
                "a straight line",
            ),
            // x(0.25) = 0.1771875 + 0.0815625 + 0.015625 = 0.274375 and y(0.25) = 0.140625 + 0.015625 = 0.15625
            (
                bezier(0.42, 0.0, 0.58, 1.0),
                0.274_375,
                0.15625,
                1e-9,
                "inverts x at parameter 0.25",
            ),
            // the same curve with x1 and x2 swapped has x(0.25) = 0.319375, so a swap lands elsewhere
            (
                bezier(0.58, 0.0, 0.42, 1.0),
                0.319_375,
                0.15625,
                1e-9,
                "inverts x with x1, x2 swapped",
            ),
            (bezier(0.25, 0.1, 0.25, 1.0), 0.5, 0.8024, 1e-3, "the CSS ease"),
            // a handle at x = 1 is valid: at parameter 0.5, x = 0.375 + 0.1875 + 0.125 = 0.6875, y = 0.75 + 0.125
            (bezier(1.0, 2.0, 0.5, 0.0), 0.6875, 0.875, 1e-9, "x1 = 1 is allowed"),
            // x = t cubed and y = 3t^2 - 2t^3, so progress 0.125 is the parameter 0.5, where y = 0.75 - 0.25
            (bezier(0.0, 0.0, 0.0, 1.0), 0.125, 0.5, 1e-9, "both handles at x = 0"),
        ] {
            let got = curve.apply(progress);
            assert!(
                (got - expected).abs() <= tolerance,
                "{rule}: {curve:?} at {progress} gave {got}"
            );
        }
    }

    // the bisection's precision: the line's x(t) = t, so the answer is the progress itself, and 0.5 sits on the
    // boundary of the 2^-49 grid, where 49 halvings are 9e-16 off and 50 are 4.4e-16 off
    #[test]
    fn a_bezier_is_inverted_to_better_than_a_millionth_of_a_billionth() {
        let got = bezier(THIRD, THIRD, 2.0 * THIRD, 2.0 * THIRD).apply(0.5);
        assert!((got - 0.5).abs() < 6e-16, "{}", got - 0.5);
    }

    #[test]
    fn a_name_or_a_bezier_is_taken() {
        for (text, expected, rule) in [
            ("in-out", Ease::InOut, "a name"),
            (
                "cubic-bezier(0.42, 0, 0.58, 1)",
                bezier(0.42, 0.0, 0.58, 1.0),
                "a bezier",
            ),
            (
                "cubic-bezier(0.25,0.1,0.25,1)",
                bezier(0.25, 0.1, 0.25, 1.0),
                "no whitespace",
            ),
            (
                " cubic-bezier( 0.42 ,0 ,0.58, 1 ) ",
                bezier(0.42, 0.0, 0.58, 1.0),
                "whitespace is free",
            ),
            (
                "cubic-bezier(4.2e-1, 0, .58, 1.)",
                bezier(0.42, 0.0, 0.58, 1.0),
                "exponent and bare-dot numbers",
            ),
            (
                "cubic-bezier(0, 2, 1, 3)",
                bezier(0.0, 2.0, 1.0, 3.0),
                "x1 = 0 and x2 = 1 are valid",
            ),
            (
                "cubic-bezier(1, 2, 0, 3)",
                bezier(1.0, 2.0, 0.0, 3.0),
                "x at both ends of 0..1 is valid",
            ),
            (
                "cubic-bezier(0.3, -0.5, 0.7, 1.6)",
                bezier(0.3, -0.5, 0.7, 1.6),
                "y outside 0..1 is valid",
            ),
        ] {
            assert_eq!(text.parse::<Ease>(), Ok(expected), "{rule}: {text:?}");
        }
    }

    #[test]
    fn a_text_that_is_no_curve_is_refused_with_the_valid_forms_listed() {
        for (text, wanted, rule) in [
            (
                "wobble",
                "unknown ease 'wobble': use linear, hold, in, out, in-out",
                "an unknown name",
            ),
            ("", "unknown ease '': use linear", "an empty name"),
            ("Linear", "unknown ease 'Linear'", "names are case sensitive"),
            ("linear ", "unknown ease 'linear '", "a name takes no whitespace"),
            (
                "cubic-bezier(1.2, 0, 0, 1)",
                "x1 and x2 must be in 0..1 (got 1.2 and 0); use linear",
                "x1 above 1",
            ),
            (
                "cubic-bezier(0, 0, -0.1, 1)",
                "x1 and x2 must be in 0..1 (got 0 and -0.1)",
                "x2 below 0",
            ),
            (
                "cubic-bezier(-0.1, 0, 0.5, 1)",
                "x1 and x2 must be in 0..1 (got -0.1 and 0.5)",
                "x1 below 0",
            ),
            (
                "cubic-bezier(0.5, 0, 1.5, 1)",
                "x1 and x2 must be in 0..1 (got 0.5 and 1.5)",
                "x2 above 1",
            ),
            ("cubic-bezier(0.1, 0.2, 0.3)", "unknown ease", "three numbers"),
            ("cubic-bezier(0.1, 0.2, 0.3, 0.4, 0.5)", "unknown ease", "five numbers"),
            ("cubic-bezier(0.1, nan, 0.3, 0.4)", "unknown ease", "nan"),
            ("cubic-bezier(0.1, inf, 0.3, 0.4)", "unknown ease", "an infinity"),
            (
                "cubic-bezier(0.1, 1e999, 0.3, 0.4)",
                "unknown ease",
                "a y too large for a float",
            ),
            (
                "cubic-bezier(0.1, 0.2, 0.3, -1e999)",
                "unknown ease",
                "a y too small for a float",
            ),
            (
                "cubic-bezier(1e999, 0, 0.3, 0.4)",
                "x1 and x2 must be in 0..1 (got inf and 0.3)",
                "an x too large",
            ),
            ("cubic-bezier(a,b,c,d)", "unknown ease", "text"),
            (
                "cubic-bezier(1e+1,0,1,1)",
                "x1 and x2 must be in 0..1 (got 10 and 1)",
                "a signed exponent",
            ),
            ("cubic-bezier(.,0,1,1)", "unknown ease", "a lone point"),
            ("cubic-bezier(1e,0,1,1)", "unknown ease", "an exponent without digits"),
            ("cubic-bezier(0,,1,1)", "unknown ease", "an empty number"),
            ("cubic-bezier(0.1, 0.2, 0.3, 0.4", "unknown ease", "no closing bracket"),
            ("cubic-bezier(0,0,1,1))", "unknown ease", "two closing brackets"),
            ("cubic-bezier(0,0,1,1) x", "unknown ease", "text after"),
            ("cubic-bezier 0.1, 0.2, 0.3, 0.4", "unknown ease", "no brackets"),
            ("cubic-bezier (0,0,1,1)", "unknown ease", "a space before the bracket"),
        ] {
            let got = text.parse::<Ease>().map_err(|error| error.to_string());
            assert!(
                matches!(&got, Err(message) if message.contains(wanted)),
                "{rule}: {text:?} gave {got:?}, wanted a refusal with {wanted:?}"
            );
        }
    }

    #[test]
    fn a_bezier_that_is_not_well_formed_is_not_blamed_on_its_x() {
        let message = "cubic-bezier(0.1, 0.2, 0.3)"
            .parse::<Ease>()
            .map_err(|error| error.to_string());
        assert!(
            matches!(&message, Err(text) if text.starts_with("unknown ease") && !text.contains("must be in 0..1 (")),
            "{message:?}"
        );
    }

    #[test]
    fn every_error_lists_all_the_valid_forms() {
        let all = "linear, hold, in, out, in-out, cubic-in, cubic-out, cubic-in-out, overshoot, or cubic-bezier(x1, \
                   y1, x2, y2) with x1 and x2 in 0..1 and finite y1 and y2";
        for text in ["wobble", "cubic-bezier(2, 0, 0, 1)"] {
            let error = text.parse::<Ease>().map_err(|error| error.to_string());
            assert!(
                matches!(&error, Err(message) if message.ends_with(all)),
                "{text}: {error:?}"
            );
        }
    }

    #[test]
    fn overshoot_goes_past_the_end_before_settling() {
        let peak = (1..100)
            .map(|step| Ease::Overshoot.apply(f64::from(step) / 100.0))
            .fold(f64::NEG_INFINITY, f64::max);
        // the peak of Penner's back-out is about 1.1
        assert!(peak > 1.05 && peak < 1.15, "{peak}");
    }

    #[test]
    fn the_ends_are_exact_and_progress_outside_0_to_1_is_clipped() {
        let beziers = [
            bezier(0.42, 0.0, 0.58, 1.0),
            bezier(0.25, 0.1, 0.25, 1.0),
            bezier(0.0, 0.0, 0.0, 1.0),
            bezier(0.3, -0.5, 0.7, 1.6),
        ];
        for curve in named().into_iter().chain(beziers) {
            for (progress, expected) in [
                (0.0, 0.0),
                (1.0, 1.0),
                (-3.0, 0.0),
                (7.0, 1.0),
                (f64::NEG_INFINITY, 0.0),
                (f64::INFINITY, 1.0),
            ] {
                assert_eq!(curve.apply(progress), expected, "{curve:?} at {progress}");
            }
        }
    }

    #[test]
    fn apply_all_is_apply_at_each() {
        let curve = bezier(0.3, -0.5, 0.7, 1.6);
        let progress = [0.0, 0.1, 0.35, 0.8, 1.0, 2.0];
        let all = curve.apply_all(&progress);
        assert_eq!(all.len(), progress.len());
        for (got, &p) in all.iter().zip(&progress) {
            assert_eq!(*got, curve.apply(p), "{p}");
        }
        assert_eq!(Ease::Linear.apply_all(&[]), Vec::<f64>::new());
    }

    #[test]
    fn nan_progress_is_not_a_panic() {
        for curve in named().into_iter().chain([bezier(0.4, 0.0, 0.6, 1.0)]) {
            let _ = curve.apply(f64::NAN);
        }
    }

    #[test]
    fn the_error_is_an_ease_error() {
        assert!(matches!("wobble".parse::<Ease>(), Err(Error::Ease { .. })));
    }

    fn never_falls() -> Vec<Ease> {
        // Penner's back-out rises past 1 and returns; every other ease, and a bezier whose y stays in 0..1, only rises
        let mut curves: Vec<Ease> = named().into_iter().filter(|&curve| curve != Ease::Overshoot).collect();
        curves.extend([
            bezier(0.42, 0.0, 0.58, 1.0),
            bezier(0.25, 0.1, 0.25, 1.0),
            bezier(0.0, 0.0, 0.0, 1.0),
        ]);
        curves
    }

    proptest! {
        #[test]
        fn an_ease_never_falls_as_progress_rises(a in 0.0_f64..=1.0, b in 0.0_f64..=1.0) {
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            for curve in never_falls() {
                prop_assert!(curve.apply(low) <= curve.apply(high) + 1e-12, "{:?} at {} and {}", curve, low, high);
            }
        }

        // the curve's own definition (CSS Easing Level 1): the point at parameter `t`, with no inversion
        #[test]
        fn a_bezier_goes_through_the_points_of_its_definition(
            x1 in 0.1_f64..=0.9,
            y1 in -2.0_f64..=3.0,
            x2 in 0.1_f64..=0.9,
            y2 in -2.0_f64..=3.0,
            t in 0.0_f64..=1.0,
        ) {
            let s = 1.0 - t;
            let x = 3.0 * x1 * s * s * t + 3.0 * x2 * s * t * t + t * t * t;
            let y = 3.0 * y1 * s * s * t + 3.0 * y2 * s * t * t + t * t * t;
            let got = bezier(x1, y1, x2, y2).apply(x);
            prop_assert!((got - y).abs() <= 1e-9, "{} vs {}", got, y);
        }

        // handles on the diagonal make the curve the diagonal, whatever the handles are
        #[test]
        fn a_bezier_with_equal_handles_is_the_line(a in 0.0_f64..=1.0, b in 0.0_f64..=1.0, p in 0.0_f64..=1.0) {
            // x(t) = y(t) for x1 = y1 and x2 = y2, so y at the progress x is that progress
            let got = bezier(a, a, b, b).apply(p);
            prop_assert!((got - p).abs() <= 1e-9, "{} vs {}", got, p);
        }

        #[test]
        fn a_bezier_with_mirrored_handles_is_symmetric_about_the_centre(p in 0.0_f64..=1.0) {
            let curve = bezier(0.42, 0.0, 0.58, 1.0);
            prop_assert!((curve.apply(p) + curve.apply(1.0 - p) - 1.0).abs() <= 1e-9);
        }

        #[test]
        fn every_curve_gives_a_finite_value_for_any_progress(p in prop::num::f64::ANY) {
            prop_assume!(!p.is_nan());
            for curve in named().into_iter().chain([bezier(0.3, -0.5, 0.7, 1.6)]) {
                prop_assert!(curve.apply(p).is_finite(), "{:?} at {}", curve, p);
            }
        }
    }
}
