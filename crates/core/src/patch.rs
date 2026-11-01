//! A patch: a named graph of sources, shapers and operations with their values, as data (`docs/formats.md`,
//! "Patch"), read from TOML and built against a [`Bundle`].
//!
//! ```toml
//! format = "bullterrier-control/patch"
//! version = 1
//!
//! [smooth]
//! use = "slew"
//! signal = "series.energy"   # the bundle's series "energy", read linearly
//! rise = 0.5
//!
//! [glow]
//! use = "follow"
//! signal = "smooth"          # the node above
//! attack = 0.1
//! ```
//!
//! ```
//! use bullterrier_control::formats::Bundle;
//! use bullterrier_control::registry::Output;
//! use bullterrier_control::Patch;
//!
//! let text = "format = 'bullterrier-control/patch'\nversion = 1\n\n\
//!             [smooth]\nuse = 'slew'\nsignal = 'series.energy'\nrise = 0.5\nrate = 10\n";
//! let bundle = Bundle::from_json(r#"{"format": "bullterrier-control/bundle", "version": 1,
//!     "series": {"energy": {"rate": 10, "values": [0, 1, 1, 1], "unit": "ratio"}}}"#)?;
//! let built = Patch::from_toml(text)?.build(&bundle)?;
//! let Some(Output::Signal(smooth)) = built.get("smooth") else { panic!("a shaper gives a signal") };
//! // 0.1 s a sample, 0.5 s to climb by 1: 0.2 a sample
//! assert_eq!(smooth.sample(&[0.0, 0.15, 0.25])?, [0.0, 0.2, 0.4]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! A patch is a look of signals: it names what to make and with which values, never what the signals drive. The bundle
//! is not named in it: one patch serves every song, and the song is given when the patch is built.
//!
//! Promises:
//! - A patch is a table with the header `format` and `version` (both required), then one table per node. A node has
//!   `use` (the name of a registered source, shaper or operation), `signal` (a shaper's input; refused for the
//!   others), one key for each input it takes and one for each parameter it declares. Anything else is refused: a
//!   patch cannot say what a signal drives.
//! - A node's input is data written inline (a list of events, of keyframes, a grid, a series) or a *reference*:
//!   `events.<name>` and `series.<name>` are entries of the bundle, `grid` is its grid, and any other name is a node
//!   written above. A reference must fit what the key takes: events for `events`, a series for `series`, the grid for
//!   `grid`, and for `signal` a signal or a series (a series is read linearly). `points` and `values` are written
//!   inline.
//! - Reading ([`Patch::from_toml`], [`Patch::parse`]) checks, all at once, everything the patch and the registry
//!   decide: names, keys, types, ranges and references. Every problem is reported together as one
//!   [`PatchError::Invalid`] listing each as `<node>.<key>: rule` (the first 100, and how many more). Two things wait
//!   for building: the names a bundle must hold, and a rule a thing applies when it is called, which is its own.
//! - Building ([`Patch::build`]) gives every node by name, in file order ([`Built`]): a signal for a source or a
//!   shaper, the events or the series for an operation ([`Patch::build_only`] asks for some, building just those and
//!   what they use). The same patch and bundle give the same values. A node's [`returns`](Node::returns) says which,
//!   without building.
//! - A patch is built from the things it was read with: what a node names is looked up once, when it is read.
//! - Saving ([`Patch::from_signals`]) makes a patch of signals made through the registry, each signal one node, and
//!   refuses what a patch cannot hold, at its path; [`Patch::to_toml`] writes any patch as text that reads back as it
//!   and builds the same values.
//! - Nothing here depends on a consumer: a patch held inside another file (a video project) is read with
//!   [`Patch::parse`].

mod build;
#[cfg(test)]
mod privacy;
mod read;
mod repr;
mod save;
#[cfg(test)]
mod tests;
mod text;

use std::fmt;

use serde_json::{Map, Value, json};

use crate::Error;
use crate::Signal;
use crate::formats::Bundle;
use crate::formats::Problem;
use crate::inputs::Input;
use crate::inputs::InputKind;
use crate::registry::NAME_PATTERN;
use crate::registry::Output;
use crate::registry::Registry;
use crate::registry::Returns;
use crate::registry::Thing;
use crate::registry::is_name;

