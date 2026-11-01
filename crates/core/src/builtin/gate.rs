//! A signal that is 1 while its input is high and 0 while it is low, with a gap between the levels that opens and
//! closes it (hysteresis).
//!
//! A closed gate opens when the input reaches `open` (at or above it); an open one closes when the input falls below
//! `close`. Between the two it keeps its state. At time 0 it is open if the input is at or above `open`. The change
//! happens in the internal sample where the input crosses, so a jump exactly on a sample is seen at that time.

use crate::formats::Problem;
use crate::{Error, Shaper, Signal, recurrence};

/// The two levels the gate opens and closes at.
#[derive(Clone, Copy, Debug, PartialEq, crate::Params)]
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

impl Signal {
    /// This signal as 1 while high and 0 while low, with the levels of `params`: the shaper [`gate`](super), as
    /// [`pipe`](Signal::pipe) of `params` makes it (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::builtin::gate;
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// let level = Signal::from_values(&[0.0, 0.9, 0.45, 0.2], 10.0, FromValues::default(), Interpolation::Hold)?;
    /// let gated = level.gate(gate::Params { open: 0.5, close: 0.4, rate: 10.0 })?;
    /// // opens at 0.9, stays open at 0.45 (between the levels), closes at 0.2
    /// assert_eq!(gated.sample(&[0.0, 0.1, 0.2, 0.3])?, [0.0, 1.0, 1.0, 0.0]);
    /// assert!(level.gate(gate::Params { open: 0.2, close: 0.6, rate: 10.0 }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe`]: [`Error::Params`] under the name `gate` listing every rule `params` breaks (a field out of
    /// its range or not finite, or `close` above `open`).
    pub fn gate(&self, params: Params) -> Result<Signal, Error> {
        self.pipe(params)
    }
}

impl Shaper for Params {
    const NAME: &'static str = "gate";
    type Inputs = ();

    /// Its causal recurrence over `input` sampled at `rate`, rendered as far as the reads need.
    fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> {
        recurrence::over(input, self.rate, *self, run)
    }
}

