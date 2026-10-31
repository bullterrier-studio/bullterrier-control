//! `hold`: the parameters of the shaper `hold`, declared once, here. What it does is still done in Python.

/// Hold has no parameters: the events say when the value is taken.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params;
