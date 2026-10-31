//! `gate`: the parameters of the shaper `gate`, declared once, here. What it does is still done in Python.

/// The two levels the gate opens and closes at.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The level at which a closed gate opens: the input at or above it. Raising it asks for a stronger input.
    #[param(default = 0.5, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub open: f64,
    /// The level an open gate closes below: the input under it. At most `open`; the gap between the two is the band
    /// in which the gate keeps its state, which stops a flicker. Equal to `open` is a plain threshold.
    #[param(default = 0.4, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub close: f64,
    /// Internal samples per second; the gate changes on one of them. Raising it is more exact and slower.
    #[param(default = 1000.0, unit = Hertz, min = 10, max = 48000, advanced)]
    pub rate: f64,
}
