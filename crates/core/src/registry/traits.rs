//! The traits a Rust thing implements: its parameters struct ([`Params`]) is the thing, and the trait says its name
//! and what it makes. A built-in implements one in its file (`src/builtin/<name>.rs`, its `NAME` the file's name); a
//! plugin the same way, registered by [`Thing::source`](super::Thing::source), [`Thing::shaper`](super::Thing::shaper)
//! or [`Thing::operation`](super::Thing::operation).
//!
//! A trait's method is the thing's body. Called directly it is only that: unchecked and unrecorded. Every door
//! ([`Thing::call`](super::Thing::call), [`Signal::pipe`], [`Signal::pipe_with`], a typed method such as
//! [`Signal::slew`], a `lib` function) checks the values and inputs, runs the body and records the
//! [`Recipe`](super::Recipe) in one place, under `NAME`. Each kind takes its inputs as one value, its `type Inputs`
//! ([`Inputs`]), which the recipe records through [`Inputs::into_inputs`]: so a recipe holds every input the body got.

use std::collections::BTreeMap;

use super::{Output, Returns};
use crate::formats::{Event, Grid, Series};
use crate::inputs::{Input, InputKind, Keyframe};
use crate::{Error, Params, Signal};

/// A source: its parameters make a signal, of its inputs if it takes any ([`Inputs`]).
///
/// ```
/// use bullterrier_control::formats::{Event, EventOptions};
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
///     const NAME: &'static str = "level";
///     type Inputs = ();
///
///     fn source(&self, (): ()) -> Result<Signal, Error> {
///         Signal::constant(self.value)
///     }
/// }
///
/// /// How many events have passed.
/// #[derive(Clone, Debug, PartialEq, Params)]
/// struct Count {}
///
/// impl Source for Count {
///     const NAME: &'static str = "count";
///     type Inputs = Vec<Event>;
///
///     fn source(&self, events: Vec<Event>) -> Result<Signal, Error> {
///         let times: Vec<f64> = events.iter().map(Event::t).collect();
///         Ok(Signal::from_fn(move |at, out| {
///             for (time, value) in at.iter().zip(out.iter_mut()) {
///                 *value = times.iter().filter(|event| **event <= *time).map(|_| 1.0).sum();
///             }
///             Ok::<(), Error>(())
///         }))
///     }
/// }
///
/// assert_eq!(Level { value: 0.25 }.source(())?.at(9.0)?, 0.25);
/// let kicks = vec![Event::new(0.5, EventOptions::default())?, Event::new(1.0, EventOptions::default())?];
/// assert_eq!(Count {}.source(kicks)?.sample(&[0.0, 0.5, 2.0])?, [0.0, 1.0, 2.0]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait Source: Params + 'static {
    /// The name it is registered, called and recorded under (see [`NAME_PATTERN`](super::NAME_PATTERN)): its only
    /// name, whichever door it is made through.
    const NAME: &'static str;

    /// The data it takes: its inputs, as one Rust value ([`Inputs`]; `()` for none). A door records them in the
    /// signal's [`Recipe`](super::Recipe) as [`Inputs::into_inputs`] gives them, and gives the body what it recorded.
    type Inputs: Inputs;

    /// The signal these parameters make of `inputs`: the body, which a door calls only with parameters its table
    /// accepted and inputs as [`Inputs::DECLARED`] says.
    ///
    /// # Errors
    /// Whatever the source cannot make: an [`Error`] its signal's constructor gives.
    fn source(&self, inputs: Self::Inputs) -> Result<Signal, Error>;
}

