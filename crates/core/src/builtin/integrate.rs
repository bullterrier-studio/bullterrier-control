//! A signal that adds up its input over time: the area under it so far.
//!
//! The output starts at 0 and grows by the input times the time passed, so a speed in cycles a second integrates to a
//! position in cycles (a phase that turns faster while the speed is high), and a constant `c` gives `c` times the time.
//! The input is added up in steps of `1 / rate` seconds with the trapezoid rule (each step adds the mean of its two
//! ends), which is exact for an input that changes in straight lines between the steps. The output is a staircase of
//! those steps. A read that needs more steps than a read may hold is an error, as for every shaper with a history.

use crate::{Error, Shaper, Signal, recurrence};

/// How finely the input is added up.
#[derive(Clone, Copy, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Internal samples per second; the output is exact to one sample. Raising it is more exact and slower.
    #[param(default = 1000.0, unit = Hertz, min = 10, max = 48000, advanced)]
    pub rate: f64,
}

impl Signal {
    /// This signal added up over time: the shaper [`integrate`](super), as [`pipe`](Signal::pipe) of `params` makes it
    /// (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::builtin::integrate;
    /// use bullterrier_control::Signal;
    ///
    /// // a speed of 2 a second: 1 after half a second, 4 after two
    /// let speed = Signal::constant(2.0)?;
    /// let position = speed.integrate(integrate::Params { rate: 10.0 })?.sample(&[0.5, 2.0])?;
    /// assert!(position.iter().zip([1.0, 4.0]).all(|(got, want)| (got - want).abs() < 1e-9));
    /// assert!(speed.integrate(integrate::Params { rate: 1.0 }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe`]: [`Error::Params`] under the name `integrate` listing every rule `params` breaks (a field out
    /// of its range or not finite).
    pub fn integrate(&self, params: Params) -> Result<Signal, Error> {
        self.pipe(params)
    }
}

impl Shaper for Params {
    const NAME: &'static str = "integrate";
    type Inputs = ();

    /// The running area under `input`, in steps of `1 / rate` seconds.
    fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> {
        recurrence::over(input, self.rate, *self, run)
    }
}

/// The trapezoid sums of `inputs`: 0 first, then each output the one before plus the mean of the two inputs it lies
/// between times a sample's length. Output *i* depends on inputs 0 to *i* only. A NaN input makes every later output
/// NaN, for `sample` to report.
fn run(inputs: &[f64], params: &Params) -> Vec<f64> {
    let step = 1.0 / params.rate;
    let mut area = 0.0;
    let mut before: Option<f64> = None;
    inputs
        .iter()
        .map(|&now| {
            if let Some(then) = before {
                area += 0.5 * (then + now) * step;
            }
            before = Some(now);
            area
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    /// A signal that reads as its time.
    fn ramp() -> Signal {
        Signal::from_fn(|times, out| {
            out.copy_from_slice(times);
            Ok::<(), std::fmt::Error>(())
        })
    }

    #[test]
    fn a_constant_adds_up_to_its_value_times_the_time() {
        // 3 at 2 samples a second: steps of 0.5 s, so 1.5 a step
        assert_eq!(run(&[3.0; 5], &Params { rate: 2.0 }), [0.0, 1.5, 3.0, 4.5, 6.0]);
    }

    #[test]
    fn a_ramp_adds_up_to_half_the_time_squared_exactly() {
        // the input is its time at 4 samples a second: 0, 0.25, ... 1; the area under it up to t is t^2 / 2, and the
        // trapezoid rule is exact for straight lines (every number here is a binary fraction)
        let times = [0.0, 0.25, 0.5, 0.75, 1.0];
        assert_eq!(run(&times, &Params { rate: 4.0 }), [0.0, 0.03125, 0.125, 0.28125, 0.5]);
    }

    #[test]
    fn nothing_adds_up_to_nothing_and_a_nan_poisons_what_follows_it() {
        assert!(run(&[], &Params::default()).is_empty());
        let out = run(&[1.0, f64::NAN, 1.0], &Params { rate: 1000.0 });
        assert!(out[0] == 0.0 && out[1].is_nan() && out[2].is_nan(), "{out:?}");
    }

    #[test]
    fn the_core_runs_it_by_name_with_its_parameters_checked() -> Result<(), Error> {
        // the input is its time: 0.5 s is 5 samples at 10 a second, the area under it t^2 / 2
        let out = crate::shape("integrate", &ramp(), &json!({"rate": 10.0}))?.sample(&[0.5, 1.0])?;
        assert!(out.iter().zip([0.125, 0.5]).all(|(got, want)| (got - want).abs() < 1e-12), "{out:?}");
        let refused = crate::shape("integrate", &ramp(), &json!({"rate": 1.0}));
        assert!(matches!(&refused, Err(Error::Params { shaper, .. }) if shaper == "integrate"), "{refused:?}");
        Ok(())
    }

    proptest! {
        #[test]
        fn each_step_adds_the_mean_of_its_ends_and_a_longer_input_leaves_the_earlier_outputs_alone(
            inputs in prop::collection::vec(-100.0_f64..=100.0, 2..40),
            extra in prop::collection::vec(-100.0_f64..=100.0, 0..10),
            rate in 10.0_f64..=1000.0,
        ) {
            let params = Params { rate };
            let out = run(&inputs, &params);
            prop_assert_eq!(out[0], 0.0);
            for index in 1..inputs.len() {
                let mean = (inputs[index - 1] + inputs[index]) / 2.0;
                prop_assert!(((out[index] - out[index - 1]) * rate - mean).abs() <= 1e-9 * (1.0 + out[index].abs() * rate), "{index}");
            }
            let longer: Vec<f64> = inputs.iter().chain(&extra).copied().collect();
            prop_assert_eq!(&run(&longer, &params)[..inputs.len()], &out[..]);
        }
    }
}
