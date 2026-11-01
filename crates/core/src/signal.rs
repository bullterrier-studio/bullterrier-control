//! [`Signal`]: a value that changes over time, readable at any moment in seconds.
//!
//! ```
//! use bullterrier_control::Signal;
//!
//! let level = Signal::constant(0.25)?;
//! assert_eq!(level.sample(&[0.0, 12.4, 100.0])?, [0.25, 0.25, 0.25]);
//! # Ok::<(), bullterrier_control::Error>(())
//! ```
//!
//! Promises:
//! - A signal has a value at every time. A time before 0 reads as time 0; a signal made from a series holds its first
//!   and last value beyond its ends.
//! - [`Signal::sample_into`] (and [`Signal::sample`] and [`Signal::at`], which call it) refuses a time that is not
//!   finite and a value that is not finite (an overflow) with [`Error::Signal`] naming the time: a value is finite or
//!   not given. The same times give the same values, whatever was read before and in whatever order or batches.
//! - Signals combine with `+`, `-`, `*` and negation, a signal or a borrowed one with a signal or an `f64` on either
//!   side, and with [`maximum`] and [`minimum`]; the result is a signal that combines again. A crossfade is
//!   `a * (1.0 - m) + b * m`. A number that is not finite gives a signal whose every read is the [`Error::Signal`]
//!   [`Signal::constant`] gives for it, since an operator cannot return an error.

mod combine;
mod node;
mod recorded;
mod rendered;
mod series;
#[cfg(test)]
mod tests;
mod walk;

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use crate::formats::{Series, SeriesOptions, Unit};
use crate::registry::Recipe;
use crate::{Error, Shaper};
use combine::Operation;
use node::{Constant, FromFn, Node};
use recorded::Recorded;
use rendered::Rendered;
pub use rendered::Rendering;
use series::SeriesNode;

/// How many nodes deep a signal's graph may be, counting the signal and its deepest input: reading and dropping a
/// graph recurse this deep. An operator that would go deeper gives a signal refusing every read with
/// [`Error::Depth`]; a sum of more terms than this fits as a balanced tree (`(a + b) + (c + d)`), not as a chain.
///
/// ```
/// use bullterrier_control::{MAX_DEPTH, Signal};
///
/// let mut sum = Signal::constant(0.0)?; // 1 deep
/// for _ in 1..MAX_DEPTH {
///     sum = sum + 1.0;
/// }
/// assert_eq!(sum.at(0.0)?, 999.0);
/// assert!((sum + 1.0).at(0.0).is_err()); // one more is too deep
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
// A level of a read takes about 400 bytes of stack in a debug build and 80 in a release one (a drop less): 1000 fits a
// 2 MiB test thread five times over and wasm's 1 MiB stack ten times over.
pub const MAX_DEPTH: usize = 1000;

/// How a signal made from a series reads a time between two samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Interpolation {
    /// On the straight line between the samples either side.
    Linear,
    /// The earlier sample's value, until the next sample's time.
    Hold,
}

/// How a signal was made ([`Signal::origin`]): by a thing of the [registry](crate::registry), which a patch can name,
/// or by one of the means a patch has no node for.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Origin<'a> {
    /// By a thing of the registry, through any of its doors: how is its recipe.
    Thing(&'a Recipe),
    /// By an operator: `+`, `-`, `*`, negation, [`maximum`] or [`minimum`].
    Operator,
    /// From values it holds: [`Signal::constant`], [`Signal::from_values`] or [`Signal::from_series`].
    Values,
    /// By a callback: [`Signal::from_fn`] (in Python, a `compute`).
    Callback,
    /// By a recurrence run over another signal: [`Signal::rendered`] (in Python, `Rendered(run=...)`).
    Rendered,
}

/// The optional parts of [`Signal::from_values`]; end a literal with `..Default::default()`.
///
/// ```
/// use bullterrier_control::{FromValues, Interpolation, Signal};
///
/// let late = Signal::from_values(&[0.4, 0.8], 10.0, FromValues { start: 1.0, ..Default::default() }, Interpolation::Hold)?;
/// assert_eq!(late.sample(&[0.0, 1.05, 1.1])?, [0.4, 0.4, 0.8]);
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FromValues {
    /// When the first value is, in seconds, `>= 0`. Default 0.
    pub start: f64,
}

