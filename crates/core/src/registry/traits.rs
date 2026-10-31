//! The traits a Rust thing implements: its parameters struct ([`Params`]) is the thing, and the trait says what it
//! makes. A built-in implements one in its file (`src/builtin/<name>.rs`); a plugin the same way, registered by
//! [`Thing::source`](super::Thing::source), [`Thing::shaper`](super::Thing::shaper) or
//! [`Thing::operation`](super::Thing::operation).

use std::collections::BTreeMap;

use super::{Output, Returns};
use crate::formats::{Event, Grid, Series};
use crate::inputs::{Input, InputKind, Keyframe};
use crate::{Error, Params, Signal};

/// A source: its parameters make a signal.
///
/// ```
/// use bullterrier_control::{Error, Params, Signal, Source};
///
/// /// A level that holds.
/// #[derive(Clone, Debug, PartialEq, Params)]
/// struct Level {
///     /// The value it holds.
///     #[param(default = 0.5, unit = Ratio, min = -1, max = 1)]
///     value: f64,
/// }
///
/// impl Source for Level {
///     fn source(&self) -> Result<Signal, Error> {
///         Signal::constant(self.value)
///     }
/// }
///
/// assert_eq!(Level { value: 0.25 }.source()?.at(9.0)?, 0.25);
/// # Ok::<(), Error>(())
/// ```
pub trait Source: Params {
    /// The signal these parameters make. They keep their table's rules: whoever calls a source checks them first (a
    /// call through the registry reads them by the table; a typed call runs [`Params::check`]).
    ///
    /// # Errors
    /// Whatever the source cannot make: an [`Error`] its signal's constructor gives.
    fn source(&self) -> Result<Signal, Error>;
}

/// A shaper: its parameters make a signal of a signal. [`Signal::pipe`] puts any shaper into a chain.
///
/// ```
/// use bullterrier_control::{Error, FromValues, Interpolation, Params, Shaper, Signal};
///
/// /// Where the signal folds back.
/// #[derive(Clone, Debug, PartialEq, Params)]
/// struct Fold {
///     /// How far down from 1 the fold is.
///     #[param(default = 0.5, unit = Fraction, min = 0, max = 1)]
///     depth: f64,
/// }
///
/// impl Shaper for Fold {
///     fn shape(&self, input: &Signal) -> Result<Signal, Error> {
///         let (line, input) = (1.0 - self.depth, input.clone());
///         Ok(Signal::from_fn(move |times, out| {
///             input.read_into(times, out)?;
///             for value in out.iter_mut() {
///                 if *value > line {
///                     *value = 2.0 * line - *value;
///                 }
///             }
///             Ok::<(), Error>(())
///         }))
///     }
/// }
///
/// let glow = Signal::from_values(&[0.25, 0.5, 0.75], 1.0, FromValues::default(), Interpolation::Hold)?;
/// assert_eq!(glow.pipe(Fold { depth: 0.5 })?.sample(&[0.0, 1.0, 2.0])?, [0.25, 0.5, 0.25]);
/// # Ok::<(), Error>(())
/// ```
pub trait Shaper: Params {
    /// The signal these parameters make of `input`. They keep their table's rules: whoever calls a shaper checks them
    /// first ([`Signal::pipe`] runs [`Params::check`]; a call through the registry reads them by the table).
    ///
    /// # Errors
    /// Whatever the shaper cannot make: an [`Error`] its signal's constructor gives ([`Error::Depth`], ...).
    fn shape(&self, input: &Signal) -> Result<Signal, Error>;
}

