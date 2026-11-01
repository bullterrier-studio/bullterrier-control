//! The node of [`Signal::retimed`](super::Signal::retimed): its input read at other times than its own.

use super::node::{Next, Node, Read};
use super::walk::callback;
use super::{Origin, Signal};
use crate::Error;

/// A map, its own error already made an [`Error`] by `Error::from_callback`.
type Map = dyn Fn(&[f64]) -> Result<Vec<f64>, Error> + Send + Sync;

/// The input's value at `map(t)` at each time t.
pub(crate) struct Retimed {
    input: Signal,
    map: Box<Map>,
    /// One more than the input's.
    depth: usize,
}

impl Retimed {
    /// The node reading `input` at `map`'s times, `map`'s error a core [`Error`] as it is, any other the source of an [`Error::Node`].
    pub(crate) fn new<E>(input: &Signal, map: impl Fn(&[f64]) -> Result<Vec<f64>, E> + Send + Sync + 'static) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            input: input.clone(),
            map: Box::new(move |times| map(times).map_err(Error::from_callback)),
            depth: input.0.depth() + 1,
        }
    }
}

impl Node for Retimed {
    // the input's times are `map`'s, checked here: the walk reads a node's input at finite times from 0 up only
    fn start<'g>(&'g self, times: &[f64], _out: &mut [f64]) -> Result<Next<'g>, Box<Error>> {
        let mapped = callback(|| (self.map)(times))?;
        if mapped.len() != times.len() {
            return Err(Box::new(Error::Signal {
                message: format!(
                    "a retiming gave {} times for {}: it must give one for each",
                    mapped.len(),
                    times.len()
                ),
            }));
        }
        if let Some(time) = mapped.iter().find(|time| !(time.is_finite() && **time >= 0.0)) {
            return Err(Box::new(Error::Signal {
                message: format!("a retiming must give finite times from 0 up (got {time:?})"),
            }));
        }
        Ok(Next::At(&self.input, mapped))
    }

    fn finish(&self, _times: &[f64], inputs: &[f64], out: &mut [f64]) -> Read {
        out.copy_from_slice(inputs);
        Ok(())
    }

    fn depth(&self) -> usize {
        self.depth
    }

    fn origin(&self) -> Origin<'_> {
        Origin::Callback
    }

    fn label(&self) -> &'static str {
        "retimed"
    }
}
