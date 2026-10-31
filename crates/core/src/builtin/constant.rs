//! `constant`: the parameters of the source `constant`, declared once, here. What it does is still done in Python.

/// The value a constant signal holds.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The value the signal holds at every time; negative values are fine.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub value: f64,
}
