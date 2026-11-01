//! [`Thing`]: one source, shaper or operation, its rules checked, what it says about itself, and its call.

use std::any::TypeId;
use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use serde_json::{Map, Value, json};

use super::repr::{python, quoted};
use super::{
    About, Call, Declared, Inputs, Kind, NODE_KEYS, Operation, Output, Recipe, Returns, Shaper, Source, is_name,
};
use crate::formats::Event;
use crate::inputs::InputKind;
use crate::params::{ParamTable, Values};
use crate::signal::callback;
use crate::{Error, Params, Signal};

/// The body of a thing: called with a checked call and its parameters as the table read them.
type Run = dyn Fn(&Call, &Values) -> Result<Output, Error> + Send + Sync;

/// A registered source, shaper or operation: what it is, takes and gives, what it says about itself, and its call.
/// Cheap to clone (it is shared), `Send` and `Sync`.
///
/// ```
/// use bullterrier_control::registry::{Kind, Registry, Returns};
///
/// let slew = Registry::builtin().get("slew")?;
/// assert_eq!((slew.name(), slew.kind(), slew.returns()), ("slew", Kind::Shaper, Returns::Signal));
/// assert!(slew.inputs().is_empty());
/// let names: Vec<&str> = slew.table().params().iter().map(|param| param.name.as_str()).collect();
/// assert_eq!(names, ["rise", "fall", "rate"]);
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
#[derive(Clone)]
pub struct Thing(Arc<Inner>);

struct Inner {
    name: Cow<'static, str>,
    about: About,
    summary: String,
    kind: Kind,
    identity: Identity,
    table: Cow<'static, ParamTable>,
    inputs: Vec<(InputKind, bool)>,
    returns: Returns,
    run: Box<Run>,
}

/// Who defined a thing: a Rust type (by its [`TypeId`]; its path only for messages, as a path need not be unique) or
/// a callback's own identity ([`Declared::identity`]). Two identities are the same only if both are a type and the
/// same type, or both are a callback's and the same text: a callback never passes for a Rust type.
#[derive(Clone, Debug)]
pub(super) enum Identity {
    /// A Rust type, and its path.
    Type(TypeId, &'static str),
    /// A callback's identity, and how to show it if not as itself ([`Declared::shown`]).
    Defined(String, Option<String>),
}

impl Identity {
    /// The identity of the Rust type `T`.
    fn of<T: 'static>() -> Self {
        Self::Type(TypeId::of::<T>(), std::any::type_name::<T>())
    }
}

impl PartialEq for Identity {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Type(one, _), Self::Type(other, _)) => one == other,
            (Self::Defined(one, _), Self::Defined(other, _)) => one == other,
            _ => false,
        }
    }
}

impl fmt::Display for Identity {
    /// The type's path, or the callback's identity.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Type(_, path) => formatter.write_str(path),
            Self::Defined(identity, shown) => formatter.write_str(shown.as_ref().unwrap_or(identity)),
        }
    }
}

/// What a thing is made of before its rules are checked ([`Thing::checked`]).
struct Parts {
    name: Cow<'static, str>,
    kind: Kind,
    identity: Identity,
    table: Cow<'static, ParamTable>,
    inputs: Vec<(InputKind, bool)>,
    returns: Returns,
}

impl fmt::Debug for Thing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Thing")
            .field("name", &self.0.name)
            .field("kind", &self.0.kind)
            .finish_non_exhaustive()
    }
}

