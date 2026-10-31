//! `beats`: the parameters of the operation `beats`, declared once, here. What it does is still done in Python.

/// How finely to divide each beat.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Events per beat, 1 to 64: 1 gives the beats alone, 2 adds one `sub` event halfway between neighbouring beats,
    /// 4 adds three, and so on. Raising it gives a finer pulse.
    #[param(default = 1, unit = Count, min = 1, max = 64)]
    pub division: i64,
}
