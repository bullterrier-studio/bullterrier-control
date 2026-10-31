//! `select`: the parameters of the operation `select`, declared once, here. What it does is still done in Python.

/// Which events to keep: by label and by strength.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Keep only events with this label; empty keeps every event, unlabelled ones too.
    #[param(default = "")]
    pub label: String,
    /// The weakest strength kept, 0 to 1: an event is kept when its strength is at least this. Raising it keeps only
    /// the stronger events; 0 keeps them all.
    #[param(default = 0.0, unit = Fraction, min = 0, max = 1)]
    pub threshold: f64,
}
