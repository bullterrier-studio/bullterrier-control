//! `clip`: the parameters of the shaper `clip`, declared once, here. What it does is still done in Python.

/// The range a clipped signal stays inside.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The lowest value the output takes; lower values are raised to it.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub low: f64,
    /// The highest value the output takes; higher values are lowered to it.
    #[param(default = 1.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub high: f64,
}
