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