impl Thing {
    /// The source `T`, under its [`NAME`](Source::NAME), with the doc `about` gives: its parameters are `T`'s table,
    /// its identity is `T`'s type, and a call gives the signal [`Source::source`] makes of the values read into a `T`.
    ///
    /// ```
    /// use bullterrier_control::registry::{About, Call, Output, Thing};
    /// use bullterrier_control::{Error, Params, Signal, Source};
    /// use serde_json::json;
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
    ///
    ///     fn source(&self) -> Result<Signal, Error> {
    ///         Signal::constant(self.value)
    ///     }
    /// }
    ///
    /// let level = Thing::source::<Level>(About { doc: "A level that holds.".into(), ..Default::default() })?;
    /// let call = Call { values: json!({"value": -0.25}), ..Default::default() };
    /// let Output::Signal(signal) = level.call(&call)? else { panic!("a source gives a signal") };
    /// assert_eq!((level.name(), signal.at(1.0)?), ("level", -0.25));
    /// # Ok::<(), Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Registration`] if the thing breaks a rule of the [registry](super), [`Error::Params`] if `T`'s
    /// declaration breaks a rule of its table.
    pub fn source<T: Source>(about: About) -> Result<Self, Error> {
        let parts = Self::parts::<T>(T::NAME, Kind::Source, Vec::new(), Returns::Signal)?;
        Self::checked(
            about,
            parts,
            Box::new(|_, values| Ok(Output::Signal(T::from_values(values).source()?))),
        )
    }