/// The gated series: closed to begin with, so the first sample opens it if it is at or above `open`; then a closed
/// gate opens at `open` or above, and an open one stays unless the input is below `close`. A NaN input is neither, so
/// it closes the gate.
fn run(inputs: &[f64], params: &Params) -> Vec<f64> {
    let mut is_open = false;
    inputs
        .iter()
        .map(|&level| {
            is_open = level >= if is_open { params.close } else { params.open };
            f64::from(u8::from(is_open))
        })
        .collect()
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
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;
    use crate::Params as _;
    use crate::formats::check_problems;
    use crate::{FromValues, Interpolation};

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

    fn params(open: f64, close: f64, rate: f64) -> Params {
        Params { open, close, rate }
    }

    /// The gate over `inputs` (a series at `params.rate`, holding each value), read at each input's time.
    fn gated(inputs: &[f64], params: &Params) -> Result<Vec<f64>, Error> {
        let input = Signal::from_values(inputs, params.rate, FromValues::default(), Interpolation::Hold)?;
        let times: Vec<f64> = (0..inputs.len()).map(|index| f64::from(u32::try_from(index).unwrap_or(0)) / params.rate).collect();
        params.shape(&input, ())?.sample(&times)
    }

    #[test]
    fn it_opens_at_open_closes_below_close_and_keeps_its_state_in_between() {
        // open 0.5, close 0.4
        let out = run(&[0.3, 0.55, 0.45, 0.42, 0.4, 0.38, 0.5, 0.6], &params(0.5, 0.4, 10.0));
        //   0.3 closed, under open; 0.55 opens; 0.45 and 0.42 in the band: stays open; 0.4 equals close, which does not
        //   close it; 0.38 below close: closes; 0.5 equals open: opens; 0.6 stays open
        assert_eq!(out, [0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn a_closed_gate_stays_closed_in_the_band_and_an_open_one_starts_open_at_open() {
        assert_eq!(run(&[0.3, 0.45, 0.42, 0.49, 0.45, 0.2], &params(0.5, 0.4, 10.0)), [0.0; 6]);
        assert_eq!(run(&[0.5, 0.45, 0.45], &params(0.5, 0.4, 10.0)), [1.0; 3]);
        assert_eq!(run(&[0.49, 0.45, 0.45], &params(0.5, 0.4, 10.0)), [0.0; 3]);
    }

    #[test]
    fn with_close_equal_to_open_it_is_a_plain_threshold_that_includes_the_threshold() {
        // x >= 0.5 whatever the state before: 0.5 opens a closed gate and, after 0.9, does not close an open one
        let out = run(&[0.1, 0.5, 0.499, 0.9, 0.5, 0.2], &params(0.5, 0.5, 10.0));
        assert_eq!(out, [0.0, 1.0, 0.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn levels_at_the_ends_of_the_parameter_range_gate_like_any_others() {
        let out = run(&[-1e6, 1e6, 0.0, -1e6, -1.0e300, 1e300], &params(1e6, -1e6, 10.0));
        // open at 1e6 or above; close below -1e6: -1e6 itself does not close it
        assert_eq!(out, [0.0, 1.0, 1.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn a_nan_input_closes_the_gate_where_it_is_without_a_panic() {
        // neither at or above open nor at or above close; the gate's own output stays 0 or 1
        assert_eq!(run(&[0.9, f64::NAN, 0.45, 0.9], &params(0.5, 0.4, 10.0)), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn no_inputs_give_no_outputs() {
        assert_eq!(run(&[], &params(0.5, 0.4, 10.0)), Vec::<f64>::new());
    }

    #[test]
    fn as_a_signal_a_jump_exactly_on_a_sample_is_seen_at_that_time() -> Result<(), Error> {
        let rate = 1000.0;
        // up at exactly 1.0 s, a sample of the default internal rate; down likewise
        let up: Vec<f64> = (0..20).map(|index| if index < 10 { 0.0 } else { 1.0 }).collect();
        let level = Signal::from_values(&up, 10.0, FromValues::default(), Interpolation::Hold)?;
        let gate = level.gate(params(0.5, 0.4, rate))?;
        assert_eq!(gate.sample(&[0.999, 1.0, 1.001])?, [0.0, 1.0, 1.0]);
        let falling = Signal::from_values(&up.iter().map(|v| 1.0 - v).collect::<Vec<_>>(), 10.0, FromValues::default(), Interpolation::Hold)?;
        assert_eq!(falling.gate(params(0.5, 0.4, rate))?.sample(&[0.999, 1.0, 1.001])?, [1.0, 0.0, 0.0]);
        Ok(())
    }

    #[test]
    fn a_lower_rate_sees_a_jump_between_its_samples_later() -> Result<(), Error> {
        // up at 0.05 s: between the samples of 10 Hz (0, 0.1, ...), a sample itself at 1000 Hz
        let values: Vec<f64> = (0..25).map(|index| if index < 5 { 0.0 } else { 1.0 }).collect();
        let level = Signal::from_values(&values, 100.0, FromValues::default(), Interpolation::Hold)?;
        assert_eq!(level.gate(params(0.5, 0.4, 10.0))?.sample(&[0.05, 0.1])?, [0.0, 1.0]);
        assert_eq!(level.gate(params(0.5, 0.4, 1000.0))?.sample(&[0.049, 0.05])?, [0.0, 1.0]);
        Ok(())
    }

    #[test]
    fn a_close_above_open_is_refused_naming_close() -> Result<(), Error> {
        let refused = Signal::from_values(&[0.5], 1.0, FromValues::default(), Interpolation::Hold)?.gate(params(0.5, 0.6, 1000.0));
        assert!(
            matches!(&refused, Err(Error::Params { shaper, problems, .. })
                if shaper == "gate" && problems.len() == 1 && problems[0].path == "close"),
            "{refused:?}"
        );
        Ok(())
    }

    /// Whether an output is exactly 1 (an output is exactly 0 or 1, by bits: the gate writes no other number).
    fn is_on(value: f64) -> bool {
        value.to_bits() == 1.0_f64.to_bits()
    }

    proptest! {
        #[test]
        fn the_output_is_two_valued_and_changes_state_only_where_the_input_crosses_the_level_of_that_direction(
            inputs in prop::collection::vec(-2.0_f64..=2.0, 1..60),
            open in -1.5_f64..=1.5,
            width in 0.0_f64..=1.5,
        ) {
            let close = open - width;
            let out = run(&inputs, &params(open, close, 10.0));
            prop_assert_eq!(out.len(), inputs.len());
            prop_assert!(out.iter().all(|value| value.to_bits() == 0.0_f64.to_bits() || is_on(*value)));
            prop_assert_eq!(is_on(out[0]), inputs[0] >= open);
            for (index, level) in inputs.iter().enumerate().skip(1) {
                let was_open = is_on(out[index - 1]);
                // it opens exactly when it was closed and the input is at or above open, closes exactly when it was
                // open and the input is below close, and otherwise keeps its state
                prop_assert_eq!(!was_open && *level >= open, !was_open && is_on(out[index]));
                prop_assert_eq!(was_open && *level < close, was_open && !is_on(out[index]));
            }
        }

        #[test]
        fn a_later_input_never_changes_an_earlier_output(
            inputs in prop::collection::vec(-2.0_f64..=2.0, 2..60),
            later in prop::collection::vec(-2.0_f64..=2.0, 0..20),
            open in -1.5_f64..=1.5,
            width in 0.0_f64..=1.5,
        ) {
            let range = params(open, open - width, 10.0);
            let mut longer = inputs.clone();
            longer.extend(later);
            prop_assert_eq!(&run(&longer, &range)[..inputs.len()], &run(&inputs, &range)[..]);
        }

        #[test]
        fn as_a_signal_it_gives_what_its_recurrence_gives_on_the_same_inputs(
            inputs in prop::collection::vec(-2.0_f64..=2.0, 1..40),
            open in -1.5_f64..=1.5,
            width in 0.0_f64..=1.5,
            rate in 10.0_f64..=48000.0,
        ) {
            let range = params(open, open - width, rate);
            let out = gated(&inputs, &range).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(out, run(&inputs, &range));
        }
    }
}
