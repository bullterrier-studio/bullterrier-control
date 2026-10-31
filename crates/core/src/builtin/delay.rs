//! `delay`: the parameters of the shaper `delay`, declared once, here. What it does is still done in Python.

/// How far the output trails its input.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Seconds the output trails its input, 0 to 3600; raising it shows the input later. 0 passes it through.
    #[param(default = 0.1, unit = Seconds, min = 0, max = 3600)]
    pub lag: f64,
}