    /// The shaper `T`, under its [`NAME`](Shaper::NAME), with the doc `about` gives: its parameters are `T`'s table,
    /// its identity is `T`'s type, and a call gives the signal [`Shaper::shape`] makes of its signal with the values
    /// read into a `T`.
    ///
    /// ```
    /// use bullterrier_control::builtin::slew;
    /// use bullterrier_control::registry::{About, Kind, Thing};
    ///
    /// let mine = Thing::shaper::<slew::Params>(About { doc: "Smooths it.".into(), ..Default::default() })?;
    /// assert_eq!((mine.name(), mine.kind(), mine.summary()), ("slew", Kind::Shaper, "Smooths it."));
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Thing::source`].
    pub fn shaper<T: Shaper>(about: About) -> Result<Self, Error> {
        let parts = Self::parts::<T>(T::NAME, Kind::Shaper, Vec::new(), Returns::Signal)?;
        Self::checked(
            about,
            parts,
            Box::new(|call, values| match call.signals.as_slice() {
                [input] => Ok(Output::Signal(T::from_values(values).shape(input)?)),
                // `call` checked the count already
                _ => Err(Error::Call {
                    message: "a shaper takes 1 signal".to_owned(),
                }),
            }),
        )
    }

    /// The operation `T`, under its [`NAME`](Operation::NAME), with the doc `about` gives: its parameters are `T`'s
    /// table, its inputs [`Operation::Inputs`]'s, what it gives [`Operation::RETURNS`], its identity `T`'s type; a
    /// call gives what [`Operation::run`] makes of its inputs with the values read into a `T`.
    ///
    /// # Errors
    /// As [`Thing::source`].
    pub fn operation<T: Operation>(about: About) -> Result<Self, Error> {
        let parts = Self::parts::<T>(T::NAME, Kind::Operation, T::Inputs::DECLARED.to_vec(), T::RETURNS)?;
        Self::checked(
            about,
            parts,
            Box::new(|call, values| T::from_values(values).run(T::Inputs::from_inputs(&call.inputs)?)),
        )
    }

    /// The thing `declared` says, with the doc `about` gives, whose body is `run`: how the Python package registers a
    /// thing whose body is Python. `run` is called with the call as [`Thing::call`] checked it, as a callback: one
    /// running inside too many others on the thread (bodies, `Signal::from_fn`s, recurrences) is refused instead.
    ///
    /// # Errors
    /// [`Error::Registration`] if the thing breaks a rule of the [registry](super) (its name, a parameter's name, an
    /// input declared twice, what its kind returns).
    pub fn custom(
        about: About,
        declared: Declared,
        run: impl Fn(&Call) -> Result<Output, Error> + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let Declared {
            name,
            kind,
            identity,
            shown,
            table,
            inputs,
            returns,
        } = declared;
        let parts = Parts {
            name: Cow::Owned(name),
            kind,
            identity: Identity::Defined(identity, shown),
            table: Cow::Owned(table),
            inputs,
            returns,
        };
        Self::checked(about, parts, Box::new(move |call, _| callback(|| run(call))))
    }

    /// What the Rust type `T` declares as a thing of `kind` called `name`.
    fn parts<T: Params + 'static>(
        name: &'static str,
        kind: Kind,
        inputs: Vec<(InputKind, bool)>,
        returns: Returns,
    ) -> Result<Parts, Error> {
        Ok(Parts {
            name: Cow::Borrowed(name),
            kind,
            identity: Identity::of::<T>(),
            table: Cow::Borrowed(T::table().map_err(|error| Error::params(name, error))?),
            inputs,
            returns,
        })
    }

    /// The thing of these parts, if it keeps every rule of the [registry](super) but the taken name (which is the
    /// registry's to check): the one place those rules are checked.
    fn checked(about: About, parts: Parts, run: Box<Run>) -> Result<Self, Error> {
        let Parts {
            name,
            kind,
            identity,
            table,
            inputs,
            returns,
        } = parts;
        let label = format!("{kind} {}", quoted(&name));
        let refused = |rule: String| Error::Registration {
            message: format!("{label}: {rule}"),
        };
        if !is_name(&name) {
            return Err(refused(
                "a name is lower case letters, digits and underscores, starting with a letter".to_owned(),
            ));
        }
        let mut clashing: Vec<&str> = table
            .params()
            .iter()
            .map(|param| param.name.as_str())
            .filter(|name| InputKind::NAMES.contains(name) || NODE_KEYS.contains(name))
            .collect();
        clashing.sort_unstable();
        if let Some(clash) = clashing.first() {
            return Err(refused(format!(
                "the parameter {} has the name of an input or a patch key",
                quoted(clash)
            )));
        }
        for (index, (input, _)) in inputs.iter().enumerate() {
            if inputs[..index].iter().any(|(earlier, _)| earlier == input) {
                return Err(refused(format!("the input {} is declared twice", quoted(input.name()))));
            }
        }
        match (kind, returns) {
            (Kind::Source | Kind::Shaper, Returns::Signal) | (Kind::Operation, Returns::Events | Returns::Series) => {}
            (Kind::Source | Kind::Shaper, _) => {
                return Err(refused(format!("a {kind} returns a signal (not {returns})")));
            }
            (Kind::Operation, Returns::Signal) => {
                return Err(refused(
                    "an operation returns events or a series (not a signal)".to_owned(),
                ));
            }
        }
        let summary = about
            .doc
            .split("\n\n")
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        Ok(Self(Arc::new(Inner {
            name,
            about,
            summary,
            kind,
            identity,
            table,
            inputs,
            returns,
            run,
        })))
    }

    /// Its name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0.name
    }

    /// Its kind.
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.0.kind
    }

    /// What a call gives.
    #[must_use]
    pub fn returns(&self) -> Returns {
        self.0.returns
    }

    /// The first paragraph of its doc (up to the first blank line), on one line: its words joined by one space.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.0.summary
    }

    /// Its parameters.
    #[must_use]
    pub fn table(&self) -> &ParamTable {
        &self.0.table
    }

    /// The inputs it takes, each with whether it is required, in the order it declares them.
    #[must_use]
    pub fn inputs(&self) -> &[(InputKind, bool)] {
        &self.0.inputs
    }

    /// It as plain data, for a tool that checks a pipeline before it runs (`bt-control list --json`): `kind`
    /// ([`Kind::name`]), `summary`, `returns` ([`Returns::name`]), `inputs` (each by name: `required` and `doc`, what
    /// [`InputKind::doc`] says) and `parameters`, the JSON Schema of its parameters: `additionalProperties` false,
    /// `properties` as [`ParamTable::describe`] gives them, `description` (its parameters' doc, if there is one),
    /// `title` and `type` `object`. Its objects' keys are sorted; [`ParamTable::listed`] gives the parameters in the
    /// order a reader is shown them.
    ///
    /// ```
    /// let slew = bullterrier_control::Registry::builtin().get("slew")?.description();
    /// assert_eq!((slew["kind"].as_str(), slew["returns"].as_str()), (Some("shaper"), Some("signal")));
    /// assert_eq!(slew["parameters"]["title"], "SlewParams");
    /// assert_eq!(slew["parameters"]["properties"]["rise"]["unit"], "seconds");
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    #[must_use]
    pub fn description(&self) -> Value {
        let Inner {
            about,
            summary,
            kind,
            table,
            inputs,
            returns,
            ..
        } = &*self.0;
        let inputs: Map<String, Value> = inputs
            .iter()
            .map(|(input, required)| {
                (
                    input.name().to_owned(),
                    json!({"required": required, "doc": input.doc()}),
                )
            })
            .collect();
        let mut parameters = json!({
            "additionalProperties": false,
            "properties": table.describe(),
            "title": about.params_title,
            "type": "object",
        });
        if !about.params_doc.is_empty() {
            parameters["description"] = about.params_doc.clone().into();
        }
        json!({
            "kind": kind.name(),
            "summary": summary,
            "returns": returns.name(),
            "inputs": inputs,
            "parameters": parameters,
        })
    }