/// A value that changes over time, readable at any moment in seconds: a graph of nodes behind an [`Arc`], so a clone
/// is cheap and shares the graph, and one signal can feed several others. `Send` and `Sync`: any thread may read it.
///
/// ```
/// use bullterrier_control::Signal;
///
/// let echo = Signal::from_fn(|times, out| {
///     out.copy_from_slice(times);
///     Ok::<(), std::fmt::Error>(())
/// });
/// assert_eq!(echo.sample(&[-1.0, 0.5])?, [0.0, 0.5]); // a time before 0 reads as 0
///
/// let fade = Signal::constant(0.25)?;
/// let mix = &echo * (1.0 - &fade) + 2.0 * &fade; // signals and numbers combine on either side
/// assert_eq!(mix.sample(&[0.0, 2.0])?, [0.5, 2.0]);
/// assert!((echo + f64::NAN).at(0.0).is_err()); // a number that is not finite refuses every read
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
#[derive(Clone)]
pub struct Signal(Arc<dyn Node + Send + Sync>);

impl Drop for Signal {
    // the last handle drops the node, which drops its inputs' handles: a walk as deep as the graph, run as a loop
    fn drop(&mut self) {
        walk::release(&mut self.0);
    }
}

impl fmt::Debug for Signal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Signal").field(&self.0).finish()
    }
}

impl Signal {
    /// The signal that is `value` at every time.
    ///
    /// ```
    /// use bullterrier_control::Signal;
    ///
    /// assert_eq!(Signal::constant(-3.5)?.at(7.0)?, -3.5);
    /// assert!(Signal::constant(f64::NAN).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Signal`] if `value` is not finite.
    pub fn constant(value: f64) -> Result<Self, Error> {
        Ok(Self::of(Constant(finite(value)?)))
    }

    /// The signal whose values `f` writes: called with times that are finite and from 0 up, and `out` as long, filled
    /// with 0.0. Its values are checked like any other's (a value that is not finite is an [`Error::Signal`]); an error
    /// it returns is the [`Error::Node`] of the read, with `f`'s error as its [`source`](std::error::Error::source).
    /// It must give the same values for the same times, whatever it was called with before: a signal's promise.
    /// A node counts as depth 1 for [`MAX_DEPTH`] whatever `f` samples inside: a chain of closures that each sample
    /// the previous one is not capped, so it must stay shallow.
    ///
    /// ```
    /// use std::error::Error as _;
    /// use bullterrier_control::Signal;
    ///
    /// let doubled = Signal::from_fn(|times, out| {
    ///     for (value, time) in out.iter_mut().zip(times) {
    ///         *value = 2.0 * time;
    ///     }
    ///     Ok::<(), std::fmt::Error>(())
    /// });
    /// assert_eq!(doubled.sample(&[0.25, 3.0])?, [0.5, 6.0]);
    ///
    /// let failing = Signal::from_fn(|_, _| Err(std::fmt::Error));
    /// let error = failing.at(1.0).err().and_then(|error| error.source().map(ToString::to_string));
    /// assert_eq!(error.as_deref(), Some("an error occurred when formatting an argument"));
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    pub fn from_fn<E>(f: impl Fn(&[f64], &mut [f64]) -> Result<(), E> + Send + Sync + 'static) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::of(FromFn::new(f))
    }

