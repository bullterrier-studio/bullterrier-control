//! [`Thing`]: one source, shaper or operation, its rules checked, what it says about itself, and its call.

use std::fmt;
use std::sync::Arc;

use serde_json::{Map, Value, json};

use super::repr::{python, quoted};
use super::{
    About, Call, Declared, Inputs as _, Kind, NODE_KEYS, Operation, Output, Recipe, Returns, Shaper, Source, is_name,
};
use crate::inputs::InputKind;
use crate::params::{ParamTable, Values};
use crate::{Error, Params, Signal};

/// The body of a thing: called with a checked call and its parameters as the table read them.
type Run = dyn Fn(&Call, &Values) -> Result<Output, Error> + Send + Sync;

/// A registered source, shaper or operation: what it is, takes and gives, what it says about itself, and its call.
/// Cheap to clone (it is shared), `Send` and `Sync`.
#[derive(Clone)]
pub struct Thing(Arc<Inner>);

struct Inner {
    about: About,
    summary: String,
    kind: Kind,
    identity: String,
    table: ParamTable,
    inputs: Vec<(InputKind, bool)>,
    returns: Returns,
    run: Box<Run>,
}

impl fmt::Debug for Thing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Thing")
            .field("name", &self.0.about.name)
            .field("kind", &self.0.kind)
            .finish_non_exhaustive()
    }
}

impl Thing {
    /// The source `T` under the name and doc `about` gives: its parameters are `T`'s table, its identity is `T`'s type,
    /// and a call gives the signal [`Source::source`] makes of the values read into a `T`.
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
    ///     fn source(&self) -> Result<Signal, Error> {
    ///         Signal::constant(self.value)
    ///     }
    /// }
    ///
    /// let level = Thing::source::<Level>(About { name: "level".into(), ..Default::default() })?;
    /// let call = Call { values: json!({"value": -0.25}), ..Default::default() };
    /// let Output::Signal(signal) = level.call(&call)? else { panic!("a source gives a signal") };
    /// assert_eq!(signal.at(1.0)?, -0.25);
    /// # Ok::<(), Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Registration`] if the thing breaks a rule of the [registry](super), [`Error::Params`] if `T`'s
    /// declaration breaks a rule of its table.
    pub fn source<T: Source>(about: About) -> Result<Self, Error> {
        let declared = Self::declared::<T>(&about, Kind::Source, Vec::new(), Returns::Signal)?;
        Self::checked(
            about,
            declared,
            Box::new(|_, values| Ok(Output::Signal(T::from_values(values).source()?))),
        )
    }

