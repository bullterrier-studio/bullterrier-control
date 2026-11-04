//! A signal that plays the first `length` of its input again and again, or forward and backward.
//!
//! The value at time t is the input at t folded into 0 to `length`: 'wrap' restarts the input every `length`,
//! 'ping-pong' runs it forward, then backward, then forward again. `length` counts seconds, or beats of the `grid` if
//! `per` is 'beat', so the repeat follows a tempo change. Counting beats starts at the beat position of time 0, so a
//! read never uses the input's future: the folded time is never later than the time read.

use crate::cadence::{Per, positions};
use crate::formats::{FormatError, Grid};
use crate::{Choice, Error, Shaper, Signal};

/// How the input is played again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Mode {
    /// The choice `wrap`.
    Wrap,
    /// The choice `ping-pong`.
    #[choice(name = "ping-pong")]
    PingPong,
}

/// The part of the input that repeats, and how.
#[derive(Clone, Copy, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// How the input is played again: 'wrap' (restarts after `length`) or 'ping-pong' (forward, then backward).
    #[param(default = Mode::Wrap)]
    pub mode: Mode,
    /// How much of the input repeats: seconds, or beats if `per` is 'beat', 0.001 to 3600. Raising it repeats a longer
    /// stretch, so the repeat is slower.
    #[param(default = 16.0, unit = Seconds, min = 0.001, max = 3600)]
    pub length: f64,
    /// What `length` counts in: 'second' (free-running) or 'beat' (follows the tempo of the `grid`, which it needs).
    #[param(default = Per::Second)]
    pub per: Per,
}

impl Signal {
    /// This signal repeated as `params` say: the shaper [`repeat`](super), as [`pipe_with`](Signal::pipe_with) of
    /// `params` and `grid` makes it (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::builtin::repeat;
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// // 0 at 0 s up to 1 at 1 s
    /// let ramp = Signal::from_values(&[0.0, 1.0], 1.0, FromValues::default(), Interpolation::Linear)?;
    /// let params = repeat::Params { length: 0.5, ..Default::default() };
    /// // the ramp of the first half second, again from 0.5 s
    /// assert_eq!(ramp.repeat(params, None)?.sample(&[0.25, 0.5, 0.75])?, [0.25, 0.0, 0.25]);
    /// let back = repeat::Params { length: 0.5, mode: repeat::Mode::PingPong, ..Default::default() };
    /// // forward to 0.5 s, then backward
    /// assert_eq!(ramp.repeat(back, None)?.sample(&[0.25, 0.75, 1.0])?, [0.25, 0.25, 0.0]);
    /// assert!(ramp.repeat(repeat::Params { length: 0.0, ..Default::default() }, None).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe_with`]: [`Error::Params`] under the name `repeat` listing every rule `params` breaks, and one
    /// naming `per` if `per` is 'beat' and there is no `grid`.
    pub fn repeat(&self, params: Params, grid: Option<&Grid>) -> Result<Signal, Error> {
        self.pipe_with(params, grid.cloned())
    }
}

/// `position` (from 0) folded into 0 to `length`.
fn fold(position: f64, length: f64, mode: Mode) -> f64 {
    match mode {
        Mode::Wrap => position % length,
        Mode::PingPong => {
            let turn = position % (2.0 * length);
            if turn > length { 2.0 * length - turn } else { turn }
        }
    }
}

impl Shaper for Params {
    const NAME: &'static str = "repeat";
    type Inputs = Option<Grid>;

    /// `input` read at each time folded into the first `length` seconds or beats.
    fn shape(&self, input: &Signal, grid: Option<Grid>) -> Result<Signal, Error> {
        // a fold that counts no times is refused as one that counts any: per beat without a grid, when made
        positions(1.0, self.per, grid.as_ref(), &[])?;
        let Params { mode, length, per } = *self;
        input.retimed(move |times| match (per, grid.as_ref()) {
            (Per::Beat, Some(grid)) => {
                let signal = |error: FormatError| Error::Signal { message: error.to_string() };
                // beats are counted from where time 0 falls, so that the fold of a later time is never later than it
                let origin = grid.beat_at(&[0.0]).map_err(signal)?[0];
                let positions: Vec<f64> = grid.beat_at(times).map_err(signal)?.iter().map(|beat| beat - origin).collect();
                let folded: Vec<f64> = positions.iter().map(|position| origin + fold(*position, length, mode)).collect();
                let back = grid.seconds_at(&folded).map_err(signal)?;
                Ok(times
                    .iter()
                    .zip(&positions)
                    .zip(back)
                    // a time inside the first stretch is its own: it needs no trip through beats and back
                    // (not f64::max below: a -0.0 is 0.0, as in delay)
                    .map(|((time, position), back)| if *position < length { *time } else if back > 0.0 { back } else { 0.0 })
                    .collect())
            }
            _ => Ok::<_, Error>(times.iter().map(|time| fold(*time, length, mode)).collect()),
        })
    }
}