/// The `format` of every patch.
const FORMAT: &str = "bullterrier-control/patch";
/// The one `version` of a patch this reader knows.
const VERSION: u32 = 1;
/// The names no node may have: `format` and `version` are the header, and `events`, `series` and `grid` start the
/// paths into a bundle.
const RESERVED: [&str; 5] = ["format", "version", "events", "series", "grid"];

/// Whether `name` may name a node: a thing's name ([`NAME_PATTERN`]) that is not [`RESERVED`].
fn is_node_name(name: &str) -> bool {
    is_name(name) && !RESERVED.contains(&name)
}

/// The rule a name that is not [`is_node_name`] breaks.
fn not_a_node_name() -> String {
    format!(
        "is not a node name: lower case letters, digits and _, starting with a letter, and not one of {}",
        RESERVED.join(", ")
    )
}

/// The nodes of a patch by name, in file order: what each uses and what it is built from. Read with
/// [`Patch::from_toml`] or [`Patch::parse`]; two patches are equal when their nodes name the same things with the same
/// references, inputs and values.
///
/// ```
/// use bullterrier_control::Patch;
/// use serde_json::json;
///
/// let text = "format = 'bullterrier-control/patch'\nversion = 1\n[smooth]\nuse = 'slew'\nsignal = 'series.energy'\n";
/// let entries = [
///     ("format".to_owned(), json!("bullterrier-control/patch")),
///     ("version".to_owned(), json!(1)),
///     ("smooth".to_owned(), json!({"use": "slew", "signal": "series.energy"})),
/// ];
/// assert_eq!(Patch::from_toml(text)?, Patch::parse(&entries)?);
/// # Ok::<(), bullterrier_control::patch::PatchError>(())
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    nodes: Vec<(String, Node)>,
}

/// One node as the patch says it: the thing it uses and what it is built from.
///
/// ```
/// use bullterrier_control::Patch;
/// use bullterrier_control::registry::Returns;
///
/// let patch = Patch::from_toml("format = 'bullterrier-control/patch'\nversion = 1\n[smooth]\nuse = 'slew'\nsignal = 'series.energy'\n")?;
/// let (name, node) = patch.nodes().next().ok_or("one node")?;
/// assert_eq!((name, node.uses(), node.returns()), ("smooth", "slew", Returns::Signal));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug)]
pub struct Node {
    /// The thing it uses, as the registry it was read with holds it.
    thing: Thing,
    /// The signal it shapes: a shaper's; `None` for any other thing.
    signal: Option<Reference>,
    /// Its inputs, each written inline or a reference, in the order the thing declares them.
    inputs: Vec<(InputKind, Given)>,
    /// Its parameter values as written (those left out take their defaults when it is built), each valid.
    values: Map<String, Value>,
}

impl PartialEq for Node {
    /// The same thing by name, the same references, inputs and values.
    fn eq(&self, other: &Self) -> bool {
        self.uses() == other.uses()
            && self.signal == other.signal
            && self.inputs == other.inputs
            && self.values == other.values
    }
}

impl Node {
    /// The name of the source, shaper or operation it uses: its `use`.
    #[must_use]
    pub fn uses(&self) -> &str {
        self.thing.name()
    }

    /// What it gives when built: a signal, events or a series.
    #[must_use]
    pub fn returns(&self) -> Returns {
        self.thing.returns()
    }

