//! A signal that puts its input, taken as 0 to 1, onto the range from `low` to `high`, bent by an ease.
//!
//! An input outside 0 to 1 is clipped to it first: below 0 gives `low`, above 1 gives `high`. A `low` above `high`
//! inverts. The output is exactly inside the range: an ease that overshoots (`overshoot`, or a Bezier with a y outside
//! 0 to 1) is clipped to `low` and `high`.

use crate::easing::Ease;
use crate::formats::Problem;
use crate::scaling::between;
use crate::{Error, Shaper, Signal};

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

impl Signal {
    /// This signal, taken as 0 to 1, put onto `params.low`..`params.high` through `params.ease`: the shaper
    /// [`remap`](super), as [`pipe`](Signal::pipe) of `params` makes it (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::builtin::remap;
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// let level = Signal::from_values(&[0.0, 0.5, 2.0], 1.0, FromValues::default(), Interpolation::Hold)?;
    /// let opacity = level.remap(remap::Params { low: 40.0, high: 120.0, ease: "in".to_owned() })?;
    /// assert_eq!(opacity.sample(&[0.0, 1.0, 2.0])?, [40.0, 60.0, 120.0]); // 40 + 80 * 0.5^2; 2 is read as 1
    /// assert!(level.remap(remap::Params { ease: "wobble".to_owned(), ..Default::default() }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe`]: [`Error::Params`] under the name `remap` listing every rule `params` breaks (a field out
    /// of its range or not finite, an `ease` that names no curve).
    pub fn remap(&self, params: Params) -> Result<Signal, Error> {
        self.pipe(params)
    }
}

impl Shaper for Params {
    const NAME: &'static str = "remap";
    type Inputs = ();

    /// `input` through the ease (which clips its progress to 0..1), then onto `low`..`high`; the ease is parsed here,
    /// once, not on every read.
    fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> {
        let ease: Ease = self.ease.parse()?;
        let (low, high) = (self.low, self.high);
        input.mapped(move |value| between(low, high, ease.apply(value)))
    }
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
    use crate::{Error, FromValues, Interpolation, Params as _, Shaper, Signal};

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
    fn params(low: f64, high: f64, ease: &str) -> Params {
        Params {
            low,
            high,
            ease: ease.to_owned(),
        }
    }

    /// The remapped value of a signal that holds `value`, read at time 0.
    fn remapped(value: f64, params: Params) -> Result<f64, Error> {
        Signal::from_values(&[value], 1.0, FromValues::default(), Interpolation::Hold)?.remap(params)?.at(0.0)
    }

    // each ease's value at progress 0.25, worked by hand from its formula (Penner; CSS for the Bezier), put onto 40..120
    // as 40 + 80 * the eased value
    #[test]
    fn each_ease_bends_the_input_before_it_is_put_onto_the_range() -> Result<(), Error> {
        for (ease, expected, rule) in [
            ("linear", 60.0, "p: 40 + 80 * 0.25"),
            ("hold", 40.0, "0 until the end"),
            ("in", 45.0, "p^2 = 0.0625"),
            ("out", 75.0, "1 - (1 - p)^2 = 0.4375"),
            ("in-out", 52.5, "p^2 (3 - 2p) = 0.15625"),
            ("cubic-in", 41.25, "p^3 = 0.015625"),
            ("cubic-out", 86.25, "1 - 0.75^3 = 0.578125"),
            ("cubic-in-out", 45.0, "below the middle 4 p^3 = 0.0625"),
            ("overshoot", 105.392_775, "1 + 2.70158 (-0.75)^3 + 1.70158 (-0.75)^2 = 0.8174096875"),
            ("cubic-bezier(0, 0, 1, 1)", 60.0, "handles on the diagonal: the line"),
        ] {
            let got = remapped(0.25, params(40.0, 120.0, ease))?;
            assert!((got - expected).abs() <= 1e-9, "{ease} ({rule}): {got}");
        }
        Ok(())
    }

    // (low, high, ease, input) -> output, each by hand
    #[test]
    fn the_input_is_clipped_to_0_to_1_and_the_output_to_the_range_which_may_run_downward() -> Result<(), Error> {
        for (low, high, ease, input, expected, rule) in [
            (40.0, 120.0, "in", -0.3, 40.0, "below 0 reads as 0 (unclipped, in gives 0.09: 47.2)"),
            (40.0, 120.0, "out", 1.7, 120.0, "above 1 reads as 1 (unclipped, out gives 0.51: 80.8)"),
            (40.0, 120.0, "linear", 1e300, 120.0, "far above 1"),
            (1.0, 0.0, "linear", 0.25, 0.75, "low above high inverts: 1 - 0.25"),
            (120.0, 40.0, "in", 0.5, 100.0, "inverted, eased: 120 - 80 * 0.25"),
            (2.0, 2.0, "out", 0.3, 2.0, "low equal to high is that value"),
            (0.3, 0.9, "linear", 1.0, 0.9, "the top exactly, where 0.3 + 0.6 * 1 is 0.9000000000000001"),
            (0.3, 0.9, "linear", 0.0, 0.3, "the bottom exactly"),
            (0.0, 1.0, "overshoot", 0.5, 1.0, "an overshoot (1.0876975) is cut at high"),
            (1.0, 0.0, "overshoot", 0.5, 0.0, "inverted, the overshoot is cut at high below low"),
            (40.0, 120.0, "cubic-bezier(0.3, -2, 0.7, 3)", 0.1904, 40.0, "a Bezier dipping to -0.472 is cut at low"),
            (-1e6, 1e6, "linear", 0.75, 500_000.0, "the ends of the parameters' range"),
        ] {
            let got = remapped(input, params(low, high, ease))?;
            assert!((got - expected).abs() <= 1e-9, "{rule}: {low}..{high} {ease} at {input} gave {got}");
        }
        Ok(())
    }

    #[test]
    fn a_zero_of_either_sign_is_low() -> Result<(), Error> {
        for input in [0.0, -0.0] {
            assert_eq!(remapped(input, params(0.0, 1.0, "linear"))?.to_bits(), 0.0_f64.to_bits(), "{input:?}");
        }
        Ok(())
    }

    #[test]
    fn the_defaults_leave_0_to_1_alone() -> Result<(), Error> {
        let ramp = Signal::from_values(&[0.0, 1.0], 1.0, FromValues::default(), Interpolation::Linear)?;
        assert_eq!(ramp.pipe(Params::default())?.sample(&[0.0, 0.25, 1.0])?, [0.0, 0.25, 1.0]);
        Ok(())
    }

    // the ease is parsed when the shaper is made: one its table would refuse never makes a signal that fails on a read
    #[test]
    fn an_ease_that_names_no_curve_is_refused_when_the_shaper_is_made() -> Result<(), Error> {
        let level = Signal::from_values(&[0.5], 1.0, FromValues::default(), Interpolation::Hold)?;
        let made = params(0.0, 1.0, "wobble").shape(&level, ());
        assert!(matches!(&made, Err(Error::Ease { .. })), "{made:?}");
        let refused = level.remap(params(0.0, 1.0, "wobble"));
        assert!(
            matches!(&refused, Err(Error::Params { shaper, problems, .. })
                if shaper == "remap" && problems.len() == 1 && problems[0].path == "ease"),
            "{refused:?}"
        );
        Ok(())
    }
}