/// An operation: its parameters make data (events or a series) of its inputs, data too.
///
/// ```
/// use bullterrier_control::formats::{Event, EventOptions};
/// use bullterrier_control::registry::{Output, Returns};
/// use bullterrier_control::{Error, Operation, Params};
///
/// /// How many events to keep.
/// #[derive(Clone, Debug, PartialEq, Params)]
/// struct First {
///     /// How many of the first events are kept.
///     #[param(default = 1, unit = Count, min = 0, max = 100)]
///     n: i64,
/// }
///
/// impl Operation for First {
///     type Inputs = Vec<Event>;
///     const RETURNS: Returns = Returns::Events;
///
///     fn run(&self, events: Vec<Event>) -> Result<Output, Error> {
///         Ok(Output::Events(events.into_iter().take(usize::try_from(self.n).unwrap_or(0)).collect()))
///     }
/// }
///
/// let events = vec![Event::new(0.5, EventOptions::default())?, Event::new(1.0, EventOptions::default())?];
/// assert!(matches!(First { n: 1 }.run(events)?, Output::Events(kept) if kept.len() == 1));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait Operation: Params {
    /// The data it takes: its inputs, as one Rust value ([`Inputs`]).
    type Inputs: Inputs;

    /// What it gives: [`Returns::Events`] or [`Returns::Series`] (an operation makes no signal).
    const RETURNS: Returns;

    /// The data these parameters make of `inputs`: an [`Output`] of the kind [`RETURNS`](Self::RETURNS) says. The
    /// parameters keep their table's rules and the inputs theirs: whoever calls an operation checks them first.
    ///
    /// # Errors
    /// Whatever the operation cannot make.
    fn run(&self, inputs: Self::Inputs) -> Result<Output, Error>;
}

/// The inputs of an [`Operation`] as one Rust value: `()` for none; one input's data for one (`Vec<Event>` for
/// `events`, `Vec<Keyframe>` for `points`, `Grid`, `Vec<f64>` for `values`, `Series`), an `Option` of it when it is
/// optional; or a struct of the operation's own for several.
///
/// ```
/// use bullterrier_control::formats::{Event, Grid};
/// use bullterrier_control::inputs::InputKind;
/// use bullterrier_control::registry::Inputs;
///
/// assert_eq!(<Vec<Event>>::DECLARED, [(InputKind::Events, true)]);
/// assert_eq!(<Option<Grid>>::DECLARED, [(InputKind::Grid, false)]);
/// assert!(<()>::DECLARED.is_empty());
/// ```
pub trait Inputs: Sized {
    /// Each input it takes, with whether it is required, in the order `help()` lists them.
    const DECLARED: &'static [(InputKind, bool)];

    /// The value of `given`: each input under its name, as [`Thing::call`](super::Thing::call) checked them against
    /// [`DECLARED`](Self::DECLARED) (each required one there, each of its kind, no other).
    ///
    /// # Errors
    /// [`Error::Call`] if `given` is not as `DECLARED` says (a bug of the caller: the registry checks it first).
    fn from_inputs(given: &BTreeMap<String, Input>) -> Result<Self, Error>;
}

impl Inputs for () {
    const DECLARED: &'static [(InputKind, bool)] = &[];

    fn from_inputs(_given: &BTreeMap<String, Input>) -> Result<Self, Error> {
        Ok(())
    }
}

/// `Inputs` for the data of one input kind (`$kind`, the variant of [`Input`] holding a `$data`), required, and for
/// an `Option` of it, optional.
macro_rules! one_input {
    ($data:ty, $kind:ident) => {
        impl Inputs for $data {
            const DECLARED: &'static [(InputKind, bool)] = &[(InputKind::$kind, true)];

            fn from_inputs(given: &BTreeMap<String, Input>) -> Result<Self, Error> {
                match given.get(InputKind::$kind.name()) {
                    Some(Input::$kind(data)) => Ok(data.clone()),
                    _ => Err(not_given(InputKind::$kind)),
                }
            }
        }

        impl Inputs for Option<$data> {
            const DECLARED: &'static [(InputKind, bool)] = &[(InputKind::$kind, false)];

            fn from_inputs(given: &BTreeMap<String, Input>) -> Result<Self, Error> {
                match given.get(InputKind::$kind.name()) {
                    Some(Input::$kind(data)) => Ok(Some(data.clone())),
                    None => Ok(None),
                    Some(_) => Err(not_given(InputKind::$kind)),
                }
            }
        }
    };
}

one_input!(Vec<Keyframe>, Points);
one_input!(Vec<Event>, Events);
one_input!(Grid, Grid);
one_input!(Vec<f64>, Values);
one_input!(Series, Series);

/// The error for an input of `kind` that is not given as its kind.
fn not_given(kind: InputKind) -> Error {
    Error::Call {
        message: format!("the input '{kind}' is not given as {kind}"),
    }
}