    /// The signal it shapes, as it writes it: a shaper's `signal`; `None` for a source or an operation.
    ///
    /// ```
    /// use bullterrier_control::Patch;
    /// use bullterrier_control::patch::Reference;
    ///
    /// let text = "format = 'bullterrier-control/patch'\nversion = 1\n[smooth]\nuse = 'slew'\nsignal = 'series.energy'\n";
    /// let patch = Patch::from_toml(text)?;
    /// let (_, node) = patch.nodes().next().ok_or("one node")?;
    /// assert_eq!(node.signal(), Some(&Reference::Series("energy".to_owned())));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn signal(&self) -> Option<&Reference> {
        self.signal.as_ref()
    }

    /// Its inputs, each by kind with what is given for it, in the order its thing declares them; an input left out is
    /// not listed.
    ///
    /// ```
    /// use bullterrier_control::Patch;
    /// use bullterrier_control::inputs::{Input, InputKind};
    /// use bullterrier_control::patch::{Given, Reference};
    ///
    /// let text = "format = 'bullterrier-control/patch'\nversion = 1\n\
    ///             [pulse]\nuse = 'select'\nevents = 'events.kick'\n";
    /// let patch = Patch::from_toml(text)?;
    /// let (_, node) = patch.nodes().next().ok_or("one node")?;
    /// let inputs: Vec<(InputKind, &Given)> = node.inputs().collect();
    /// assert_eq!(inputs, [(InputKind::Events, &Given::Reference(Reference::Events("kick".to_owned())))]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn inputs(&self) -> impl Iterator<Item = (InputKind, &Given)> {
        self.inputs.iter().map(|(kind, given)| (*kind, given))
    }

    /// Its parameter values as written, by name (those left out take their defaults when it is built).
    ///
    /// ```
    /// use bullterrier_control::Patch;
    /// use serde_json::json;
    ///
    /// let text = "format = 'bullterrier-control/patch'\nversion = 1\n\
    ///             [smooth]\nuse = 'slew'\nsignal = 'series.energy'\nrise = 0.5\n";
    /// let patch = Patch::from_toml(text)?;
    /// let (_, node) = patch.nodes().next().ok_or("one node")?;
    /// assert_eq!(node.values().get("rise"), Some(&json!(0.5)));
    /// assert_eq!(node.values().len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn values(&self) -> &Map<String, Value> {
        &self.values
    }
}

/// An input as a node writes it: the data itself, or a reference to where it is.
#[derive(Clone, Debug, PartialEq)]
pub enum Given {
    /// The data, checked as its kind.
    Data(Input),
    /// Where the data is.
    Reference(Reference),
}

/// A name written where data goes: a node above, or an entry of the bundle. Its [`Display`](fmt::Display) is how a
/// patch writes it: `glow`, `events.kick`, `series.energy`, `grid`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reference {
    /// The node of this name, above the one that refers to it.
    Node(String),
    /// The bundle's events of this name (`events.<name>`).
    Events(String),
    /// The bundle's series of this name (`series.<name>`).
    Series(String),
    /// The bundle's grid (`grid`).
    Grid,
}

impl fmt::Display for Reference {
    /// The reference as a patch writes it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(name) => formatter.write_str(name),
            Self::Events(name) => write!(formatter, "events.{name}"),
            Self::Series(name) => write!(formatter, "series.{name}"),
            Self::Grid => formatter.write_str("grid"),
        }
    }
}

impl Patch {
    /// The patch in the TOML `text`, its things looked up in [`Registry::builtin`].
    ///
    /// ```
    /// use bullterrier_control::Patch;
    ///
    /// let patch = Patch::from_toml("format = 'bullterrier-control/patch'\nversion = 1\n")?;
    /// assert_eq!(patch.nodes().count(), 0);
    /// let refused = Patch::from_toml("format = \n").map_err(|error| error.to_string());
    /// assert!(refused.is_err_and(|text| text.starts_with("not TOML (line 1, column 10: ")));
    /// # Ok::<(), bullterrier_control::patch::PatchError>(())
    /// ```
    ///
    /// # Errors
    /// [`PatchError::Toml`] if `text` is not TOML (a repeated key or table, nesting deeper than the parser follows),
    /// with the parser's words, line and column; else as [`Patch::parse`]. A value TOML holds and a patch cannot (a
    /// date, a number that is not finite, a whole number beyond 64 bits) is a problem at its key among the others.
    pub fn from_toml(text: &str) -> Result<Self, PatchError> {
        Self::from_toml_with(Registry::builtin(), text)
    }

    /// The patch in the TOML `text`, its things looked up in `registry`: as [`Patch::from_toml`].
    ///
    /// ```
    /// use bullterrier_control::{Patch, Registry};
    ///
    /// let text = "format = 'bullterrier-control/patch'\nversion = 1\n[smooth]\nuse = 'slew'\nsignal = 'series.energy'\n";
    /// let refused = Patch::from_toml_with(&Registry::new(), text).map_err(|error| error.to_string());
    /// assert_eq!(refused.err().as_deref(), Some("Patch: smooth.use: unknown source, shaper or operation 'slew'; available: "));
    /// # Ok::<(), bullterrier_control::patch::PatchError>(())
    /// ```
    ///
    /// # Errors
    /// As [`Patch::from_toml`].
    pub fn from_toml_with(registry: &Registry, text: &str) -> Result<Self, PatchError> {
        let decoded = text::decode(text)?;
        read::read(registry, &decoded.entries, decoded.spoiled)
    }

