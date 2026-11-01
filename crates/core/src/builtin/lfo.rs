//! `lfo`: the parameters of the source `lfo`, declared once, here. What it does is still done in Python.

use crate::Choice;
use crate::cadence::Per;

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