    /// What `help()` shows: how to call it (`shaper slew(signal, **values) -> signal`; an input required by its name,
    /// an optional one as `name=None`, after `*`), its doc, its inputs (each `name (required)` or `(optional)` and what
    /// [`InputKind::doc`] says), then each parameter in the order [`ParamTable::listed`] gives, in its own terms: a
    /// number's unit and range (`rise (seconds, 0 to 3600)`), its default as Python writes it (`0.1`, `'linear'`,
    /// `True`), whether it is advanced or cyclic, and its description. The words and layout are the Python
    /// package's; trailing blank lines are dropped.
    ///
    /// ```
    /// let help = bullterrier_control::Registry::builtin().get("slew")?.help();
    /// assert!(help.starts_with("shaper slew(signal, **values) -> signal\n\nA signal that follows its input"));
    /// assert!(help.contains("\n  rate (hertz, 10 to 48000), default 1000.0, advanced\n"));
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    #[must_use]
    pub fn help(&self) -> String {
        let Inner {
            name,
            about,
            kind,
            table,
            inputs,
            returns,
            ..
        } = &*self.0;
        let mut takes: Vec<String> = Vec::new();
        if *kind == Kind::Shaper {
            takes.push("signal".to_owned());
        }
        if !inputs.is_empty() {
            takes.push("*".to_owned());
            for (input, required) in inputs {
                takes.push(if *required {
                    input.name().to_owned()
                } else {
                    format!("{input}=None")
                });
            }
        }
        takes.push("**values".to_owned());
        let mut lines = vec![
            format!("{kind} {name}({}) -> {returns}", takes.join(", ")),
            String::new(),
            about.doc.clone(),
            String::new(),
        ];
        if !inputs.is_empty() {
            lines.push("Inputs:".to_owned());
            for (input, required) in inputs {
                let need = if *required { "required" } else { "optional" };
                lines.push(format!("  {input} ({need})"));
                lines.push(format!("      {}", input.doc()));
            }
            lines.push(String::new());
        }
        if !table.params().is_empty() {
            lines.push("Parameters:".to_owned());
        }
        for param in table.listed() {
            let measure = match (param.unit, param.range_text()) {
                (Some(unit), Some(range)) => format!(" ({unit}, {range})"),
                _ => String::new(),
            };
            let advanced = if param.advanced { ", advanced" } else { "" };
            let cyclic = if param.cyclic { ", cyclic" } else { "" };
            let flags = format!("{advanced}{cyclic}");
            lines.push(format!(
                "  {}{measure}, default {}{flags}",
                param.name,
                python(&param.default)
            ));
            lines.push(format!("      {}", param.doc));
        }
        lines.join("\n").trim_end().to_owned()
    }