    /// The signal that needs history (a smoother, a slew limiter): `run`, a causal recurrence, over `input` sampled at
    /// `rendering.rate` samples per second from time 0, read by holding the latest sample at or before each time.
    ///
    /// `run` gets the input's values at times `i / rate` (unchecked: a value that is not finite comes through, and only a
    /// read of an output it makes not finite is refused) and returns as many outputs. It must be causal (output *i*
    /// depends on inputs 0 to *i* only) and give the same outputs for the same inputs: then the same time gives the same
    /// value whatever was read before, in whatever order or batches, and a read at time t depends on the input only up to
    /// t (a float rounding of a billionth of a sample aside).
    ///
    /// The signal renders as far as the latest time read needs, at least 1024 samples and at least twice what it holds,
    /// at most `rendering.limit`, and keeps what it rendered. A read needing more than the limit is an [`Error::Signal`]
    /// before anything is rendered or allocated. Several threads may read it at once: no lock is held while the input is
    /// read or `run` runs, so either may read this signal again; two reads that render at once each render, and the
    /// longer history is kept.
    ///
    /// ```
    /// use bullterrier_control::{Rendering, Signal};
    ///
    /// // the running sum of a constant 1 at 10 samples a second, in seconds: (i + 1) / 10 at sample i
    /// let integral = Signal::rendered(&Signal::constant(1.0)?, Rendering { rate: 10.0, ..Default::default() }, |inputs| {
    ///     let mut sum = 0.0;
    ///     Ok::<_, std::fmt::Error>(inputs.iter().map(|input| { sum += input; sum / 10.0 }).collect())
    /// })?;
    /// assert_eq!(integral.sample(&[0.0, 0.15, 2.55])?, [0.1, 0.2, 2.6]);
    /// assert!(integral.at(1e7).is_err()); // 100 million samples: over the limit
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Signal`] if `rendering.rate` is not positive and finite or `rendering.limit` is 0, [`Error::Depth`] if
    /// the graph would be deeper than [`MAX_DEPTH`]. A read is refused as by any signal, and also with
    /// [`Error::Signal`] when it needs more samples than the limit or `run` returns another number of outputs than it
    /// got inputs; an error `run` returns is the [`Error::Node`] of the read, `run`'s error its
    /// [`source`](std::error::Error::source).
    pub fn rendered<E>(
        input: &Signal,
        rendering: Rendering,
        run: impl Fn(&[f64]) -> Result<Vec<f64>, E> + Send + Sync + 'static,
    ) -> Result<Self, Error>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::checked(Rendered::new(input, rendering, run)?)
    }

    /// The signal that reads `series`: sample *i* is at `start + i / rate` seconds, the first and last values hold
    /// beyond the ends, and between samples the value follows `interpolation`. A time that is a sample's time to within
    /// a billionth of a sample (`0.29` s at rate 100, which is 28.999999999999996 samples in floats) is that sample's.
    ///
    /// ```
    /// use bullterrier_control::formats::{Series, SeriesOptions, Unit};
    /// use bullterrier_control::{Interpolation, Signal};
    ///
    /// let series = Series::new(2.0, vec![0.0, 1.0, 0.0], Unit::Ratio, SeriesOptions { start: 1.0, ..Default::default() })?;
    /// let ramp = Signal::from_series(&series, Interpolation::Linear);
    /// assert_eq!(ramp.sample(&[0.0, 1.25, 1.5, 9.0])?, [0.0, 0.5, 1.0, 0.0]);
    /// let steps = Signal::from_series(&series, Interpolation::Hold);
    /// assert_eq!(steps.sample(&[1.25, 1.5, 1.9])?, [0.0, 1.0, 1.0]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn from_series(series: &Series, interpolation: Interpolation) -> Self {
        Self::of(SeriesNode {
            start: series.start(),
            rate: series.rate(),
            values: series.values().to_vec(),
            interpolation,
        })
    }

    /// The signal that reads `values` as plain ratios, value *i* at `options.start + i / rate` seconds: the
    /// [`Signal::from_series`] of a [`Series`] in [`Unit::Ratio`].
    ///
    /// ```
    /// use bullterrier_control::{Error, FromValues, Interpolation, Signal};
    ///
    /// let energy = Signal::from_values(&[0.0, 1.0, 0.2, 0.0], 10.0, FromValues::default(), Interpolation::Linear)?;
    /// assert_eq!(energy.sample(&[0.05, 0.1, 0.3, 1.0])?, [0.5, 1.0, 0.0, 0.0]);
    ///
    /// let refused = Signal::from_values(&[], 10.0, FromValues::default(), Interpolation::Linear);
    /// assert!(matches!(refused, Err(Error::Format { source }) if source.problems[0].path == "values"));
    /// # Ok::<(), Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Format`] listing every rule of a series broken (`values` empty or not finite, `rate` not above 0,
    /// `start` below 0), each naming its field.
    pub fn from_values(
        values: &[f64],
        rate: f64,
        options: FromValues,
        interpolation: Interpolation,
    ) -> Result<Self, Error> {
        let options = SeriesOptions {
            start: options.start,
            ..Default::default()
        };
        let series =
            Series::new(rate, values.to_vec(), Unit::Ratio, options).map_err(|source| Error::Format { source })?;
        Ok(Self::from_series(&series, interpolation))
    }

    /// This signal through `shaper`: any [`Shaper`], built-in or a plugin's, goes into a chain the same way, by the
    /// one door of the [registry](crate::registry). The result is what [`Thing::call`](crate::registry::Thing::call)
    /// of the thing `T` is gives with `shaper`'s values: checked by its table, refused under its `NAME`, and carrying
    /// its [`recipe`](Self::recipe).
    ///
    /// ```
    /// use bullterrier_control::builtin::slew;
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// let step = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
    /// let ramp = step.pipe(slew::Params { rise: 0.5, fall: 0.5, rate: 10.0 })?;
    /// assert_eq!(ramp.sample(&[0.0, 0.1, 0.2])?, [0.0, 0.2, 0.4]);
    /// assert_eq!(ramp.recipe().map(|recipe| recipe.name.as_str()), Some("slew"));
    /// let refused = step.pipe(slew::Params { rise: -1.0, ..Default::default() }).map_err(|error| error.to_string());
    /// assert_eq!(refused.err().as_deref(), Some(r#"the parameters of "slew" do not fit: rise: must be >= 0.0 (got -1.0)"#));
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Thing::call`](crate::registry::Thing::call): [`Error::Params`] under `T`'s `NAME` listing every rule the
    /// parameters break, [`Error::Depth`], whatever error the shaper gives; and [`Error::Registration`] if `T` breaks
    /// a rule of the registry (its `NAME` is not a name, a parameter has an input's name).
    pub fn pipe<T: Shaper>(&self, shaper: T) -> Result<Signal, Error> {
        crate::registry::shaped(shaper, self)
    }

    /// The values at `times` (seconds, in any order), one for each.
    ///
    /// ```
    /// use bullterrier_control::Signal;
    ///
    /// let level = Signal::constant(0.5)?;
    /// assert_eq!(level.sample(&[3.0, -1.0, 0.0])?, [0.5, 0.5, 0.5]);
    /// assert!(level.sample(&[f64::NAN]).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::sample_into`].
    pub fn sample(&self, times: &[f64]) -> Result<Vec<f64>, Error> {
        let mut values = vec![0.0; times.len()];
        self.sample_into(times, &mut values)?;
        Ok(values)
    }

    /// The values at `times` into `values`, which must be as long: for a loop that reads into one buffer again and
    /// again. After an error, what `values` holds means nothing.
    ///
    /// ```
    /// use bullterrier_control::Signal;
    ///
    /// let level = Signal::constant(0.5)?;
    /// let mut values = [0.0; 3];
    /// level.sample_into(&[0.0, 1.0, 2.0], &mut values)?;
    /// assert_eq!(values, [0.5, 0.5, 0.5]);
    /// assert!(level.sample_into(&[0.0, 1.0], &mut values).is_err()); // two times, room for three
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Signal`] if `values` is not as long as `times`, a time is not finite (naming it), or a value is not
    /// finite (naming the first time that gave one); whatever error a node of the graph gives (an [`Error::Node`] from
    /// a callback).
    pub fn sample_into(&self, times: &[f64], values: &mut [f64]) -> Result<(), Error> {
        self.read_into(times, values)?;
        match values.iter().zip(times).find(|(value, _)| !value.is_finite()) {
            Some((_, time)) => Err(Error::Signal {
                message: format!("the signal is not finite at time {time:?} (an overflow?)"),
            }),
            None => Ok(()),
        }
    }

    /// The values at `times` into `values` as [`Signal::sample_into`] reads them, but a value that is not finite is
    /// given, not refused: for a callback combining signals (a [`Signal::from_fn`] reading its input), whose own
    /// result the read that called it checks.
    ///
    /// ```
    /// use bullterrier_control::Signal;
    ///
    /// let huge = Signal::constant(f64::MAX)? * 2.0;
    /// let mut values = [0.0];
    /// huge.read_into(&[1.0], &mut values)?;
    /// assert_eq!(values, [f64::INFINITY]);
    /// assert!(huge.sample_into(&[1.0], &mut values).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Signal`] if `values` is not as long as `times` or a time is not finite (naming it); whatever error a
    /// node of the graph gives (an [`Error::Node`] from a callback).
    pub fn read_into(&self, times: &[f64], values: &mut [f64]) -> Result<(), Error> {
        if times.len() != values.len() {
            return Err(Error::Signal {
                message: format!(
                    "{} times need as many values, not room for {}",
                    times.len(),
                    values.len()
                ),
            });
        }
        if let Some(time) = times.iter().find(|time| !time.is_finite()) {
            return Err(Error::Signal {
                message: format!("times must be finite (got {time:?})"),
            });
        }
        // a copy only when a time is before 0: a loop reading times from 0 up allocates nothing here
        let from_zero = if times.iter().any(|&time| time < 0.0) {
            Cow::Owned(times.iter().map(|&time| if time < 0.0 { 0.0 } else { time }).collect())
        } else {
            Cow::Borrowed(times)
        };
        walk::read(self, &from_zero, values).map_err(|error| *error)
    }

    /// The value at `time` (seconds).
    ///
    /// ```
    /// use bullterrier_control::Signal;
    ///
    /// assert_eq!(Signal::constant(0.5)?.at(-2.0)?, 0.5);
    /// assert!(Signal::constant(0.5)?.at(f64::INFINITY).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::sample_into`].
    pub fn at(&self, time: f64) -> Result<f64, Error> {
        let mut value = [0.0];
        self.sample_into(&[time], &mut value)?;
        let [value] = value;
        Ok(value)
    }

    /// How this signal was made from a thing of the [registry](crate::registry), through any of its doors
    /// ([`Thing::call`](crate::registry::Thing::call), [`pipe`](Self::pipe), a typed method such as
    /// [`slew`](Self::slew), a [`lib`](crate::lib) function), if it was: a signal made any other way (an operator,
    /// [`Signal::from_fn`], [`Signal::rendered`]) has none.
    ///
    /// ```
    /// use bullterrier_control::registry::{Call, Output, Registry};
    /// use bullterrier_control::Signal;
    /// use serde_json::json;
    ///
    /// let level = Signal::constant(0.5)?;
    /// assert!(level.recipe().is_none());
    /// let call = Call { signals: vec![level], values: json!({"rise": 0.5}), ..Default::default() };
    /// let Output::Signal(slewed) = Registry::builtin().get("slew")?.call(&call)? else { panic!("a signal") };
    /// let recipe = slewed.recipe().map(|recipe| (recipe.name.as_str(), recipe.values["rise"].as_f64()));
    /// assert_eq!(recipe, Some(("slew", Some(0.5))));
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    #[must_use]
    pub fn recipe(&self) -> Option<&Recipe> {
        match self.0.origin() {
            Origin::Thing(recipe) => Some(recipe),
            _ => None,
        }
    }

    /// How this signal was made: by a thing of the [registry](crate::registry) (with its [`recipe`](Self::recipe)),
    /// or by which other means. What saving a graph as a patch asks of each signal, to say why one it cannot save is
    /// not a thing's.
    ///
    /// ```
    /// use bullterrier_control::{Origin, Signal};
    ///
    /// let level = Signal::constant(0.5)?;
    /// assert!(matches!(level.origin(), Origin::Values));
    /// assert!(matches!((&level * 2.0).origin(), Origin::Operator));
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    #[must_use]
    pub fn origin(&self) -> Origin<'_> {
        self.0.origin()
    }

    /// `signal` with the recipe the registry made it by.
    ///
    /// # Errors
    /// [`Error::Depth`] if it would be deeper than [`MAX_DEPTH`] (see [`Recorded`]).
    pub(crate) fn recorded(recipe: Recipe, signal: Signal) -> Result<Self, Error> {
        Self::checked(Recorded::new(recipe, signal))
    }

    /// The signal holding `node`, which has no inputs.
    fn of(node: impl Node + Send + Sync + 'static) -> Self {
        Self(Arc::new(node))
    }

    /// The signal holding `node`, which may have inputs.
    ///
    /// # Errors
    /// [`Error::Depth`] if the graph from `node` is deeper than [`MAX_DEPTH`].
    pub(crate) fn checked(node: impl Node + Send + Sync + 'static) -> Result<Self, Error> {
        match node.depth() {
            depth if depth > MAX_DEPTH => Err(Error::Depth {
                message: format!(
                    "a signal can be at most {MAX_DEPTH} nodes deep (this one would be {depth}): combine many signals \
                     as a balanced tree, not a chain"
                ),
            }),
            _ => Ok(Self::of(node)),
        }
    }
}

