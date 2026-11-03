//! A signal that steps through `values`, holding each for one step, and goes back to the first after the last.
//!
//! The position is rate * t, with t counted in seconds, or, if `per` is 'beat', in beats of `grid` (so it follows a
//! tempo change, and a time before the first beat or after the last counts beats of the first or last beat's
//! length). The value is the one at index floor(position) mod len(values); a step starts where that position reaches
//! a whole number, to within float rounding (of rate * t, and per beat of the grid's seconds-to-beats conversion).
//! `grid` is needed only per beat and ignored per second. A time too far from 0 for the position to be a number is a
//! `SignalError`.

use std::collections::BTreeMap;

use crate::cadence::{Per, positions};
use crate::formats::Grid;
use crate::inputs::{Input, InputKind, NO_VALUES, empty};
use crate::registry;
use crate::{Error, Signal, Source};

/// How fast the list of values is stepped through.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Steps per second, or per beat if `per` is 'beat'; each step holds one value. Raising it steps faster.
    #[param(default = 4.0, unit = Count, min = 0.001, max = 1000)]
    pub rate: f64,
    /// What `rate` counts in: 'second' (free-running) or 'beat' (follows the tempo of the `grid`, which it needs).
    #[param(default = Per::Second)]
    pub per: Per,
}

/// What `steps` steps through, and the beats it may follow.
#[derive(Clone, Debug, PartialEq)]
pub struct Inputs {
    /// The values, in the order they are stepped through: at least one.
    pub values: Vec<f64>,
    /// The beats `per` = 'beat' counts in; ignored per second.
    pub grid: Option<Grid>,
}

impl registry::Inputs for Inputs {
    const DECLARED: &'static [(InputKind, bool)] = &[(InputKind::Values, true), (InputKind::Grid, false)];

    fn from_inputs(given: &BTreeMap<String, Input>) -> Result<Self, Error> {
        Ok(Self { values: registry::Inputs::from_inputs(given)?, grid: registry::Inputs::from_inputs(given)? })
    }

    fn into_inputs(self) -> Result<Vec<Input>, Error> {
        Ok([self.values.into_inputs()?, self.grid.into_inputs()?].concat())
    }
}

impl Source for Params {
    const NAME: &'static str = "steps";
    type Inputs = Inputs;

    /// The signal stepping through `values` at `rate` steps per second or per beat of `grid`.
    ///
    /// ```
    /// use bullterrier_control::builtin::steps;
    /// use bullterrier_control::lib;
    ///
    /// // two steps a second: 0.25 s is in the first, 0.75 s in the second, 1.25 s back in the first
    /// let pattern = lib::steps(&[0.2, 0.9], steps::Params { rate: 2.0, ..Default::default() }, None)?;
    /// assert_eq!(pattern.sample(&[0.25, 0.75, 1.25])?, [0.2, 0.9, 0.2]);
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Input`] if there are no `values` (the door's refusal of them, here too when called directly);
    /// [`Error::Params`] naming `per` if `per` is 'beat' and there is no `grid`; a read is an [`Error::Signal`] at a
    /// time too far for its position to be a number.
    fn source(&self, Inputs { values, grid }: Inputs) -> Result<Signal, Error> {
        if values.is_empty() {
            return Err(empty(NO_VALUES));
        }
        let rate = self.rate;
        let per = self.per;
        // a counter that counts no times is refused as one that counts any: per beat without a grid, when made
        positions(rate, per, grid.as_ref(), &[])?;
        Ok(Signal::from_fn(move |times, out| {
            for (value, position) in out.iter_mut().zip(positions(rate, per, grid.as_ref(), times)?) {
                // in range: values is not empty, and a finite position's step lies below its length
                *value = values[step(position, values.len())];
            }
            Ok::<(), Error>(())
        }))
    }
}

/// The index of the step `position` (finite) falls in, going round `count` values: floor(position) mod `count`, in
/// floats until the last moment, so a position beyond 2^53 (a whole number) is exact, and a negative one (before a
/// first beat that is not at 0) counts back from the end of the list.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "a count of values is far below 2^53, and a whole number mod it is a whole number from 0 below it"
)]
fn step(position: f64, count: usize) -> usize {
    position.floor().rem_euclid(count as f64) as usize
}
