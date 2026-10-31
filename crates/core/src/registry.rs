//! The table of sources, shapers and operations: each a [`Thing`] under its name, the rules for getting in, what each
//! says about itself, and the one door through which any of them is called ([`Thing::call`]).
//!
//! ```
//! use bullterrier_control::params::ParamTable;
//! use bullterrier_control::registry::{About, Call, Declared, Kind, Output, Registry, Returns, Thing};
//! use bullterrier_control::Signal;
//!
//! // a thing whose body is a callback: how the Python package registers its own
//! let level = Thing::custom(
//!     About { name: "level".into(), doc: "A signal that holds 0.5.".into(), ..Default::default() },
//!     Declared { kind: Kind::Source, identity: "example.level".into(), table: ParamTable::new(vec![])?,
//!         inputs: vec![], returns: Returns::Signal },
//!     |_call| Ok(Output::Signal(Signal::constant(0.5)?)),
//! )?;
//! let mut registry = Registry::new();
//! registry.register(level)?;
//!
//! let Output::Signal(signal) = registry.get("level")?.call(&Call::default())? else { panic!("a source gives a signal") };
//! assert_eq!(signal.at(3.0)?, 0.5);
//! let refused = registry.get("lvl").map_err(|error| error.to_string());
//! assert_eq!(refused.err().as_deref(), Some("unknown source, shaper or operation 'lvl'; available: level"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Promises:
//! - A registry is a value, not a global: [`Registry::new`] (or a clone of another) is one to register in. Nothing in
//!   it is shared or locked, so it is `Send` and `Sync`, and a call runs no code of the registry's while a thing's
//!   callback runs: a callback may call any thing again.
//! - Getting in, checked once, when a [`Thing`] is made ([`Thing::custom`]) and when it is registered
//!   ([`Registry::register`]); each refusal is an [`Error::Registration`] naming the thing's kind and name:
//!   - a name is lower case letters, digits and `_`, starting with a letter ([`NAME_PATTERN`], [`is_name`]);
//!   - no parameter is named like an input ([`InputKind::NAMES`]) or a key of a patch node ([`NODE_KEYS`]);
//!   - an input is declared once;
//!   - a source and a shaper return a signal, an operation events or a series;
//!   - a name is taken by the thing registered under it: another thing (another identity, or another kind) is
//!     refused, while the same thing registered again replaces itself (a module reloaded).
//! - Calling: see [`Thing::call`]. What a thing says about itself: [`Thing::help`] and [`Thing::description`], in the
//!   words and shape of the Python package's `help()` and `bt-control list --json`.

mod repr;
#[cfg(test)]
mod tests;
mod thing;

use std::collections::BTreeMap;
use std::fmt;

use crate::Error;
use crate::Signal;
use crate::formats::{Event, Series};
use crate::inputs::{Input, InputKind};
use crate::params::ParamTable;

pub(crate) use repr::quoted;
pub use thing::Thing;

/// The pattern a thing's name matches in full (and, in a patch, a node's): lower case letters, digits and `_`,
/// starting with a letter. Without anchors, to be used inside another pattern.
pub const NAME_PATTERN: &str = "[a-z][a-z0-9_]*";

/// The keys of a patch node besides its inputs' and its parameters' names: no parameter may be named like one, or the
/// node `{use = "x", signal = "y", ...}` would be ambiguous.
pub const NODE_KEYS: [&str; 2] = ["use", "signal"];

/// Whether `name` matches [`NAME_PATTERN`] in full.
///
/// ```
/// use bullterrier_control::registry::is_name;
///
/// assert!(is_name("slew") && is_name("every_2nd"));
/// assert!(!is_name("2nd") && !is_name("Slew") && !is_name("") && !is_name("é"));
/// ```
#[must_use]
pub fn is_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|rest| rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '_')
}

/// What a thing is: it makes a signal from nothing ([`Source`](Kind::Source)), a signal from a signal
/// ([`Shaper`](Kind::Shaper)) or data from data ([`Operation`](Kind::Operation)). Each may also take inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Makes a signal from its parameters (and inputs); takes no signal.
    Source,
    /// Makes a signal from one signal and its parameters (and inputs).
    Shaper,
    /// Makes events or a series from its inputs and parameters; takes no signal.
    Operation,
}

impl Kind {
    /// The kind's name, as `help()` and a description write it: `source`, `shaper` or `operation`.
    ///
    /// ```
    /// use bullterrier_control::registry::Kind;
    ///
    /// assert_eq!(Kind::Shaper.name(), "shaper");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Shaper => "shaper",
            Self::Operation => "operation",
        }
    }

    /// How many signals a thing of this kind takes.
    const fn signals(self) -> usize {
        match self {
            Self::Shaper => 1,
            Self::Source | Self::Operation => 0,
        }
    }
}

impl fmt::Display for Kind {
    /// Its [`name`](Kind::name).
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// What a call gives: a signal, events or a series.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Returns {
    /// A [`Signal`]: what a source and a shaper give.
    Signal,
    /// Events, in time order.
    Events,
    /// A [`Series`].
    Series,
}

impl Returns {
    /// Its name, as `help()` and a description write it: `signal`, `events` or `series`.
    ///
    /// ```
    /// use bullterrier_control::registry::Returns;
    ///
    /// assert_eq!(Returns::Events.name(), "events");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Signal => "signal",
            Self::Events => "events",
            Self::Series => "series",
        }
    }
}

impl fmt::Display for Returns {
    /// Its [`name`](Returns::name).
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// What a call gives (see [`Thing::call`]).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Output {
    /// The signal a source or a shaper makes.
    Signal(Signal),
    /// The events an operation makes, in time order.
    Events(Vec<Event>),
    /// The series an operation makes.
    Series(Series),
}

