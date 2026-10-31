//! `steps`: the parameters of the source `steps`, declared once, here. What it does is still done in Python.

use crate::Choice;

/// The choices of `per`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Per {
    /// The choice `second`.
    Second,
    /// The choice `beat`.
    Beat,
}

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
