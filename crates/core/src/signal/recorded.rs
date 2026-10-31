//! [`Recorded`]: a signal with the recipe the registry made it by.

use std::fmt;

use super::Signal;
use super::node::{Node, Read};
use crate::registry::Recipe;

/// A signal as it is, with how the registry made it: it reads as `inner`. A read recurses through it into `inner`,
/// and a drop also into the recipe's signal, so it is as deep as the deeper of `inner` (one more if `inner` is
/// recorded too) and its recipe's signal with it: a call that makes no node of its own (a body that gives its input,
/// or a constant) still counts toward [`MAX_DEPTH`](crate::MAX_DEPTH), while one that does costs nothing more.
pub(crate) struct Recorded {
    recipe: Recipe,
    inner: Signal,
    depth: usize,
}

impl Recorded {
    /// `inner` with `recipe`, and its depth.
    pub(crate) fn new(recipe: Recipe, inner: Signal) -> Self {
        let through_inner = inner.0.depth() + usize::from(inner.0.recipe().is_some());
        let through_recipe = recipe.signal.as_ref().map_or(0, |signal| signal.0.depth() + 1);
        Self {
            depth: through_inner.max(through_recipe),
            recipe,
            inner,
        }
    }
}

impl fmt::Debug for Recorded {
    // not the signals: formatting them would recurse through every recipe upstream, twice where a body gives its input
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Recorded")
            .field("name", &self.recipe.name)
            .field("values", &self.recipe.values)
            .field("depth", &self.depth)
            .finish_non_exhaustive()
    }
}

impl Node for Recorded {
    fn read(&self, times: &[f64], out: &mut [f64]) -> Read {
        self.inner.0.read(times, out)
    }

    fn depth(&self) -> usize {
        self.depth
    }

    fn recipe(&self) -> Option<&Recipe> {
        Some(&self.recipe)
    }
}