    /// The result of `call`, checked first, in this order, as the Python package checks a call:
    /// 1. the number of signals: one for a shaper, none for a source or an operation;
    /// 2. no input it does not declare (the first by name is named);
    /// 3. each input it declares, in its order: one given holds data of its kind, a required one is given;
    /// 4. the parameter values, read by its table: an object by name, a value left out taking its default.
    ///
    /// The thing's body then gets the call with the values as the table read them (every field, a cyclic one
    /// wrapped); what it gives must be what it [`returns`](Self::returns), events in time order. A signal it gives
    /// carries its [`Recipe`] ([`Signal::recipe`]): this thing's name, the values as read, the inputs in its order and
    /// the signal it shaped. It reads as the body's signal and is as deep, but at least one deeper than the signal it
    /// shaped, so a chain of calls whose bodies make no node (one that gives its input back) is bounded by
    /// [`MAX_DEPTH`](crate::MAX_DEPTH) as a chain of nodes is.
    ///
    /// # Errors
    /// [`Error::Call`] naming the thing (`shaper 'slew' takes 1 signal(s), not 0`; `operation 'select' takes no input
    /// 'grid'`; `... needs the input 'events'`; `... the input 'events' holds values`; `... gave events, not the
    /// series it returns`); [`Error::Output`] naming it if its events are out of time order (`operation 'x' returned
    /// events out of time order (event 1 at 1 s comes after one at 3 s)`); [`Error::Params`] under its name listing
    /// every rule the values break; [`Error::Depth`] if the signal it gives would be deeper than
    /// [`MAX_DEPTH`](crate::MAX_DEPTH), or if a [custom](Self::custom) body would run inside too many callbacks
    /// already running on the thread; and whatever error its body gives, unchanged.
    ///
    /// ```
    /// use bullterrier_control::registry::{Call, Registry};
    /// use bullterrier_control::Signal;
    /// use serde_json::json;
    ///
    /// let slew = Registry::builtin().get("slew")?;
    /// let refused = slew.call(&Call::default()).map(|_| ()).map_err(|error| error.to_string());
    /// assert_eq!(refused, Err("shaper 'slew' takes 1 signal(s), not 0".to_owned()));
    /// let call = Call { signals: vec![Signal::constant(1.0)?], values: json!({"rise": -1}), ..Default::default() };
    /// assert!(slew.call(&call).is_err_and(|error| error.to_string().contains("rise: must be >= 0")));
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    pub fn call(&self, call: &Call) -> Result<Output, Error> {
        self.called(call.clone())
    }

    /// The signal of `call`, by [`call`](Self::call): what a door that makes a signal gives.
    pub(crate) fn signal(&self, call: Call) -> Result<Signal, Error> {
        match self.called(call)? {
            Output::Signal(signal) => Ok(signal),
            // a thing that gives data takes no signal: `called` refuses a call of it with one first
            other => Err(Error::Call {
                message: format!("{} gave {}, not a signal", self.label(), other.returns()),
            }),
        }
    }

    /// The one door: [`call`](Self::call) of `call`, which it takes over. Every way of making a signal from a thing
    /// comes here ([`Thing::call`], [`shape`](crate::shape), the typed doors [`shaped`] and [`sourced`]), so each
    /// checks, runs, names and records alike.
    fn called(&self, call: Call) -> Result<Output, Error> {
        let Inner {
            kind,
            identity,
            table,
            inputs,
            returns,
            run,
            ..
        } = &*self.0;
        let refused = |what: String| Error::Call {
            message: format!("{} {what}", self.label()),
        };
        let wanted = kind.signals();
        if call.signals.len() != wanted {
            return Err(refused(format!("takes {wanted} signal(s), not {}", call.signals.len())));
        }
        if let Some(name) = call
            .inputs
            .keys()
            .find(|name| !inputs.iter().any(|(input, _)| input.name() == name.as_str()))
        {
            return Err(refused(format!("takes no input {}", quoted(name))));
        }
        for (input, required) in inputs {
            match call.inputs.get(input.name()) {
                Some(given) if given.kind() != *input => {
                    return Err(Error::Call {
                        message: format!(
                            "{}: the input {} holds {}",
                            self.label(),
                            quoted(input.name()),
                            given.kind()
                        ),
                    });
                }
                None if *required => return Err(refused(format!("needs the input {}", quoted(input.name())))),
                _ => {}
            }
        }
        let values = table
            .parse(&call.values)
            .map_err(|error| Error::params(self.name(), error))?;
        let checked = Call {
            values: values.clone().into_json(),
            ..call
        };
        let output = run(&checked, &values)?;
        if output.returns() != *returns {
            return Err(refused(format!(
                "gave {}, not the {returns} it returns",
                output.returns()
            )));
        }
        let signal = match output {
            Output::Signal(signal) => signal,
            Output::Events(events) => return self.in_time_order(events).map(Output::Events),
            // data: a later node uses it inline, so there is nothing to record
            other => return Ok(other),
        };
        let Call {
            signals,
            inputs: mut given,
            values,
        } = checked;
        let recipe = Recipe {
            name: self.name().to_owned(),
            values,
            inputs: inputs
                .iter()
                .filter_map(|(input, _)| given.remove_entry(input.name()))
                .collect(),
            signal: signals.into_iter().next(),
            made_by: (*kind, identity.clone()),
        };
        Ok(Output::Signal(Signal::recorded(recipe, signal)?))
    }

