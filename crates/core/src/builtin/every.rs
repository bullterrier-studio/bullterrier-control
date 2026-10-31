//! `every`: the parameters of the operation `every`, declared once, here. What it does is still done in Python.

/// Which of the events in time order to keep: every `n`th, counting from `offset`.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Keep one event in this many, 1 to 1000: 1 keeps all, 2 every second one, 3 every third.
    #[param(default = 2, unit = Count, min = 1, max = 1000)]
    pub n: i64,
    /// How many events to skip before the first one kept, 0 to 999 and below `n`: 0 starts with the first event, 1
    /// with the second.
    #[param(default = 0, unit = Count, min = 0, max = 999)]
    pub offset: i64,
}
