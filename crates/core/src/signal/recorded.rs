//! [`Recorded`]: a signal with the recipe the registry made it by.

use super::Signal;
use super::node::{Node, Read};
use crate::registry::Recipe;

/// A signal as it is, with how the registry made it: it reads as `inner` and is as deep (a recipe is not a node of
/// the graph).
#[derive(Debug)]
pub(crate) struct Recorded {
    pub(crate) recipe: Recipe,
    pub(crate) inner: Signal,
}

impl Node for Recorded {
    fn read(&self, times: &[f64], out: &mut [f64]) -> Read {
        self.inner.0.read(times, out)
    }

    fn depth(&self) -> usize {
        self.inner.0.depth()
    }

    fn recipe(&self) -> Option<&Recipe> {
        Some(&self.recipe)
    }
}
