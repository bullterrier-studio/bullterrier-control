//! Seeded random noise between `low` and `high`: a new random value every 1 / `rate` seconds, held, joined by straight
//! lines or eased between them.
//!
//! The random value at whole position k (k = floor(rate * t)) is the k-th number of the stream of `seed` (`SplitMix64`,
//! mapped to 0..1), so the signal is a pure function of seed and time: it can be read at any time in any order and is
//! the same on every machine. A read at position k + x (x from 0 to 1) is the value at k for 'hold', the straight line
//! from k to k + 1 for 'linear', or that line eased by 3x^2 - 2x^3 for 'smooth', which has no slope at a lattice point.
//! The value is low + (high - low) times that, and never leaves the range between `low` and `high`. A time too far
//! from 0 for the position to be a number is a `SignalError`.

use crate::cadence::{Per, positions};
use crate::hashing::uniform;
use crate::scaling::between;
use crate::{Choice, Error, Signal, Source};

/// The choices of `shape`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Shape {
    /// The choice `hold`.
    Hold,
    /// The choice `linear`.
    Linear,
    /// The choice `smooth`.
    Smooth,
}

/// The noise: how it moves between its random values, how often it draws a new one, and its seed and range.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// How the value moves from one random value to the next: 'hold' (steps), 'linear' (straight lines, with corners)
    /// or 'smooth' (eased, no corners).
    #[param(default = Shape::Smooth)]
    pub shape: Shape,
    /// Random values per second: a new one every 1 / rate seconds. Raising it makes the noise busier.
    #[param(default = 4.0, unit = Hertz, min = 0.001, max = 1000)]
    pub rate: f64,
    /// Which noise, 0 to 4294967295: the same seed gives the same values on every machine, read in any order; another
    /// seed gives other values.
    #[param(default = 0, unit = Count, min = 0, max = 4294967295)]
    pub seed: i64,
    /// The bottom of the range the values fall in; set above `high` to flip the noise.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub low: f64,
    /// The top of the range the values fall in.
    #[param(default = 1.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub high: f64,
}

/// 2^64: a position is reduced to the index the hash reads, so whole numbers wrap here (and the float 2^64 is exact).
const INDEX_PERIOD: f64 = 18_446_744_073_709_551_616.0;

impl Params {
    /// The value at `position` (cycles counted from 0): the lattice values around it joined as `shape` says.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "seed is 0 to 4294967295; a position is not negative (rate and time are not), and whole % 2^64 is a whole number from 0 to 2^64 - 1"
    )]
    fn value_at(&self, position: f64) -> f64 {
        let seed = self.seed as u64;
        let whole = position.floor();
        let fraction = position - whole;
        // fmod is exact, so a position beyond 2^64 reads the index its integer wraps to
        let first = (whole % INDEX_PERIOD) as u64;
        let here = uniform(seed, first);
        let progress = match self.shape {
            Shape::Hold => return between(self.low, self.high, here),
            Shape::Linear => fraction,
            // 3x^2 - 2x^3: 0 at 0 and 1 at 1, with no slope at either end
            Shape::Smooth => fraction * fraction * (3.0 - 2.0 * fraction),
        };
        let there = uniform(seed, first.wrapping_add(1));
        // plain operators: Rust never fuses a multiply and an add
        between(self.low, self.high, here + (there - here) * progress)
    }
}

impl Source for Params {
    const NAME: &'static str = "noise";
    type Inputs = ();

    /// The signal of the noise of `seed`, read at any time.
    ///
    /// ```
    /// use bullterrier_control::builtin::noise;
    /// use bullterrier_control::lib;
    ///
    /// // one value a second, held: the same seed gives the same value at the same position, in the range
    /// let wobble = lib::noise(noise::Params { shape: noise::Shape::Hold, rate: 1.0, seed: 7, ..Default::default() })?;
    /// let values = wobble.sample(&[0.5, 0.9, 1.5])?;
    /// assert_eq!(values[0], values[1]); // both in the first second
    /// assert!(values.iter().all(|value| (0.0..=1.0).contains(value)));
    /// assert_eq!(wobble.sample(&[1.5])?, [values[2]]);
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// None when made; a read is an [`Error::Signal`] (as it is, not wrapped) at a time too far from 0 for `rate * t` to
    /// be a number.
    fn source(&self, (): ()) -> Result<Signal, Error> {
        let params = self.clone();
        Ok(Signal::from_fn(move |times, out| {
            for (value, position) in out.iter_mut().zip(positions(params.rate, Per::Second, None, times)?) {
                *value = params.value_at(position);
            }
            Ok::<(), Error>(())
        }))
    }
}
