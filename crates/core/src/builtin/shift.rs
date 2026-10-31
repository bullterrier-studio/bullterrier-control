//! `shift`: the parameters of the operation `shift`, declared once, here. What it does is still done in Python.

/// How far to move the events.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Seconds to move every event, -3600 to 3600: positive moves them later (a lag), negative earlier (a lead). What
    /// would start before time 0 is cut off there.
    #[param(default = 0.0, unit = Seconds, min = -3600, max = 3600)]
    pub offset: f64,
}
