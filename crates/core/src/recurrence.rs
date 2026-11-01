//! The shapers that are a causal recurrence over their input sampled at a rate of their own (`slew`, `follow`,
//! `gate`): how one is made into a signal, in one place.

use std::convert::Infallible;

use crate::{Error, Rendering, Signal};

/// `input` sampled at `rate` samples per second from time 0 and run through `run(inputs, &params)`, which gives one
/// output per input, output *i* depending on inputs 0 to *i* only (the signal contract's causality). The result is
/// rendered as far as the reads need, with the default limit of [`Rendering`].
///
/// # Errors
/// As [`Signal::rendered`]: [`Error::Signal`] if `rate` is not positive and finite.
pub(crate) fn over<P>(input: &Signal, rate: f64, params: P, run: fn(&[f64], &P) -> Vec<f64>) -> Result<Signal, Error>
where
    P: Send + Sync + 'static,
{
    let rendering = Rendering {
        rate,
        ..Rendering::default()
    };
    Signal::rendered(input, rendering, move |inputs| {
        Ok::<_, Infallible>(run(inputs, &params))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FromValues, Interpolation};

    /// The running sum: output *i* is inputs 0 to *i* added up, times `scale`, plus `bias`.
    fn running_sum(inputs: &[f64], (scale, bias): &(f64, f64)) -> Vec<f64> {
        let mut total = 0.0;
        inputs
            .iter()
            .map(|value| {
                total += value;
                total * scale + bias
            })
            .collect()
    }

    #[test]
    fn the_recurrence_runs_over_the_input_sampled_at_the_rate_with_the_params() -> Result<(), Error> {
        // 1, 2, 3, 4 a tenth of a second apart; the sum 1, 3, 6, 10, times 2, plus a half
        let input = Signal::from_values(&[1.0, 2.0, 3.0, 4.0], 10.0, FromValues::default(), Interpolation::Hold)?;
        let summed = over(&input, 10.0, (2.0, 0.5), running_sum)?;
        assert_eq!(summed.sample(&[0.0, 0.1, 0.2, 0.3])?, [2.5, 6.5, 12.5, 20.5]);
        // sampled at 20 Hz the input holds each value for two samples: 1, 1, 2, 2, 3, 3
        let finer = over(&input, 20.0, (1.0, 0.0), running_sum)?;
        assert_eq!(
            finer.sample(&[0.0, 0.05, 0.1, 0.15, 0.2, 0.25])?,
            [1.0, 2.0, 4.0, 6.0, 9.0, 12.0]
        );
        Ok(())
    }

    #[test]
    fn a_rate_that_is_not_positive_and_finite_is_refused() -> Result<(), Error> {
        let input = Signal::from_values(&[1.0], 1.0, FromValues::default(), Interpolation::Hold)?;
        for rate in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let refused = over(&input, rate, (1.0, 0.0), running_sum);
            assert!(
                matches!(&refused, Err(Error::Signal { message }) if message.contains("rate")),
                "{rate}: {refused:?}"
            );
        }
        Ok(())
    }
}