    /// The shaper `T` under the name and doc `about` gives: its parameters are `T`'s table, its identity is `T`'s type,
    /// and a call gives the signal [`Shaper::shape`] makes of its signal with the values read into a `T`.
    ///
    /// ```
    /// use bullterrier_control::builtin::slew;
    /// use bullterrier_control::registry::{About, Kind, Thing};
    ///
    /// let mine = Thing::shaper::<slew::Params>(About { name: "my_slew".into(), ..Default::default() })?;
    /// assert_eq!((mine.name(), mine.kind()), ("my_slew", Kind::Shaper));
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Thing::source`].
    pub fn shaper<T: Shaper>(about: About) -> Result<Self, Error> {
        let declared = Self::declared::<T>(&about, Kind::Shaper, Vec::new(), Returns::Signal)?;
        Self::checked(
            about,
            declared,
            Box::new(|call, values| match call.signals.as_slice() {
                [input] => Ok(Output::Signal(T::from_values(values).shape(input)?)),
                // `call` checked the count already
                _ => Err(Error::Call {
                    message: "a shaper takes 1 signal".to_owned(),
                }),
            }),
        )
    }

    /// The operation `T` under the name and doc `about` gives: its parameters are `T`'s table, its inputs
    /// [`Operation::Inputs`]'s, what it gives [`Operation::RETURNS`], its identity `T`'s type; a call gives what
    /// [`Operation::run`] makes of its inputs with the values read into a `T`.
    ///
    /// # Errors
    /// As [`Thing::source`].
    pub fn operation<T: Operation>(about: About) -> Result<Self, Error> {
        let declared = Self::declared::<T>(&about, Kind::Operation, T::Inputs::DECLARED.to_vec(), T::RETURNS)?;
        Self::checked(
            about,
            declared,
            Box::new(|call, values| T::from_values(values).run(T::Inputs::from_inputs(&call.inputs)?)),
        )
    }

    /// The thing `about` says, declaring `declared`, whose body is `run`: how the Python package registers a thing
    /// whose body is Python. `run` is called with the call as [`Thing::call`] checked it.
    ///
    /// # Errors
    /// [`Error::Registration`] if the thing breaks a rule of the [registry](super) (its name, a parameter's name, an
    /// input declared twice, what its kind returns).
    pub fn custom(
        about: About,
        declared: Declared,
        run: impl Fn(&Call) -> Result<Output, Error> + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::checked(about, declared, Box::new(move |call, _| run(call)))
    }

    /// What the Rust type `T` declares as a thing of `kind`.
    fn declared<T: Params>(
        about: &About,
        kind: Kind,
        inputs: Vec<(InputKind, bool)>,
        returns: Returns,
    ) -> Result<Declared, Error> {
        Ok(Declared {
            kind,
            identity: std::any::type_name::<T>().to_owned(),
            table: T::table().map_err(|error| Error::params(&about.name, error))?.clone(),
            inputs,
            returns,
        })
    }

    /// The thing of these parts, if it keeps every rule of the [registry](super) but the taken name (which is the
    /// registry's to check): the one place those rules are checked.
    fn checked(about: About, declared: Declared, run: Box<Run>) -> Result<Self, Error> {
        let Declared {
            kind,
            identity,
            table,
            inputs,
            returns,
        } = declared;
        let label = format!("{kind} {}", quoted(&about.name));
        let refused = |rule: String| Error::Registration {
            message: format!("{label}: {rule}"),
        };
        if !is_name(&about.name) {
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
        &self.0.about.name
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
    #[must_use]
    pub fn help(&self) -> String {
        let Inner {
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
            format!("{kind} {}({}) -> {returns}", about.name, takes.join(", ")),
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
    /// wrapped); what it gives must be what it [`returns`](Self::returns). A signal it gives carries its
    /// [`Recipe`] ([`Signal::recipe`]): this thing's name, the values as read, the inputs in its order and the signal
    /// it shaped. It reads as the body's signal and is as deep.
    ///
    /// # Errors
    /// [`Error::Call`] naming the thing (`shaper 'slew' takes 1 signal(s), not 0`; `operation 'select' takes no input
    /// 'grid'`; `... needs the input 'events'`; `... the input 'events' holds values`; `... gave events, not the
    /// series it returns`); [`Error::Params`] under its name listing every rule the values break; and whatever error
    /// its body gives, unchanged.
    pub fn call(&self, call: &Call) -> Result<Output, Error> {
        let Inner {
            kind,
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
            signals: call.signals.clone(),
            inputs: call.inputs.clone(),
            values: values.clone().into_json(),
        };
        let output = run(&checked, &values)?;
        if output.returns() != *returns {
            return Err(refused(format!(
                "gave {}, not the {returns} it returns",
                output.returns()
            )));
        }
        let Output::Signal(signal) = output else {
            return Ok(output); // data: a later node uses it inline, so there is nothing to record
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
        };
        Ok(Output::Signal(Signal::recorded(recipe, signal)))
    }

    /// Who defined it.
    pub(super) fn identity(&self) -> &str {
        &self.0.identity
    }

    /// Whether `other` is this thing registered again: the same kind and identity.
    pub(super) fn is_same(&self, other: &Self) -> bool {
        (self.0.kind, &self.0.identity) == (other.0.kind, &other.0.identity)
    }

    /// The thing as an error names it: its kind and its name as Python writes it (`shaper 'slew'`).
    pub(super) fn label(&self) -> String {
        format!("{} {}", self.0.kind, quoted(self.name()))
    }
}
