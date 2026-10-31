//! `keyframes`: the parameters of the source `keyframes`, declared once, here. What it does is still done in Python.

/// Keyframes have no parameters: the points, with their eases, say everything.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params;