    /// The patch in `entries`, a decoded table's keys and values in the order they are written (the header and each
    /// node by name), its things looked up in [`Registry::builtin`].
    ///
    /// ```
    /// use bullterrier_control::Patch;
    /// use serde_json::json;
    ///
    /// let entries = [("format".to_owned(), json!("bullterrier-control/patch")), ("version".to_owned(), json!(2))];
    /// let refused = Patch::parse(&entries).map_err(|error| error.to_string());
    /// assert_eq!(refused.err().as_deref(),
    ///     Some("Patch: version: must be the whole number 1, the one version this reader knows (got 2)"));
    /// # Ok::<(), bullterrier_control::patch::PatchError>(())
    /// ```
    ///
    /// # Errors
    /// [`PatchError::Invalid`] listing every problem as `<node>.<key>: rule` (see [the module](self)).
    pub fn parse(entries: &[(String, Value)]) -> Result<Self, PatchError> {
        Self::parse_with(Registry::builtin(), entries)
    }

    /// The patch in `entries`, its things looked up in `registry`: as [`Patch::parse`].
    ///
    /// ```
    /// use bullterrier_control::{Patch, Registry};
    /// use serde_json::json;
    ///
    /// let entries = [("format".to_owned(), json!("bullterrier-control/patch")), ("version".to_owned(), json!(1))];
    /// assert_eq!(Patch::parse_with(&Registry::new(), &entries)?.nodes().count(), 0);
    /// # Ok::<(), bullterrier_control::patch::PatchError>(())
    /// ```
    ///
    /// # Errors
    /// As [`Patch::parse`].
    pub fn parse_with(registry: &Registry, entries: &[(String, Value)]) -> Result<Self, PatchError> {
        read::read(registry, entries, Vec::new())
    }

    /// Its nodes by name, in file order.
    ///
    /// ```
    /// use bullterrier_control::Patch;
    ///
    /// let text = "format = 'bullterrier-control/patch'\nversion = 1\n\
    ///             [b]\nuse = 'slew'\nsignal = 'series.energy'\n[a]\nuse = 'follow'\nsignal = 'b'\n";
    /// let patch = Patch::from_toml(text)?;
    /// let names: Vec<&str> = patch.nodes().map(|(name, _)| name).collect();
    /// assert_eq!(names, ["b", "a"]);
    /// # Ok::<(), bullterrier_control::patch::PatchError>(())
    /// ```
    pub fn nodes(&self) -> impl Iterator<Item = (&str, &Node)> {
        self.nodes.iter().map(|(name, node)| (name.as_str(), node))
    }

