//! A signal that shows its input `lag` seconds late.
//!
//! The value at time t is the input at t - lag; until `lag` seconds have passed that is the input at time 0 (a signal
//! has no values before 0). A signal can only lag: a lead would read the input's future, and exists for events only.

use std::convert::Infallible;

use crate::{Error, Shaper, Signal};

/// How far the output trails its input.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Seconds the output trails its input, 0 to 3600; raising it shows the input later. 0 passes it through.
    #[param(default = 0.1, unit = Seconds, min = 0, max = 3600)]
    pub lag: f64,
}

impl Signal {
    /// This signal `params.lag` seconds late: the shaper [`delay`](super), as [`pipe`](Signal::pipe) of `params` makes it
    /// (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::builtin::delay;
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// let ramp = Signal::from_values(&[0.0, 1.0], 1.0, FromValues::default(), Interpolation::Linear)?;
    /// let late = ramp.delay(delay::Params { lag: 0.25 })?;
    /// assert_eq!(late.sample(&[0.1, 0.25, 0.75])?, [0.0, 0.0, 0.5]); // the input at 0 until the lag has passed
    /// assert!(ramp.delay(delay::Params { lag: -1.0 }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe`]: [`Error::Params`] under the name `delay` if `lag` is out of its range or not finite.
    pub fn delay(&self, params: Params) -> Result<Signal, Error> {
        self.pipe(params)
    }
}

impl Shaper for Params {
    const NAME: &'static str = "delay";
    type Inputs = ();

    /// `input` read at `max(t - lag, 0)`.
    fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> {
        let lag = self.lag;
        input.retimed(move |times| {
            // not f64::max: a -0.0 (a read at -0.0 with no lag) is 0.0, as numpy's maximum gives it
            Ok::<_, Infallible>(times.iter().map(|time| if time - lag > 0.0 { time - lag } else { 0.0 }).collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;
    use crate::registry::{Call, Output};
    use crate::{FromValues, Interpolation, MAX_DEPTH, Registry, builtin::delay};

    /// 2 at 0 s, 3 at 1 s, straight between, 3 after: not 0 at time 0, so a read before the lag is not 0 by accident.
    fn ramp() -> Result<Signal, Error> {
        Signal::from_values(&[2.0, 3.0], 1.0, FromValues::default(), Interpolation::Linear)
    }

    #[test]
    fn the_output_is_the_input_a_lag_earlier_and_the_input_at_0_before_the_lag() -> Result<(), Error> {
        let late = ramp()?.delay(delay::Params { lag: 0.25 })?;
        // (time, expected): t - 0.25 on the ramp is 2 + (t - 0.25) up to 1 s
        let rows = [
            (0.0, 2.0),  // at 0: the input at 0
            (0.1, 2.0),  // before the lag: the input at 0
            (0.25, 2.0), // at the lag: the input at 0
            (0.75, 2.5), // 0.75 - 0.25 = 0.5: halfway up
            (1.25, 3.0), // the input at 1
            (2.0, 3.0),  // the input holds its last value
        ];
        let times: Vec<f64> = rows.iter().map(|(time, _)| *time).collect();
        let expected: Vec<f64> = rows.iter().map(|(_, value)| *value).collect();
        assert_eq!(late.sample(&times)?, expected);
        Ok(())
    }

    #[test]
    fn by_default_it_lags_by_a_tenth_of_a_second() -> Result<(), Error> {
        // 0.6 - 0.1 = 0.5 (in floats 0.49999999999999994): the ramp is 2.5 there
        let late = ramp()?.delay(delay::Params::default())?.sample(&[0.05, 0.1, 0.6, 1.1])?;
        let expected = [2.0, 2.0, 2.5, 3.0];
        assert!(late.iter().zip(expected).all(|(got, want)| (got - want).abs() <= 1e-12), "{late:?}");
        Ok(())
    }

    #[test]
    fn through_the_registry_it_lags_and_a_lag_below_0_is_refused_naming_it() -> Result<(), Error> {
        let thing = Registry::builtin().get("delay")?;
        let input = ramp()?;
        let call = |lag: f64| Call {
            signals: vec![input.clone()],
            values: json!({ "lag": lag }),
            ..Call::default()
        };
        let Output::Signal(late) = thing.call(&call(0.5))? else {
            return Err(Error::Call { message: "not a signal".into() });
        };
        assert_eq!(late.sample(&[0.5, 1.0])?, [2.0, 2.5]);
        assert_eq!(late.recipe().map(|recipe| (recipe.name.as_str(), recipe.values.clone())), Some(("delay", json!({"lag": 0.5}))));
        for lag in [-0.1, 3600.5] {
            let refused = thing.call(&call(lag));
            assert!(
                matches!(&refused, Err(Error::Params { shaper, problems, .. })
                    if shaper == "delay" && problems.len() == 1 && problems[0].path == "lag"),
                "{lag}: {refused:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn the_deepest_chain_of_delays_reads_on_the_smallest_thread_stack() {
        let walked = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| -> Result<Vec<f64>, Error> {
                let mut chained = ramp()?;
                for _ in 1..MAX_DEPTH {
                    chained = chained.delay(delay::Params { lag: 0.001 })?;
                }
                // 999 lags of 0.001: the input at 1.999 - 0.999 = 1 s, and at 0 s for a read before 0.999 s
                let values = chained.sample(&[0.5, 1.999, 5.0])?;
                drop(chained);
                Ok(values)
            })
            .map(std::thread::JoinHandle::join);
        assert!(
            matches!(&walked, Ok(Ok(Ok(values)))
                if values.iter().zip([2.0, 3.0, 3.0]).all(|(got, want)| (got - want).abs() <= 1e-9)),
            "{walked:?}"
        );
    }

    proptest! {
        #[test]
        fn a_lag_of_0_passes_the_input_through_exactly(
            values in prop::collection::vec(-5.0_f64..=5.0, 1..20),
            times in prop::collection::vec(0.0_f64..=30.0, 0..30),
        ) {
            let input = Signal::from_values(&values, 1.0, FromValues::default(), Interpolation::Linear)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let passed = input.delay(delay::Params { lag: 0.0 }).and_then(|late| late.sample(&times));
            prop_assert_eq!(passed.map_err(|error| error.to_string()), input.sample(&times).map_err(|error| error.to_string()));
        }
    }
}
