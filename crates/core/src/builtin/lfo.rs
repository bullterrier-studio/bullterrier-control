//! A periodic wave between `low` and `high`, repeating `rate` times per second or per beat.
//!
//! At time t the position in the cycle is x = (rate * t + phase) mod 1, with t counted in seconds, or, if `per` is
//! 'beat', in beats of `grid` (so it follows a tempo change, and a time before the first beat or after the last
//! counts beats of the first or last beat's length). The wave on 0..1 is: sine 0.5 + 0.5 sin(2 pi x); triangle
//! 1 - |2x - 1|; square 1 while x < duty, else 0; saw x; saw-down 1 - x. The value is low + (high - low) times the
//! wave, and never leaves the range between `low` and `high`. `grid` is needed only per beat and ignored per second.
//! A time too far from 0 for the position to be a number is a `SignalError`.

use std::f64::consts::PI;

use crate::cadence::{Per, positions};
use crate::formats::Grid;
use crate::scaling::between;
use crate::{Choice, Error, Signal, Source};

/// The choices of `shape`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Shape {
    /// The choice `sine`.
    Sine,
    /// The choice `triangle`.
    Triangle,
    /// The choice `square`.
    Square,
    /// The choice `saw`.
    Saw,
    /// The choice `saw-down`.
    #[choice(name = "saw-down")]
    SawDown,
}

/// The wave: its shape, how fast it repeats and where in the cycle it starts, and the range it moves in.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The wave: 'sine' (smooth), 'triangle' (straight up and down), 'square' (high, then low), 'saw' (rises, then
    /// jumps back) or 'saw-down' (falls, then jumps back).
    #[param(default = Shape::Sine)]
    pub shape: Shape,
    /// Cycles per second, or per beat if `per` is 'beat'; 0 holds the wave still at `phase`. Raising it makes the
    /// wave faster.
    #[param(default = 1.0, unit = Cycles, min = 0, max = 1000)]
    pub rate: f64,
    /// What `rate` counts in: 'second' (free-running) or 'beat' (follows the tempo of the `grid`, which it needs).
    #[param(default = Per::Second)]
    pub per: Per,
    /// Where in the cycle time 0 falls, as a share of a cycle, 0 to 1: 0.25 starts a quarter of the way in. Any
    /// number is accepted and wraps into 0 to 1 (1.25 is 0.25), so a phase that keeps turning needs no wrap.
    #[param(default = 0.0, unit = Cycles, min = 0, max = 1, cyclic)]
    pub phase: f64,
    /// The value at the bottom of the wave; set above `high` to flip the wave upside down.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub low: f64,
    /// The value at the top of the wave.
    #[param(default = 1.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub high: f64,
    /// Square only: the share of each cycle spent high, 0 to 1; 0 is never high, 1 always.
    #[param(default = 0.5, unit = Fraction, min = 0, max = 1)]
    pub duty: f64,
}

impl Params {
    /// The value at `position` (cycles counted from 0, finite).
    #[expect(clippy::float_cmp, reason = "exactly 1.0, the one value mod 1 gives that is not below 1")]
    fn value_at(&self, position: f64) -> f64 {
        // a phase of 1 is the phase of 0: reduced first, so adding a whole cycle cannot move a time across a cycle edge
        let cycle = mod_one(position + mod_one(self.phase));
        // mod 1 of a tiny negative number rounds to exactly 1.0, which is the end of a cycle, not its start: one step
        // below it, where a square with duty 1 is still high and a saw is at the top of its ramp
        let cycle = if cycle == 1.0 { cycle.next_down() } else { cycle };
        between(self.low, self.high, self.shape.wave(cycle, self.duty))
    }
}

impl Shape {
    /// The wave on 0..1 at `cycle` (in [0, 1)), `duty` the share of a square's cycle spent high.
    fn wave(self, cycle: f64, duty: f64) -> f64 {
        match self {
            // 2 pi (`PI`, the float nearest pi, doubled exactly): one full turn per cycle
            Self::Sine => 0.5 + 0.5 * libm::sin(2.0 * PI * cycle),
            Self::Triangle => 1.0 - (2.0 * cycle - 1.0).abs(),
            Self::Square => {
                if cycle < duty {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Saw => cycle,
            Self::SawDown => 1.0 - cycle,
        }
    }
}

/// `value` mod 1 as Python's float `%` and numpy's `np.mod` give it: from 0 up to 1, +0.0 for a whole number (-0.0
/// included), a negative value counted back from 1 (-0.25 is 0.75), and NaN for NaN or an infinity. The rest of
/// `%` (fmod) is exact; adding 1 to a negative one rounds, so a tiny negative value gives exactly 1.0.
fn mod_one(value: f64) -> f64 {
    let rest = value % 1.0;
    if rest == 0.0 {
        0.0
    } else if rest < 0.0 {
        rest + 1.0
    } else {
        rest
    }
}

impl Source for Params {
    const NAME: &'static str = "lfo";
    type Inputs = Option<Grid>;

    /// The wave of these parameters, counted per second or per beat of `grid`.
    ///
    /// ```
    /// use bullterrier_control::builtin::lfo;
    /// use bullterrier_control::lib;
    ///
    /// // a saw from -1 to 3 once a second: a quarter of the way through each second it is at 0
    /// let ramp = lib::lfo(lfo::Params { shape: lfo::Shape::Saw, low: -1.0, high: 3.0, ..Default::default() }, None)?;
    /// assert_eq!(ramp.sample(&[0.25, 1.25, 2.5])?, [0.0, 0.0, 1.0]);
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Params`] naming `per` if `per` is 'beat' and there is no `grid`; a read is an [`Error::Signal`] at a
    /// time too far for its position to be a number.
    fn source(&self, grid: Option<Grid>) -> Result<Signal, Error> {
        // a counter that counts no times is refused as one that counts any: per beat without a grid, when made
        positions(self.rate, self.per, grid.as_ref(), &[])?;
        let params = self.clone();
        Ok(Signal::from_fn(move |times, out| {
            for (value, position) in out.iter_mut().zip(positions(params.rate, params.per, grid.as_ref(), times)?) {
                *value = params.value_at(position);
            }
            Ok::<(), Error>(())
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::mod_one;

    #[test]
    fn mod_one_is_numpys_mod_its_sign_that_of_the_divisor() {
        // (value, value mod 1 worked out by hand: value - floor(value), +0.0 where that is 0)
        let rows = [
            (0.75, 0.75),
            (2.5, 0.5),
            (-0.25, 0.75),
            (-2.75, 0.25),
            (1.0, 0.0),
            (-1.0, 0.0),
            (0.0, 0.0),
            (-0.0, 0.0),
            (1e300, 0.0), // a float that large is a whole number
            (f64::MAX, 0.0),
            (5e-324, 5e-324), // fmod is exact: the smallest float is its own rest
            (-5e-324, 1.0),   // 1 - 5e-324 rounds to 1: the case the reading moves one step below 1
            (-2e-20, 1.0),
            (-1e-16, 1.0_f64.next_down()), // 1 - 1e-16 is nearest 1 - 2^-53 (floats below 1 are 2^-53 apart)
            (1.0_f64.next_down(), 1.0_f64.next_down()),
            (1e15 + 0.25, 0.25), // 0.25 is exact at 1e15 (a float there is a multiple of 2^-3)
        ];
        for (value, expected) in rows {
            let got = mod_one(value);
            assert_eq!(
                got.to_bits(),
                expected.to_bits(),
                "{value:e} mod 1: {got:e}, not {expected:e}"
            );
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(mod_one(value).is_nan(), "{value}");
        }
    }
}