    /// Every node built against `bundle` (`None` when the patch names nothing in one), by name in file order.
    ///
    /// ```
    /// use bullterrier_control::Patch;
    ///
    /// let patch = Patch::from_toml("format = 'bullterrier-control/patch'\nversion = 1\n[smooth]\nuse = 'slew'\nsignal = 'series.energy'\n")?;
    /// let refused = patch.build(None).map_err(|error| error.to_string());
    /// assert_eq!(refused.err().as_deref(), Some("smooth.signal: 'series.energy' is a path into a bundle, and none was given"));
    /// # Ok::<(), bullterrier_control::patch::PatchError>(())
    /// ```
    ///
    /// # Errors
    /// [`PatchError::Bundle`] at `<node>.<key>` for a path into the bundle that it does not hold (naming what it
    /// holds), or any path when there is no bundle; [`PatchError::Node`] naming the node, with its thing's error
    /// unchanged, if a thing refuses its call or fails.
    pub fn build<'a>(&self, bundle: impl Into<Option<&'a Bundle>>) -> Result<Built, PatchError> {
        build::build(self, bundle.into(), None)
    }

    /// The nodes called `names` built against `bundle`, by name in file order: only they and the nodes they use are
    /// built, so the rest of the patch needs nothing of the bundle.
    ///
    /// ```
    /// use bullterrier_control::Patch;
    ///
    /// let text = "format = 'bullterrier-control/patch'\nversion = 1\n\
    ///             [smooth]\nuse = 'slew'\nsignal = 'series.energy'\n[glow]\nuse = 'follow'\nsignal = 'smooth'\n";
    /// let patch = Patch::from_toml(text)?;
    /// assert!(patch.build_only(None, &[])?.is_empty());
    /// let refused = patch.build_only(None, &["glow"]).map_err(|error| error.to_string()); // `smooth` needs a bundle
    /// assert!(refused.is_err_and(|text| text.starts_with("smooth.signal: ")));
    /// let unknown = patch.build_only(None, &["halo"]).map_err(|error| error.to_string());
    /// assert_eq!(unknown.err().as_deref(), Some("unknown node 'halo'; the patch has: smooth, glow"));
    /// # Ok::<(), bullterrier_control::patch::PatchError>(())
    /// ```
    ///
    /// # Errors
    /// [`PatchError::UnknownNode`] for the first of `names` the patch has no node of; else as [`Patch::build`].
    pub fn build_only<'a>(&self, bundle: impl Into<Option<&'a Bundle>>, names: &[&str]) -> Result<Built, PatchError> {
        build::build(self, bundle.into(), Some(names))
    }

    /// The patch of `signals`, each under its name, for [`Registry::builtin`]: as [`Patch::from_signals_with`].
    ///
    /// ```
    /// use bullterrier_control::builtin::{constant, follow, slew};
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::{Patch, lib};
    ///
    /// let level = lib::constant(constant::Params { value: 0.5 })?;
    /// let glow = level.slew(slew::Params::default())?.follow(follow::Params::default())?;
    /// let text = Patch::from_signals(&[("glow", &glow)])?.to_toml();
    /// assert!(text.contains("\n[glow_2]\nuse = \"slew\"\nsignal = \"glow_1\"\n"));
    ///
    /// let built = Patch::from_toml(&text)?.build(None)?;
    /// let Some(Output::Signal(again)) = built.get("glow") else { panic!("a shaper gives a signal") };
    /// assert_eq!(again.sample(&[0.0, 0.05, 2.0])?, glow.sample(&[0.0, 0.05, 2.0])?);
    ///
    /// let refused = Patch::from_signals(&[("mix", &(&glow * 2.0))]).map_err(|error| error.to_string());
    /// assert!(refused.is_err_and(|text| text.starts_with("cannot save as a patch: mix: is made by an operator")));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// As [`Patch::from_signals_with`].
    pub fn from_signals(signals: &[(&str, &Signal)]) -> Result<Self, PatchError> {
        Self::from_signals_with(Registry::builtin(), signals)
    }

    /// The patch of `signals`, each under its name, for `registry`: a node for each, and one for every signal it is
    /// made from that is not given itself. What a node holds is its signal's [`recipe`](Signal::recipe): the thing,
    /// every parameter value as its table read it, its inputs written inline, and the node above it as its `signal`.
    ///
    /// - The nodes are in an order a patch reads: each signal below every signal it is made from, the signals given
    ///   in the order given otherwise. A signal several signals are made from is one node, which each refers to; a
    ///   signal given under two names is a node under each.
    /// - A signal above that is not given is named after the signal given that it is first found above, numbered from
    ///   the top: `glow_1`, `glow_2`, ... above `glow`. A made name is none that is given, nor made already.
    /// - [`to_toml`](Self::to_toml) writes it as text that reads back with `registry` as the same patch, and the
    ///   patch builds signals of the same values as `signals`, bit for bit (it needs no bundle: its data is inline).
    ///
    /// ```
    /// use bullterrier_control::builtin::{constant, slew};
    /// use bullterrier_control::{Patch, Registry, lib};
    ///
    /// let base = lib::constant(constant::Params { value: 0.25 })?.slew(slew::Params::default())?;
    /// let fast = base.slew(slew::Params { rise: 0.01, ..Default::default() })?;
    /// let slow = base.slew(slew::Params { rise: 1.0, ..Default::default() })?;
    /// let saved = Patch::from_signals(&[("fast", &fast), ("slow", &slow)])?;
    /// let names: Vec<&str> = saved.nodes().map(|(name, _)| name).collect();
    /// assert_eq!(names, ["fast_1", "fast_2", "fast", "slow"]); // `slow` uses `fast_2` too
    ///
    /// let refused = Patch::from_signals_with(&Registry::new(), &[("fast", &fast)]).map_err(|error| error.to_string());
    /// assert!(refused.is_err_and(|text| text.contains("fast: is made by 'slew', which the registry it is saved for does not hold")));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// [`PatchError::Unsaved`] listing, each at its path (`glow`, `glow.signal` for the signal it shapes, and so on
    /// up), every signal given or above one that a patch cannot hold, with why: it is made by an operator, from
    /// values held as they are (a constant, [`from_values`](Signal::from_values), [`from_series`](Signal::from_series)),
    /// by a callback ([`from_fn`](Signal::from_fn)) or by a recurrence of one ([`rendered`](Signal::rendered)), or
    /// by a thing other than the one `registry` holds under its name (or `registry` holds none). Also each name that
    /// is no node name, or is given twice.
    pub fn from_signals_with(registry: &Registry, signals: &[(&str, &Signal)]) -> Result<Self, PatchError> {
        save::save(registry, signals)
    }

    /// The patch as TOML text, which reads back (with the registry it was read with or saved for) as the same patch:
    /// the header, then each node in file order, a blank line before each: `use`, `signal`, its inputs in the order
    /// its thing declares them (a reference, or the data inline as a bundle writes it), then its parameters in the
    /// order its table declares them. A number is written as it is held (a decimal as a decimal, a whole number as a
    /// whole one), each decimal as the same bits; text and a key are quoted and escaped as TOML needs. The same patch
    /// gives the same text.
    ///
    /// ```
    /// use bullterrier_control::Patch;
    ///
    /// let text = "format = 'bullterrier-control/patch'\nversion = 1\n[smooth]\nsignal = 'series.energy'\nuse = 'slew'\nrise = 0.5\n";
    /// let written = Patch::from_toml(text)?.to_toml();
    /// assert_eq!(written, "format = \"bullterrier-control/patch\"\nversion = 1\n\n\
    ///                      [smooth]\nuse = \"slew\"\nsignal = \"series.energy\"\nrise = 0.5\n");
    /// assert_eq!(Patch::from_toml(&written)?, Patch::from_toml(text)?);
    /// # Ok::<(), bullterrier_control::patch::PatchError>(())
    /// ```
    #[must_use]
    pub fn to_toml(&self) -> String {
        text::write(self)
    }
}

