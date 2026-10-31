//! `remap`: the parameters of the shaper `remap`, declared once, here. What it does is still done in Python.

/// The range the input is put onto, and the ease that bends the way there.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The output when the input is 0; set above `high` to flip the signal.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub low: f64,
    /// The output when the input is 1.
    #[param(default = 1.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub high: f64,
    /// How the input is bent before it is put onto the range: one of the eases keyframes take (linear, hold, in, out,
    /// in-out, cubic-in, cubic-out, cubic-in-out, overshoot) or cubic-bezier(x1, y1, x2, y2). 'linear' keeps the
    /// shape.
    #[param(default = "linear")]
    pub ease: String,
}
