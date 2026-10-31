//! `normalise`: the parameters of the operation `normalise`, declared once, here. What it does is still done in Python.

/// The percentiles of the series that become 0 and 1.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The share of the values, 0 to 0.99, at or below the level that becomes 0: 0.05 puts the 5th percentile at 0,
    /// and lower values are clipped to 0. Raising it clips more of the quiet end; below `upper`.
    #[param(default = 0.05, unit = Fraction, min = 0, max = 0.99)]
    pub lower: f64,
    /// The share of the values, 0.01 to 1, at or below the level that becomes 1: 0.98 puts the 98th percentile at 1,
    /// and higher values are clipped to 1. Lowering it clips more of the loud end; above `lower`.
    #[param(default = 0.98, unit = Fraction, min = 0.01, max = 1)]
    pub upper: f64,
}