/// The larger of `first` and `second` at every time.
///
/// ```
/// use bullterrier_control::{FromValues, Interpolation, Signal, maximum};
///
/// let ramp = Signal::from_values(&[0.0, 1.0], 1.0, FromValues::default(), Interpolation::Linear)?;
/// let floor = Signal::constant(0.25)?;
/// assert_eq!(maximum(&ramp, &floor).sample(&[0.0, 0.5, 1.0])?, [0.25, 0.5, 1.0]);
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
#[must_use]
pub fn maximum(first: &Signal, second: &Signal) -> Signal {
    combine::signals(Operation::Maximum, first.clone(), second.clone())
}

/// The smaller of `first` and `second` at every time.
///
/// ```
/// use bullterrier_control::{FromValues, Interpolation, Signal, minimum};
///
/// let ramp = Signal::from_values(&[0.0, 1.0], 1.0, FromValues::default(), Interpolation::Linear)?;
/// let ceiling = Signal::constant(0.75)?;
/// assert_eq!(minimum(&ramp, &ceiling).sample(&[0.0, 0.5, 1.0])?, [0.0, 0.5, 0.75]);
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
#[must_use]
pub fn minimum(first: &Signal, second: &Signal) -> Signal {
    combine::signals(Operation::Minimum, first.clone(), second.clone())
}

/// `value` if it is finite; the one refusal of a number that is to be a constant.
fn finite(value: f64) -> Result<f64, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::Signal {
            message: format!("a constant must be finite (got {value:?})"),
        })
    }
}
