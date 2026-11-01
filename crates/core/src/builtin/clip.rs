//! A signal that follows its input but never leaves the range from `low` to `high`.
//!
//! Lower values are raised to `low`, higher ones lowered to `high`, and what lies inside stays exactly as it is. The
//! clip applies to the signal, not to its samples: between two samples that cross the range it is cut where the line
//! crosses.

use crate::formats::Problem;
use crate::{Error, FromValues, Interpolation, Shaper, Signal, maximum, minimum};

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

impl Signal {
    /// This signal kept between `params.low` and `params.high`: the shaper [`clip`](super), as [`pipe`](Signal::pipe)
    /// of `params` makes it (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::builtin::clip;
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// let line = Signal::from_values(&[-1.0, 3.0], 1.0, FromValues::default(), Interpolation::Linear)?;
    /// let kept = line.clip(clip::Params { low: 0.0, high: 1.0 })?;
    /// assert_eq!(kept.sample(&[0.0, 0.25, 0.5, 0.75])?, [0.0, 0.0, 1.0, 1.0]);
    /// assert!(line.clip(clip::Params { low: 1.0, high: 0.5 }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe`]: [`Error::Params`] under the name `clip` listing every rule `params` breaks (a field out of
    /// its range or not finite, or `high` below `low`).
    pub fn clip(&self, params: Params) -> Result<Signal, Error> {
        self.pipe(params)
    }
}

impl Shaper for Params {
    const NAME: &'static str = "clip";
    type Inputs = ();

    /// `input` raised to `low`, then lowered to `high`.
    fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> {
        Ok(minimum(&maximum(input, &holding(self.low)?), &holding(self.high)?))
    }
}

/// A signal that holds `value` at every time. Made as a one-sample series: a built-in's file names no other built-in,
/// and `Signal::constant` shares its name with the built-in `constant` (`tests/test_layering.py` reads paths).
fn holding(value: f64) -> Result<Signal, Error> {
    Signal::from_values(&[value], 1.0, FromValues::default(), Interpolation::Hold)
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
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;
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

    fn params(low: f64, high: f64) -> Params {
        Params { low, high }
    }

    /// `values` one a second, held (so each reads as itself in the middle of its second), clipped, read at `times`.
    fn clipped(values: &[f64], range: Params, times: &[f64]) -> Result<Vec<f64>, Error> {
        Signal::from_values(values, 1.0, FromValues::default(), Interpolation::Hold)?.clip(range)?.sample(times)
    }

    #[test]
    fn values_below_low_and_above_high_are_pulled_in_and_the_rest_stay_even_on_the_bounds() -> Result<(), Error> {
        // (low, high, input) -> expected, each worked by hand
        let rows = [
            (0.0, 1.0, vec![-2.0, 0.25, 0.5, 3.0], vec![0.0, 0.25, 0.5, 1.0]),
            (0.0, 1.0, vec![0.0, 1.0, -0.5, 1.5], vec![0.0, 1.0, 0.0, 1.0]), // the bounds themselves stay
            (2.0, 2.0, vec![0.0, 2.0, 5.0], vec![2.0, 2.0, 2.0]),             // low equal to high: one value
            (-3.0, -1.0, vec![-4.0, -2.0, 0.0], vec![-3.0, -2.0, -1.0]),     // a range below 0
            (-1e6, 1e6, vec![-1e9, 1e6, 1e300], vec![-1e6, 1e6, 1e6]),        // the ends of the parameter range
        ];
        for (low, high, input, expected) in rows {
            let times: Vec<f64> = (0..input.len()).map(|index| f64::from(u32::try_from(index).unwrap_or(0))).collect();
            assert_eq!(clipped(&input, params(low, high), &times)?, expected, "{low}..{high} over {input:?}");
        }
        Ok(())
    }

    #[test]
    fn between_samples_the_clip_applies_to_the_signal_not_to_the_samples() -> Result<(), Error> {
        // a line from -1 to 3 over a second crosses 0 at 0.25 s and 1 at 0.5 s
        let line = Signal::from_values(&[-1.0, 3.0], 1.0, FromValues::default(), Interpolation::Linear)?;
        let out = line.clip(params(0.0, 1.0))?.sample(&[0.125, 0.25, 0.375, 0.5, 0.75])?;
        assert_eq!(out, [0.0, 0.0, 0.5, 1.0, 1.0]);
        Ok(())
    }

    #[test]
    fn the_defaults_clip_to_0_and_1() -> Result<(), Error> {
        let kept = Signal::from_values(&[-1.0, 2.0], 1.0, FromValues::default(), Interpolation::Hold)?
            .pipe(Params::default())?;
        assert_eq!(kept.sample(&[0.0, 1.0])?, [0.0, 1.0]);
        Ok(())
    }

    #[test]
    fn a_high_below_low_is_refused_naming_high() -> Result<(), Error> {
        let signal = holding(0.5)?;
        let refused = signal.clip(params(1.0, 0.5));
        assert!(
            matches!(&refused, Err(Error::Params { shaper, problems, .. })
                if shaper == "clip" && problems.len() == 1 && problems[0].path == "high"),
            "{refused:?}"
        );
        Ok(())
    }

    proptest! {
        #[test]
        fn the_output_never_leaves_the_range_and_leaves_what_is_inside_exactly_as_it_was(
            values in prop::collection::vec(-50.0_f64..=50.0, 1..10),
            low in -20.0_f64..=0.0,
            high in 0.0_f64..=20.0,
            times in prop::collection::vec(0.0_f64..=12.0, 1..15),
        ) {
            let input = Signal::from_values(&values, 1.0, FromValues::default(), Interpolation::Linear)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let out = input.clip(params(low, high)).and_then(|kept| kept.sample(&times))
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let before = input.sample(&times).map_err(|error| TestCaseError::fail(error.to_string()))?;
            for (kept, original) in out.iter().zip(&before) {
                prop_assert!(*kept >= low && *kept <= high, "{kept} outside {low}..{high}");
                if (low..=high).contains(original) {
                    prop_assert!((kept - original).abs() <= 0.0, "{original} inside the range became {kept}");
                }
            }
        }
    }
}
