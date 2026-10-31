//! A signal that follows its input but climbs and falls no faster than `rise` and `fall` allow.
//!
//! It starts at the input's value at time 0. A limit is a time per unit of change: rise = 2 means at most 0.5 per
//! second.

use std::convert::Infallible;

use crate::{Error, Rendering, Shaper, Signal};

/// How fast the output may climb and fall.
#[derive(Clone, Copy, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Seconds the output takes to climb by 1; larger is slower. 0 follows a rise at once.
    #[param(default = 0.1, unit = Seconds, min = 0, max = 3600)]
    pub rise: f64,
    /// Seconds the output takes to fall by 1; larger is slower. 0 follows a fall at once.
    #[param(default = 0.1, unit = Seconds, min = 0, max = 3600)]
    pub fall: f64,
    /// Internal samples per second; the output is exact to one sample. Raising it is more exact and slower.
    #[param(default = 1000.0, unit = Hertz, min = 10, max = 48000, advanced)]
    pub rate: f64,
}

impl Signal {
    /// This signal, climbing and falling no faster than `params` allow: the shaper [`slew`](super), as
    /// [`pipe`](Signal::pipe) of `params` makes it (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::builtin::slew;
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// let step = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
    /// // 0.1 s a sample, 0.5 s to climb by 1: 0.2 a sample
    /// let ramp = step.slew(slew::Params { rise: 0.5, fall: 0.5, rate: 10.0 })?;
    /// assert_eq!(ramp.sample(&[0.0, 0.1, 0.2])?, [0.0, 0.2, 0.4]);
    /// assert!(step.slew(slew::Params { rise: -1.0, ..Default::default() }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe`]: [`Error::Params`] under the name `slew` listing every rule `params` breaks (a field out
    /// of its range or not finite).
    pub fn slew(&self, params: Params) -> Result<Signal, Error> {
        self.pipe(params)
    }
}

impl Shaper for Params {
    const NAME: &'static str = "slew";

    /// Its causal recurrence over `input` sampled at `rate`, rendered as far as the reads need.
    fn shape(&self, input: &Signal) -> Result<Signal, Error> {
        let rendering = Rendering {
            rate: self.rate,
            ..Rendering::default()
        };
        let params = *self;
        Signal::rendered(input, rendering, move |inputs| Ok::<_, Infallible>(run(inputs, &params)))
    }
}