/// What a patch built: each node's result by name, in file order.
///
/// ```
/// use bullterrier_control::Patch;
/// use bullterrier_control::formats::Bundle;
///
/// let text = "format = 'bullterrier-control/patch'\nversion = 1\n\
///             [smooth]\nuse = 'slew'\nsignal = 'series.energy'\n[glow]\nuse = 'follow'\nsignal = 'smooth'\n";
/// let bundle = Bundle::from_json(r#"{"format": "bullterrier-control/bundle", "version": 1,
///     "series": {"energy": {"rate": 10, "values": [0, 1], "unit": "ratio"}}}"#)?;
/// let built = Patch::from_toml(text)?.build(&bundle)?;
/// let names: Vec<&str> = built.iter().map(|(name, _)| name).collect();
/// assert_eq!((names, built.len(), built.is_empty()), (vec!["smooth", "glow"], 2, false));
/// assert!(built.get("glow").is_some() && built.get("halo").is_none());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Default)]
pub struct Built {
    made: Vec<(String, Output)>,
}

impl Built {
    /// What the node called `name` gave, if it was built.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Output> {
        self.made
            .iter()
            .find_map(|(made, output)| (made == name).then_some(output))
    }

    /// Each node's name and result, in file order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Output)> {
        self.made.iter().map(|(name, output)| (name.as_str(), output))
    }

    /// How many nodes were built and given back.
    #[must_use]
    pub fn len(&self) -> usize {
        self.made.len()
    }

    /// Whether no node was given back.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.made.is_empty()
    }
}

impl IntoIterator for Built {
    type Item = (String, Output);
    type IntoIter = std::vec::IntoIter<(String, Output)>;

    /// Each node's name and result, in file order.
    fn into_iter(self) -> Self::IntoIter {
        self.made.into_iter()
    }
}

