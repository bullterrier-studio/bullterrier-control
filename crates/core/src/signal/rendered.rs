//! The node of [`Signal::rendered`](super::Signal::rendered): a causal recurrence over its input sampled at a fixed
//! rate, rendered as far as the reads need and kept.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::node::{Next, Node, Read};
use super::series::SAMPLE_ROUNDING;
use super::walk::callback;
use super::{Origin, Signal};
use crate::Error;

/// Samples a first render makes at least, so that small reads share one render.
const FIRST_RENDER: usize = 1024;

/// How a [`Signal::rendered`](super::Signal::rendered) samples its input; end a literal with `..Default::default()`.
///
/// ```
/// use bullterrier_control::Rendering;
///
/// let fine = Rendering { rate: 4000.0, ..Default::default() };
/// assert_eq!(fine.limit, 20_000_000);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rendering {
    /// Samples of the input per second, positive and finite. Default 1000.
    pub rate: f64,
    /// The most samples one read may need, at least 1. Default 20 000 000: 160 MB of `f64`, at 1000 Hz about five and
    /// a half hours.
    pub limit: usize,
}

impl Default for Rendering {
    fn default() -> Self {
        Self {
            rate: 1000.0,
            limit: 20_000_000,
        }
    }
}

/// A recurrence, its own error already made an [`Error`] by `Error::from_callback`.
type Recurrence = dyn Fn(&[f64]) -> Result<Vec<f64>, Error> + Send + Sync;

/// The values of `run` over `input` sampled at `rate` from time 0, as far as the reads so far needed.
pub(crate) struct Rendered {
    input: Signal,
    rate: f64,
    limit: usize,
    run: Box<Recurrence>,
    /// The values rendered so far, sample *i* at `i / rate`. Locked only to read or replace the `Arc`: never while the
    /// input is read or `run` called, which may read this signal again (a Python callback) or take long.
    history: Mutex<Arc<Vec<f64>>>,
    /// One more than the input's.
    depth: usize,
}

impl Rendered {
    /// The node running `run` over `input` as `rendering` says, `run`'s error kept as the source of an
    /// [`Error::Node`].
    ///
    /// # Errors
    /// [`Error::Signal`] if the rate is not positive and finite (nor large enough that `limit / rate` is a time) or the limit
    /// is 0.
    pub(crate) fn new<E>(
        input: &Signal,
        Rendering { rate, limit }: Rendering,
        run: impl Fn(&[f64]) -> Result<Vec<f64>, E> + Send + Sync + 'static,
    ) -> Result<Self, Error>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        if !(rate.is_finite() && rate > 0.0) {
            return Err(Error::Signal {
                message: format!("rate must be a positive finite number of samples per second (got {rate:?})"),
            });
        }
        if limit == 0 {
            return Err(Error::Signal {
                message: "limit must be at least 1 sample (got 0)".to_owned(),
            });
        }
        // the last sample's time, `limit / rate`, must be a number: the input is read at finite times only
        #[expect(clippy::cast_precision_loss, reason = "a limit is far below 2^53: exact")]
        let last_time = limit as f64 / rate;
        if !last_time.is_finite() {
            return Err(Error::Signal {
                message: format!("rate {rate:?} is too small: {limit} samples would end beyond the largest time"),
            });
        }
        Ok(Self {
            input: input.clone(),
            rate,
            limit,
            run: Box::new(move |inputs| run(inputs).map_err(Error::from_callback)),
            history: Mutex::new(Arc::new(Vec::new())),
            depth: input.0.depth() + 1,
        })
    }

    /// The index of the latest sample at or before `time` (finite, from 0 up), as a float: infinite for a time too far
    /// to count samples for.
    fn sample_at(&self, time: f64) -> f64 {
        (time * self.rate + SAMPLE_ROUNDING).floor()
    }

    /// `run` over `inputs`, the input's values at the first `inputs.len()` samples: the new history, checked to be
    /// as long.
    fn render(&self, inputs: &[f64]) -> Result<Vec<f64>, Box<Error>> {
        let outputs = callback(|| (self.run)(inputs))?;
        if outputs.len() != inputs.len() {
            return Err(Box::new(Error::Signal {
                message: format!(
                    "the recurrence returned {} values for {} inputs: it must return one for each",
                    outputs.len(),
                    inputs.len()
                ),
            }));
        }
        Ok(outputs)
    }

    /// The value at each of `times` into `out`, by holding the latest sample of `history`, which holds them all.
    fn held(&self, history: &[f64], times: &[f64], out: &mut [f64]) {
        for (value, time) in out.iter_mut().zip(times) {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "0 <= the sample <= last, which the history holds: the sample index grows with the time"
            )]
            let sample = self.sample_at(*time) as usize;
            *value = history[sample];
        }
    }

    /// The lock on the history; one a panic poisoned is as good (the history is replaced whole, never half written).
    fn locked(&self) -> MutexGuard<'_, Arc<Vec<f64>>> {
        self.history.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Node for Rendered {
    // the history kept if it holds the latest time read, else the input's values on the grid as far as a longer
    // history needs (read by the walk, no lock held), then `finish` renders and keeps it unless another read kept a
    // longer one meanwhile
    fn start<'g>(&'g self, times: &[f64], out: &mut [f64]) -> Result<Next<'g>, Box<Error>> {
        let Some(largest) = times.iter().copied().reduce(f64::max) else {
            return Ok(Next::Done);
        };
        let last = self.sample_at(largest);
        #[expect(clippy::cast_precision_loss, reason = "a limit is far below 2^53: exact")]
        let limit = self.limit as f64;
        if last >= limit {
            return Err(Box::new(Error::Signal {
                message: format!(
                    "reading time {largest:?} s needs more than {} samples at {:?} Hz: over the limit",
                    self.limit, self.rate
                ),
            }));
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0 <= last < the limit: a whole count of samples"
        )]
        let needed = last as usize + 1;
        let kept = Arc::clone(&self.locked());
        if kept.len() >= needed {
            self.held(&kept, times, out);
            return Ok(Next::Done);
        }
        let count = needed
            .max(kept.len().saturating_mul(2))
            .max(FIRST_RENDER)
            .min(self.limit);
        drop(kept);
        #[expect(clippy::cast_precision_loss, reason = "a count of samples is far below 2^53: exact")]
        let grid = (0..count).map(|index| index as f64 / self.rate).collect();
        Ok(Next::At(&self.input, grid))
    }

    fn finish(&self, times: &[f64], inputs: &[f64], out: &mut [f64]) -> Read {
        let rendered = self.render(inputs)?;
        let history = {
            let mut history = self.locked();
            if rendered.len() > history.len() {
                *history = Arc::new(rendered);
            }
            Arc::clone(&history)
        };
        self.held(&history, times, out);
        Ok(())
    }

    fn depth(&self) -> usize {
        self.depth
    }

    fn origin(&self) -> Origin<'_> {
        Origin::Rendered
    }

    fn label(&self) -> &'static str {
        "rendered"
    }
}

#[cfg(test)]
mod tests;
