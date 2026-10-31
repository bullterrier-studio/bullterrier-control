//! What a [`Signal`](super::Signal) holds: a [`Node`] of the graph, and the nodes without inputs.

use std::fmt;
use std::sync::Arc;

use crate::Error;

/// A node of a signal graph. Sealed (crate-private): code outside the crate makes one through `Signal::from_fn`.
pub(crate) trait Node: fmt::Debug {
    /// The value at each of `times` into `out`, which is as long. Every time is finite and from 0 up: the public
    /// `Signal::sample_into` checks the times and the values once, so a node reads its inputs unchecked.
    fn read(&self, times: &[f64], out: &mut [f64]) -> Result<(), Error>;
}

/// The same value at every time.
#[derive(Debug)]
pub(crate) struct Constant(pub(crate) f64);

impl Node for Constant {
    fn read(&self, _times: &[f64], out: &mut [f64]) -> Result<(), Error> {
        out.fill(self.0);
        Ok(())
    }
}

/// The callback a [`FromFn`] node calls, its own error already wrapped as [`Error::Node`].
type Callback = dyn Fn(&[f64], &mut [f64]) -> Result<(), Error> + Send + Sync;

/// Values a callback writes: `out` arrives filled with 0.0, so what the callback leaves is the same on every read.
pub(crate) struct FromFn(pub(crate) Box<Callback>);

impl FromFn {
    /// The node calling `f`, keeping its error as the source of an [`Error::Node`].
    pub(crate) fn new<E>(f: impl Fn(&[f64], &mut [f64]) -> Result<(), E> + Send + Sync + 'static) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self(Box::new(move |times, out| {
            f(times, out).map_err(|source| Error::Node {
                source: Arc::new(source),
            })
        }))
    }
}

impl fmt::Debug for FromFn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FromFn")
    }
}

impl Node for FromFn {
    fn read(&self, times: &[f64], out: &mut [f64]) -> Result<(), Error> {
        out.fill(0.0);
        (self.0)(times, out)
    }
}
