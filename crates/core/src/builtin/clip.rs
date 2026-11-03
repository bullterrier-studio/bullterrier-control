//! A signal that follows its input but never leaves the range from `low` to `high`.
//!
//! Lower values are raised to `low`, higher ones lowered to `high`, and what lies inside stays exactly as it is. The
//! clip applies to the signal, not to its samples: between two samples that cross the range it is cut where the line
//! crosses.

use crate::formats::Problem;
use crate::{Error, Shaper, Signal, maximum, minimum};

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
        Ok(minimum(&maximum(input, &Signal::constant(self.low)?), &Signal::constant(self.high)?))
    }
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
