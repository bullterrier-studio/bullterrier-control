//! Where a read falls in a periodic source: its position, counted in cycles or steps per second or per beat.
//!
//! ```
//! use bullterrier_control::cadence::{Per, positions};
//! use bullterrier_control::formats::Grid;
//!
//! // beats 0.5 s, then 0.6 s, then 0.7 s long
//! let grid = Grid::new(vec![0.0, 0.5, 1.1, 1.8], 4, 0)?;
//! // two cycles per beat: halfway through each beat is a whole cycle further
//! assert_eq!(positions(2.0, Per::Beat, Some(&grid), &[0.25, 0.8, 1.45])?, [1.0, 3.0, 5.0]);
//! // three per second
//! assert_eq!(positions(3.0, Per::Second, None, &[0.0, 0.5, 2.0])?, [0.0, 1.5, 6.0]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Promises:
//! - `positions` turns times (finite, from 0 up) into positions: `rate * t` per second, `rate * grid.beat_at(t)` per
//!   beat, so a source follows a tempo change. Every position is finite; its whole part says which cycle or step, its
//!   fraction where in it. It is negative for a time before a first beat that is not at 0. A step or cycle edge sits
//!   where `rate * t` reaches a whole number, to within float rounding: a time that is a multiple of `1 / rate` in
//!   decimal (frame times at rate 30) may read the step before.
//! - [`Per::Beat`] without a grid is an [`Error::Params`] naming `per`; a grid is ignored per second.
//! - A time too far for the position to be a number (`rate * t` overflows, or the grid cannot reach it) is an
//!   [`Error::Signal`].

use crate::formats::{Grid, Problem};
use crate::{Choice, Error};

/// What a source counts its cycles or steps in: the choice of its parameter `per` ('second' or 'beat').
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
#[non_exhaustive]
pub enum Per {
    /// Seconds: the rate is cycles or steps per second.
    Second,
    /// Beats of a [`Grid`]: the rate is cycles or steps per beat.
    Beat,
}

/// The position of each of `times` (seconds): `rate` cycles or steps per second or per beat of `grid`.
///
/// ```
/// use bullterrier_control::cadence::{Per, positions};
///
/// assert_eq!(positions(0.0, Per::Second, None, &[0.0, 9.0])?, [0.0, 0.0]); // rate 0 never moves
/// let refused = positions(1.0, Per::Beat, None, &[0.0]).map_err(|error| error.to_string());
/// assert!(matches!(refused, Err(text) if text.contains("per: 'beat' needs a grid")));
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
///
/// # Errors
/// [`Error::Params`] (its field `per`) if `per` is [`Per::Beat`] and there is no `grid`; [`Error::Signal`], naming the
/// time, if a time is too far from 0 for its position to be a finite number (the grid cannot reach it, or `rate * t`
/// overflows) or is not finite itself.
pub fn positions(rate: f64, per: Per, grid: Option<&Grid>, times: &[f64]) -> Result<Vec<f64>, Error> {
    let amounts = match (per, grid) {
        (Per::Second, _) => times.to_vec(),
        (Per::Beat, Some(grid)) => grid.beat_at(times).map_err(|error| Error::Signal {
            message: error.to_string(),
        })?,
        (Per::Beat, None) => {
            return Err(Error::Params {
                shaper: "cadence".to_owned(),
                problems: vec![Problem {
                    path: "per".to_owned(),
                    rule: "'beat' needs a grid".to_owned(),
                }],
                omitted: 0,
            });
        }
    };
    let counted: Vec<f64> = amounts.iter().map(|amount| rate * amount).collect();
    match counted.iter().position(|position| !position.is_finite()) {
        None => Ok(counted),
        Some(index) => Err(Error::Signal {
            message: format!(
                "the position is not a finite number: the time {:e} is too far from 0",
                times.get(index).copied().unwrap_or(f64::NAN)
            ),
        }),
    }
}
