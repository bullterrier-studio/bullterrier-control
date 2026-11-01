//! What a [`Signal`](super::Signal) holds: a [`Node`] of the graph, and the nodes without inputs.

use std::sync::Arc;

use super::walk::callback;
use super::{Origin, Signal};
use crate::Error;

/// What a step of a read gives: its error boxed, so a `?` moves a pointer, not an [`Error`].
pub(crate) type Read = Result<(), Box<Error>>;

/// What a node's read needs once [`Node::start`] has run: the walk (`walk::read`, the one place that reads inputs)
/// reads the inputs named, then calls [`Node::finish`].
pub(crate) enum Next<'g> {
    /// Nothing: the node wrote its values.
    Done,
    /// The node's values are `input`'s at the same times: the walk reads `input` in its place (no finish).
    As(&'g Signal),
    /// `input`'s values at the same times, into `out`; then [`Node::finish`] (given no values) works on them there.
    InPlace(&'g Signal),
    /// `first`'s values at the same times into `out`, then `second`'s into a buffer of their own, given to
    /// [`Node::finish`]. `second` is read only once `first` is done, as a recursive read would.
    Both(&'g Signal, &'g Signal),
    /// `input`'s values at `times` (the node's own, a grid) into a buffer of their own, given to [`Node::finish`];
    /// `times` is freed first.
    At(&'g Signal, Vec<f64>),
}

/// A node of a signal graph. Sealed (crate-private): code outside the crate makes one through `Signal::from_fn`.
///
/// A read is two steps so that no node reads another: the walk does, in a loop over a stack on the heap, so a graph
/// of any depth is read on any thread's stack. Every time is finite and from 0 up: the public `Signal::sample_into`
/// checks the times and the values once, so a node reads its inputs unchecked. `out` is as long as `times`.
pub(crate) trait Node {
    /// The first step of a read of `times`: write the values into `out`, or name the inputs to read first.
    fn start<'g>(&'g self, times: &[f64], out: &mut [f64]) -> Result<Next<'g>, Box<Error>>;

    /// The last step, once the inputs [`Node::start`] named are read: `inputs` is the buffer it asked for (empty for
    /// [`Next::InPlace`]). A node without inputs is never finished.
    fn finish(&self, _times: &[f64], _inputs: &[f64], _out: &mut [f64]) -> Read {
        Ok(())
    }

    /// How many nodes deep the graph is from this one, counting it: 1 for a node without inputs. `Signal` refuses to
    /// build a graph deeper than its `MAX_DEPTH`; a read sizes its stack of frames by it.
    fn depth(&self) -> usize;

    /// How this node's signal was made; only a recorded node was made by a thing (and holds its recipe).
    fn origin(&self) -> Origin<'_>;

    /// What the node is, in a word: its kind, or the name of the thing that made it. Never its inputs: the one
    /// `Debug` of a signal prints this and the depth alone, so a signal, and anything deriving `Debug` that holds one,
    /// formats in a few bytes a level however deep or shared the graph upstream is.
    fn label(&self) -> &str;
}

/// The same value at every time.
pub(crate) struct Constant(pub(crate) f64);

impl Node for Constant {
    fn start<'g>(&'g self, _times: &[f64], out: &mut [f64]) -> Result<Next<'g>, Box<Error>> {
        out.fill(self.0);
        Ok(Next::Done)
    }

    fn depth(&self) -> usize {
        1
    }

    fn origin(&self) -> Origin<'_> {
        Origin::Values
    }

    fn label(&self) -> &'static str {
        "constant"
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

impl Node for FromFn {
    fn start<'g>(&'g self, times: &[f64], out: &mut [f64]) -> Result<Next<'g>, Box<Error>> {
        out.fill(0.0);
        callback(|| (self.0)(times, out))?;
        Ok(Next::Done)
    }

    fn depth(&self) -> usize {
        1
    }

    fn origin(&self) -> Origin<'_> {
        Origin::Callback
    }

    fn label(&self) -> &'static str {
        "from_fn"
    }
}

/// A signal that cannot be read: every read is its error. What an operator gives where it cannot return an error.
pub(crate) struct Refused(pub(crate) Error);

impl Node for Refused {
    fn start<'g>(&'g self, _times: &[f64], _out: &mut [f64]) -> Result<Next<'g>, Box<Error>> {
        Err(Box::new(self.0.clone()))
    }

    fn depth(&self) -> usize {
        1
    }

    fn origin(&self) -> Origin<'_> {
        Origin::Operator // only an operator makes one
    }

    fn label(&self) -> &'static str {
        "refused"
    }
}
