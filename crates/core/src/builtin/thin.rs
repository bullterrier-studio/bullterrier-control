//! `thin`: the parameters of the operation `thin`, declared once, here. What it does is still done in Python.

/// The shortest time allowed between two kept events.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The least time between two kept events, in seconds, 0 to 3600: an event is kept when it comes at least this
    /// long after the last kept one. Raising it keeps fewer events; 0 keeps all.
    #[param(default = 0.1, unit = Seconds, min = 0, max = 3600)]
    pub gap: f64,
}
