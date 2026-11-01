//! The node of [`Signal::mapped`](super::Signal::mapped): its input's values, each replaced by a function of it.

use super::node::{Next, Node, Read};
use super::walk::callback;
use super::{Origin, Signal};
use crate::Error;

/// A function of one value.
type Map = dyn Fn(f64) -> f64 + Send + Sync;

/// `map` of the input's value at each time.
pub(crate) struct Mapped {
    input: Signal,
    map: Box<Map>,
    /// One more than the input's.
    depth: usize,
}

impl Mapped {
    /// The node giving `map` of each of `input`'s values.
    pub(crate) fn new(input: &Signal, map: impl Fn(f64) -> f64 + Send + Sync + 'static) -> Self {
        Self {
            input: input.clone(),
            map: Box::new(map),
            depth: input.0.depth() + 1,
        }
    }
}

impl Node for Mapped {
    fn start<'g>(&'g self, _times: &[f64], _out: &mut [f64]) -> Result<Next<'g>, Box<Error>> {
        Ok(Next::InPlace(&self.input))
    }

    // `map` is user code: it may read a signal it holds, a walk of its own on this thread's stack, so it is counted
    // as a callback (one count a read of the node, not one a value)
    fn finish(&self, _times: &[f64], _inputs: &[f64], out: &mut [f64]) -> Read {
        callback(|| {
            for value in out.iter_mut() {
                *value = (self.map)(*value);
            }
            Ok(())
        })?;
        Ok(())
    }

    fn depth(&self) -> usize {
        self.depth
    }

    fn origin(&self) -> Origin<'_> {
        Origin::Callback
    }

    fn label(&self) -> &'static str {
        "mapped"
    }
}
