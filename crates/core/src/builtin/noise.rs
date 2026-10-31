//! `noise`: the parameters of the source `noise`, declared once, here. What it does is still done in Python.

use crate::Choice;

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