    /// `events`, the events its body gave, if they are in time order (events at one time in any order); they are not
    /// sorted for it, as an order a body meant may differ.
    fn in_time_order(&self, events: Vec<Event>) -> Result<Vec<Event>, Error> {
        let late = events.windows(2).position(|pair| pair[1].t() < pair[0].t());
        match late {
            None => Ok(events),
            Some(index) => Err(Error::Output {
                message: format!(
                    "{} returned events out of time order (event {} at {} s comes after one at {} s)",
                    self.label(),
                    index + 1,
                    events[index + 1].t(),
                    events[index].t()
                ),
            }),
        }
    }

    /// Who defined it.
    pub(super) fn identity(&self) -> &Identity {
        &self.0.identity
    }

    /// Whether `other` is this thing registered again: the same kind and identity.
    pub(super) fn is_same(&self, other: &Self) -> bool {
        self.is(other.0.kind, &other.0.identity)
    }

    /// Whether this thing is of `kind` and defined as `identity`.
    pub(super) fn is(&self, kind: Kind, identity: &Identity) -> bool {
        self.0.kind == kind && self.0.identity == *identity
    }

    /// The thing as an error names it: its kind and its name as Python writes it (`shaper 'slew'`).
    pub(super) fn label(&self) -> String {
        format!("{} {}", self.0.kind, quoted(self.name()))
    }
}

/// The signal the shaper `shaper` makes of `input`, through the one door of the thing `T` is
/// ([`Thing::shaper`]): what [`Signal::pipe`] and a typed method ([`Signal::slew`]) give.
///
/// # Errors
/// As [`Thing::call`], and [`Error::Registration`] if `T` breaks a rule of the [registry](super).
#[expect(
    clippy::needless_pass_by_value,
    reason = "a door takes its thing as the caller writes it, a literal: `glow.pipe(Fold { depth: 0.5 })`"
)]
pub(crate) fn shaped<T: Shaper>(shaper: T, input: &Signal) -> Result<Signal, Error> {
    typed(&Thing::shaper::<T>(About::default())?, vec![input.clone()], (), &shaper)
}

/// The signal the source `source` makes, through the one door of the thing `T` is ([`Thing::source`]): what a
/// `lib` function gives.
///
/// # Errors
/// As [`shaped`].
#[expect(
    clippy::needless_pass_by_value,
    reason = "a door takes its thing as the caller writes it, a literal: `lib::constant(constant::Params { .. })`"
)]
pub(crate) fn sourced<T: Source>(source: T) -> Result<Signal, Error> {
    typed(&Thing::source::<T>(About::default())?, Vec::new(), (), &source)
}

/// The signal `thing` makes of typed arguments: each lowered to what a call by name gives (`inputs` by
/// [`Inputs::into_inputs`](super::Inputs::into_inputs), `params` by [`Params::to_value`]) and made through the one
/// door, so the body is given back exactly what the recipe records.
fn typed(thing: &Thing, signals: Vec<Signal>, inputs: impl Inputs, params: &impl Params) -> Result<Signal, Error> {
    let inputs = inputs
        .into_inputs()?
        .into_iter()
        .map(|input| (input.kind().name().to_owned(), input))
        .collect();
    thing.signal(Call {
        signals,
        inputs,
        values: params.to_value(),
    })
}