/// A shaper: its parameters make a signal of a signal, and of its inputs if it takes any ([`Inputs`]).
/// [`Signal::pipe`] puts a shaper that takes none into a chain, [`Signal::pipe_with`] any shaper with its inputs.
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
///     const NAME: &'static str = "fold";
///     type Inputs = ();
///
///     fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> {
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
/// let folded = glow.pipe(Fold { depth: 0.5 })?;
/// assert_eq!(folded.sample(&[0.0, 1.0, 2.0])?, [0.25, 0.5, 0.25]);
/// assert_eq!(folded.recipe().map(|recipe| recipe.name.as_str()), Some("fold"));
/// # Ok::<(), Error>(())
/// ```
pub trait Shaper: Params + 'static {
    /// The name it is registered, called and recorded under, as [`Source::NAME`].
    const NAME: &'static str;

    /// The data it takes besides its signal, as [`Source::Inputs`].
    type Inputs: Inputs;

    /// The signal these parameters make of `input` and `inputs`: the body, which a door calls only with parameters
    /// its table accepted and inputs as [`Inputs::DECLARED`] says.
    ///
    /// # Errors
    /// Whatever the shaper cannot make: an [`Error`] its signal's constructor gives ([`Error::Depth`], ...).
    fn shape(&self, input: &Signal, inputs: Self::Inputs) -> Result<Signal, Error>;
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
///     const NAME: &'static str = "first";
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
pub trait Operation: Params + 'static {
    /// The name it is registered and called under, as [`Source::NAME`].
    const NAME: &'static str;

    /// The data it takes: its inputs, as one Rust value ([`Inputs`]).
    type Inputs: Inputs;

    /// What it gives: [`Returns::Events`] or [`Returns::Series`] (an operation makes no signal).
    const RETURNS: Returns;

    /// The data these parameters make of `inputs`: an [`Output`] of the kind [`RETURNS`](Self::RETURNS) says. The
    /// body, which a door calls only with parameters its table accepted and inputs as [`Inputs::DECLARED`] says.
    ///
    /// # Errors
    /// Whatever the operation cannot make.
    fn run(&self, inputs: Self::Inputs) -> Result<Output, Error>;
}

/// The inputs of a [`Source`], [`Shaper`] or [`Operation`] as one Rust value: `()` for none; one input's data for one
/// (`Vec<Event>` for `events`, `Vec<Keyframe>` for `points`, `Grid`, `Vec<f64>` for `values`, `Series`), an `Option`
/// of it when it is optional; or a struct of the thing's own for several.
///
/// ```
/// use bullterrier_control::formats::{Event, Grid};
/// use bullterrier_control::inputs::InputKind;
/// use bullterrier_control::registry::Inputs;
///
/// assert_eq!(<Vec<Event>>::DECLARED, [(InputKind::Events, true)]);
/// assert_eq!(<Option<Grid>>::DECLARED, [(InputKind::Grid, false)]);
/// assert!(<()>::DECLARED.is_empty());
/// // and back, as a recipe records them
/// assert_eq!(None::<Grid>.into_inputs()?, []);
/// assert!(vec![f64::NAN].into_inputs().is_err());
/// # Ok::<(), bullterrier_control::Error>(())
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

    /// Each input it holds, checked by its kind's rules ([`Input::from_values`], ...): what a typed door records and
    /// gives the body back through [`from_inputs`](Self::from_inputs), so an input left out here is one the thing
    /// never sees, never one it uses unrecorded. Required, as `from_inputs` is: no type takes inputs without saying
    /// both ways what they are.
    ///
    /// # Errors
    /// [`Error::Input`] listing every rule an input breaks.
    fn into_inputs(self) -> Result<Vec<Input>, Error>;
}

impl Inputs for () {
    const DECLARED: &'static [(InputKind, bool)] = &[];

    fn from_inputs(_given: &BTreeMap<String, Input>) -> Result<Self, Error> {
        Ok(())
    }

    fn into_inputs(self) -> Result<Vec<Input>, Error> {
        Ok(Vec::new())
    }
}

/// `Inputs` for the data of one input kind (`$kind`, the variant of [`Input`] holding a `$data`, checked into one by
/// `$checked`), required, and for an `Option` of it, optional.
macro_rules! one_input {
    ($data:ty, $kind:ident, $checked:expr) => {
        impl Inputs for $data {
            const DECLARED: &'static [(InputKind, bool)] = &[(InputKind::$kind, true)];

            fn from_inputs(given: &BTreeMap<String, Input>) -> Result<Self, Error> {
                match given.get(InputKind::$kind.name()) {
                    Some(Input::$kind(data)) => Ok(data.clone()),
                    _ => Err(not_given(InputKind::$kind)),
                }
            }

            fn into_inputs(self) -> Result<Vec<Input>, Error> {
                Ok(vec![$checked(self)?])
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

            fn into_inputs(self) -> Result<Vec<Input>, Error> {
                self.map_or_else(|| Ok(Vec::new()), Inputs::into_inputs)
            }
        }
    };
}

one_input!(Vec<Keyframe>, Points, Input::from_points);
one_input!(Vec<Event>, Events, |events| Ok::<_, Error>(Input::from_events(events)));
one_input!(Grid, Grid, |grid| Ok::<_, Error>(Input::Grid(grid)));
one_input!(Vec<f64>, Values, Input::from_values);
one_input!(Series, Series, |series| Ok::<_, Error>(Input::Series(series)));

/// The error for an input of `kind` that is not given as its kind.
fn not_given(kind: InputKind) -> Error {
    Error::Call {
        message: format!("the input '{kind}' is not given as {kind}"),
    }
}