/// Why a patch could not be read or built.
///
/// ```
/// use bullterrier_control::Patch;
/// use bullterrier_control::patch::PatchError;
///
/// let text = "format = 'bullterrier-control/patch'\nversion = 1\n[Glow]\nuse = 'slew'\n";
/// let Err(PatchError::Invalid { problems, omitted }) = Patch::from_toml(text) else { panic!("refused") };
/// let paths: Vec<&str> = problems.iter().map(|problem| problem.path.as_str()).collect();
/// assert_eq!((paths, omitted), (vec!["Glow", "Glow.signal"], 0));
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum PatchError {
    /// The text is not TOML: `not TOML (line L, column C: what the parser says)`.
    Toml {
        /// The parser's words, after the line and column where it stopped.
        detail: String,
    },
    /// The patch breaks rules: `Patch: <node>.<key>: rule; ...`, every problem found.
    Invalid {
        /// Each broken rule at `<node>.<key>` (a header key, or a node's name, alone), in file order: the first 100.
        problems: Vec<Problem>,
        /// How many more problems there were than `problems` holds.
        omitted: usize,
    },
    /// Building: a path into the bundle that it cannot serve (or any path when there is no bundle), at
    /// `<node>.<key>`.
    Bundle {
        /// The node and key, and what the bundle lacks.
        problem: Problem,
    },
    /// Building some nodes: one asked for that the patch does not have.
    UnknownNode {
        /// The name asked for.
        name: String,
        /// The patch's nodes, in file order.
        nodes: Vec<String>,
    },
    /// Saving: signals that a patch cannot hold, or names that no node may have, each at its path.
    Unsaved {
        /// Each signal refused at its path, or a name refused alone, in the order given: the first 100.
        problems: Vec<Problem>,
        /// How many more problems there were than `problems` holds.
        omitted: usize,
    },
    /// Building: the thing of a node refused its call or failed.
    Node {
        /// The node's name.
        node: String,
        /// The thing's error, unchanged.
        source: Error,
    },
}

impl fmt::Display for PatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml { detail } => write!(formatter, "not TOML ({detail})"),
            Self::Invalid { problems, omitted } => listing(formatter, "Patch: ", problems, *omitted),
            Self::Unsaved { problems, omitted } => listing(formatter, "cannot save as a patch: ", problems, *omitted),
            Self::Bundle { problem } => write!(formatter, "{problem}"),
            Self::UnknownNode { name, nodes } => {
                let has = if nodes.is_empty() {
                    "none".to_owned()
                } else {
                    nodes.join(", ")
                };
                write!(formatter, "unknown node {}; the patch has: {has}", repr::quoted(name))
            }
            Self::Node { node, source } => write!(formatter, "{node}: {source}"),
        }
    }
}

/// `problems` after `title`, joined by `; `, and how many more there were.
fn listing(formatter: &mut fmt::Formatter<'_>, title: &str, problems: &[Problem], omitted: usize) -> fmt::Result {
    formatter.write_str(title)?;
    for (index, problem) in problems.iter().enumerate() {
        if index > 0 {
            formatter.write_str("; ")?;
        }
        write!(formatter, "{problem}")?;
    }
    match omitted {
        0 => Ok(()),
        omitted => write!(formatter, "; ... and {omitted} more"),
    }
}

impl std::error::Error for PatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Node { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// The JSON Schema of a patch (as decoded from TOML), for tools in other languages. It states the header, the shape
/// of a node and its name; what a schema cannot state is enforced by reading ([`Patch::parse`]): that `use` names a
/// registered thing, each key an input or parameter of it, the values, and that a reference fits and names a node
/// above.
///
/// ```
/// let schema = bullterrier_control::patch::patch_schema();
/// assert_eq!(schema["properties"]["format"]["const"], "bullterrier-control/patch");
/// assert_eq!(schema["$defs"]["node"]["required"], serde_json::json!(["use"]));
/// ```
#[must_use]
pub fn patch_schema() -> Value {
    let node_name = format!("^(?!({})$){NAME_PATTERN}$", RESERVED.join("|"));
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Patch",
        "type": "object",
        "required": ["format", "version"],
        "properties": {"format": {"const": FORMAT}, "version": {"const": VERSION}},
        "patternProperties": {node_name: {"$ref": "#/$defs/node"}},
        "additionalProperties": false,
        "$defs": {
            "node": {
                "type": "object",
                "required": ["use"],
                "properties": {
                    "use": {"type": "string", "pattern": format!("^{NAME_PATTERN}$")},
                    "signal": {"type": "string"},
                },
                "additionalProperties": {"type": ["number", "string", "boolean", "array", "object"]},
            }
        },
    })
}