/// The slew-limited series: it starts at the first input and moves toward each next one by at most a sample's limit
/// (a sample is `1 / rate` seconds, the limit that over `rise` up, over `fall` down; a time of 0 has no limit).
fn run(inputs: &[f64], params: &Params) -> Vec<f64> {
    let step = 1.0 / params.rate;
    let up = if params.rise > 0.0 { step / params.rise } else { f64::INFINITY };
    let down = if params.fall > 0.0 { step / params.fall } else { f64::INFINITY };
    let Some(&first) = inputs.first() else {
        return Vec::new();
    };
    let mut level = first;
    inputs
        .iter()
        .map(|&target| {
            // the target unless the limit stops short of it (not f64::min/max, which skip a NaN: a NaN input comes
            // through, for `sample` to report)
            level = if target > level {
                let limit = level + up;
                if limit < target { limit } else { target }
            } else {
                let limit = level - down;
                if limit > target { limit } else { target }
            };
            level
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FromValues, Interpolation};
    use proptest::prelude::*;

    /// The shaper over `inputs` (a series at `params.rate`, holding each value), read at each input's time.
    fn shaped(inputs: &[f64], params: &Params) -> Result<Vec<f64>, Error> {
        let input = Signal::from_values(inputs, params.rate, FromValues::default(), Interpolation::Hold)?;
        let times: Vec<f64> = (0..inputs.len()).map(|index| f64::from(u32::try_from(index).unwrap_or(0)) / params.rate).collect();
        params.shape(&input)?.sample(&times)
    }

    fn params(rise: f64, fall: f64, rate: f64) -> Params {
        Params { rise, fall, rate }
    }

    fn close(out: &[f64], expected: &[f64]) -> bool {
        out.len() == expected.len() && out.iter().zip(expected).all(|(a, b)| (a - b).abs() <= 1e-12)
    }

    #[test]
    fn a_step_up_climbs_by_one_step_s_limit_a_sample_until_it_arrives() {
        // rate 10: a sample is 0.1 s; rise 0.5 s to climb by 1: 0.1 / 0.5 = 0.2 a sample
        let out = run(&[0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0], &params(0.5, 0.5, 10.0));
        assert!(close(&out, &[0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.0]), "{out:?}");
    }

    #[test]
    fn no_inputs_give_no_outputs() {
        assert_eq!(run(&[], &params(0.5, 0.5, 10.0)), Vec::<f64>::new());
    }

    #[test]
    fn a_nan_input_comes_through_where_it_is_without_a_panic() {
        // a broken upstream signal: the NaN reaches the output at its sample, where `sample` reports it
        let out = run(&[0.0, f64::NAN, 1.0], &params(0.5, 0.5, 10.0));
        assert_eq!(out.len(), 3);
        assert!(out[0].abs() <= 1e-12 && out[1].is_nan(), "{out:?}");
    }

    #[test]
    fn as_a_signal_a_step_up_climbs_by_one_step_s_limit_a_sample_until_it_arrives() -> Result<(), Error> {
        let out = shaped(&[0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0], &params(0.5, 0.5, 10.0))?;
        assert!(close(&out, &[0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.0]), "{out:?}");
        Ok(())
    }

    #[test]
    fn as_a_signal_a_nan_input_makes_a_read_from_there_a_signal_error() -> Result<(), Error> {
        let broken = Signal::from_fn(|times, out| {
            for (value, time) in out.iter_mut().zip(times) {
                *value = if *time < 0.1 { 1.0 } else { f64::NAN };
            }
            Ok::<(), std::fmt::Error>(())
        });
        let slewed = params(0.5, 0.5, 10.0).shape(&broken)?;
        assert_eq!(slewed.sample(&[0.0, 0.05])?, [1.0, 1.0]);
        let refused = slewed.at(0.1);
        assert!(
            matches!(&refused, Err(Error::Signal { message }) if message.contains("not finite")),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_signal_slews_with_typed_parameters_checked_as_values_by_name_are() -> Result<(), Error> {
        let step = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
        let out = step.slew(params(0.5, 0.25, 10.0))?.sample(&[0.0, 0.1, 0.2, 0.3])?;
        assert!(close(&out, &[0.0, 0.2, 0.4, 0.6]), "{out:?}");
        let refused = step.slew(params(-1.0, 0.5, 5.0));
        let named: Vec<String> = match &refused {
            Err(Error::Params { shaper, problems, .. }) if shaper == "slew" => {
                problems.iter().map(|problem| problem.path.clone()).collect()
            }
            other => vec![format!("not slew's parameter error: {other:?}")],
        };
        assert_eq!(named, ["rise", "rate"]);
        Ok(())
    }

    proptest! {
        #[test]
        fn as_a_signal_it_gives_what_its_recurrence_gives_on_the_same_inputs(
            inputs in prop::collection::vec(-5.0_f64..=5.0, 1..40),
            rise in 0.0_f64..=3.0,
            fall in 0.0_f64..=3.0,
            rate in 10.0_f64..=48000.0,
        ) {
            let params = params(rise, fall, rate);
            let out = shaped(&inputs, &params).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(out, run(&inputs, &params));
        }

        #[test]
        fn the_output_never_moves_faster_than_its_limits_and_never_overshoots_its_input(
            inputs in prop::collection::vec(-5.0_f64..=5.0, 1..40),
            rise in 0.01_f64..=3.0,
            fall in 0.01_f64..=3.0,
            rate in 10.0_f64..=48000.0,
        ) {
            let out = run(&inputs, &params(rise, fall, rate));
            prop_assert_eq!(out.len(), inputs.len());
            for pair in out.windows(2) {
                let moved = pair[1] - pair[0];
                prop_assert!(moved <= 1.0 / (rate * rise) + 1e-9, "{moved} up");
                prop_assert!(moved >= -1.0 / (rate * fall) - 1e-9, "{moved} down");
            }
            let (mut lowest, mut highest) = (f64::INFINITY, f64::NEG_INFINITY);
            for (value, target) in out.iter().zip(&inputs) {
                (lowest, highest) = (lowest.min(*target), highest.max(*target));
                prop_assert!(*value >= lowest - 1e-9 && *value <= highest + 1e-9, "{value} beyond {lowest}..{highest}");
            }
        }
    }
}
