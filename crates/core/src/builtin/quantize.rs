//! `quantize`: the parameters of the operation `quantize`, declared once, here. What it does is still done in Python.

/// How fine the positions are that events snap to.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Snap positions per beat, 1 to 64: 1 snaps to the beats, 2 to the beats and the halves between them, 4 to
    /// quarters. Raising it moves events less.
    #[param(default = 1, unit = Count, min = 1, max = 64)]
    pub division: i64,
}