impl Output {
    /// What kind of result this is.
    ///
    /// ```
    /// use bullterrier_control::registry::{Output, Returns};
    ///
    /// assert_eq!(Output::Events(vec![]).returns(), Returns::Events);
    /// ```
    #[must_use]
    pub fn returns(&self) -> Returns {
        match self {
            Self::Signal(_) => Returns::Signal,
            Self::Events(_) => Returns::Events,
            Self::Series(_) => Returns::Series,
        }
    }
}

/// A call of a thing: its input signals (a shaper takes one), its inputs by name, its parameter values by name.
/// `Call::default()` is a call with nothing: no signal, no input and every parameter left at its default.
#[derive(Clone, Debug)]
pub struct Call {
    /// The signals it shapes: one for a shaper, none for a source or an operation.
    pub signals: Vec<Signal>,
    /// Its inputs, each under its kind's name (`events`), already checked ([`InputKind::check`]).
    pub inputs: BTreeMap<String, Input>,
    /// Its parameter values: a JSON object by name; one left out takes its default.
    pub values: serde_json::Value,
}

impl Default for Call {
    fn default() -> Self {
        Self {
            signals: Vec::new(),
            inputs: BTreeMap::new(),
            values: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

/// What a thing is called and says about itself; each field but the name may be empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct About {
    /// The name it is registered and called under (see [`NAME_PATTERN`]).
    pub name: String,
    /// What it does, as `help()` shows it: its first paragraph (up to the first blank line) is its summary.
    pub doc: String,
    /// The name of its parameters: the title of their schema in its description (`SlewParams`).
    pub params_title: String,
    /// What its parameters are: the description of their schema; none when empty.
    pub params_doc: String,
}

/// What a thing whose body is a callback ([`Thing::custom`]) declares; a Rust type says the same by its traits.
#[derive(Clone, Debug)]
pub struct Declared {
    /// Its kind.
    pub kind: Kind,
    /// Who defined it, as the error for a taken name shows it (a Python function's `module.qualname`): registering a
    /// thing of the same kind and identity under its name again replaces it, any other is refused.
    pub identity: String,
    /// Its parameters.
    pub table: ParamTable,
    /// The inputs it takes, each with whether it is required, in the order it declares them.
    pub inputs: Vec<(InputKind, bool)>,
    /// What a call gives.
    pub returns: Returns,
}

/// Things by name: a table to register in and look up.
///
/// ```
/// # use bullterrier_control::params::ParamTable;
/// # use bullterrier_control::registry::{About, Declared, Kind, Output, Registry, Returns, Thing};
/// # use bullterrier_control::Signal;
/// # let empty = ParamTable::new(vec![])?;
/// # let thing = |name: &str, identity: &str| Thing::custom(
/// #     About { name: name.into(), ..Default::default() },
/// #     Declared { kind: Kind::Source, identity: identity.into(), table: empty.clone(),
/// #         inputs: vec![], returns: Returns::Signal },
/// #     |_call| Ok(Output::Signal(Signal::constant(1.0)?)),
/// # );
/// let mut registry = Registry::new();
/// registry.register(thing("level", "a.level")?)?;
/// registry.register(thing("drift", "a.drift")?)?;
/// registry.register(thing("level", "a.level")?)?; // the same thing again: a reload
/// let refused = registry.register(thing("level", "b.level")?).map_err(|error| error.to_string());
/// assert_eq!(refused.err().as_deref(), Some("source 'level': the name is taken by the source defined as a.level"));
///
/// let names: Vec<&str> = registry.things().map(|thing| thing.name()).collect();
/// assert_eq!(names, ["drift", "level"]);
/// registry.unregister("drift")?;
/// assert!(registry.get("drift").is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Default)]
pub struct Registry {
    things: BTreeMap<String, Thing>,
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `thing` under its name.
    ///
    /// # Errors
    /// [`Error::Registration`] if the name is taken by another thing: one of another identity or another kind. The
    /// same thing (its kind and identity) registered again replaces the one registered.
    pub fn register(&mut self, thing: Thing) -> Result<(), Error> {
        if let Some(taken) = self.things.get(thing.name())
            && !taken.is_same(&thing)
        {
            return Err(Error::Registration {
                message: format!(
                    "{}: the name is taken by the {} defined as {}",
                    thing.label(),
                    taken.kind(),
                    taken.identity()
                ),
            });
        }
        self.things.insert(thing.name().to_owned(), thing);
        Ok(())
    }

    /// The thing called `name`.
    ///
    /// # Errors
    /// [`Error::UnknownName`] listing the names registered, if none is called `name`.
    pub fn get(&self, name: &str) -> Result<&Thing, Error> {
        self.things.get(name).ok_or_else(|| self.unknown(name))
    }

    /// Every thing, sorted by name.
    pub fn things(&self) -> impl Iterator<Item = &Thing> {
        self.things.values()
    }

    /// Removes the thing called `name` and gives it back.
    ///
    /// # Errors
    /// [`Error::UnknownName`] listing the names registered, if none is called `name`.
    pub fn unregister(&mut self, name: &str) -> Result<Thing, Error> {
        match self.things.remove(name) {
            Some(thing) => Ok(thing),
            None => Err(self.unknown(name)),
        }
    }

    /// The error for `name`, which is not registered.
    fn unknown(&self, name: &str) -> Error {
        Error::UnknownName {
            name: name.to_owned(),
            available: self.things.keys().cloned().collect(),
        }
    }
}
