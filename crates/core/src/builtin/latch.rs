//! `latch`: the parameters of the source `latch`, declared once, here. What it does is still done in Python.

/// Which events count, and the level before and after the first of them.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Only events with this label count; empty takes every event, unlabelled ones too.
    #[param(default = "")]
    pub label: String,
    /// The level until the first matching event, 0 to 1; it is also the level when no event matches.
    #[param(default = 0.0, unit = Fraction, min = 0, max = 1)]
    pub before: f64,
    /// The level from the first matching event on, 0 to 1; lower than `before` makes an inverted latch.
    #[param(default = 1.0, unit = Fraction, min = 0, max = 1)]
    pub after: f64,
}
