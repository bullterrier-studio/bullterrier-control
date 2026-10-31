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
//! - A signal has a value at every time. A time before 0 reads as time 0.
//! - [`Signal::sample_into`] (and [`Signal::sample`] and [`Signal::at`], which call it) refuses a time that is not
//!   finite and a value that is not finite (an overflow) with [`Error::Signal`] naming the time: a value is finite or
//!   not given. The same times give the same values, whatever was read before and in whatever order or batches.

mod node;
#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use crate::Error;
use node::{Constant, FromFn, Node};

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
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
#[derive(Clone)]
pub struct Signal(Arc<dyn Node + Send + Sync>);

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
        self.0.read(&from_zero, values)?;
        match values.iter().zip(times).find(|(value, _)| !value.is_finite()) {
            Some((_, time)) => Err(Error::Signal {
                message: format!("the signal is not finite at time {time:?} (an overflow?)"),
            }),
            None => Ok(()),
        }
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

    /// The signal holding `node`.
    fn of(node: impl Node + Send + Sync + 'static) -> Self {
        Self(Arc::new(node))
    }
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
