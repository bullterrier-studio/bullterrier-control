//! The core's registry for Python: `Registry`, a table of sources, shapers and operations (the package keeps one), and
//! `Thing`, one of them, called as the package calls it. Every rule of getting in, of calling and of what a thing says
//! about itself is the core's (`bullterrier_control::registry`); the bindings read Python's arguments into a core call
//! in the package's order (the signals, an input the thing does not take, each input it takes in its order, then the
//! values) and raise the core's refusals as the package's exceptions.
//!
//! A Python function registers as a thing whose body is a callback ([`body`]): it attaches to Python for as long as
//! the function runs, gets its signals, its `Params` and its inputs as the package's objects, and what it returns is
//! read as what the thing returns. An exception it raises reaches the caller unchanged.
//!
//! Concurrency: a registry is a value shared copy-on-write ([`Held`]): a change makes a new one under a short lock,
//! and a reader (a call, a patch read) takes the one in place and lets go of the lock before anything runs. No lock is
//! held while a callback runs, so a body may call, register and remove things; and a call of a Rust thing runs without
//! the GIL.

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyMapping, PyString, PyTuple, PyType};

use bullterrier_control::Error;
use bullterrier_control::formats::Event;
use bullterrier_control::inputs::InputKind;
use bullterrier_control::registry::{About, Call, Declared, Kind, Output, Registry, Returns, Thing};

use crate::callback::Nested;
use crate::data;
use crate::error::{parameter_error, raised, unknown_name};
use crate::formats::{PyEvent, PySeries, type_name};
use crate::inputs;
use crate::params::{self, Given, PyParamTable};
use crate::signal::{self, SignalBase, made};

/// What a registry holds: the core's registry (what a patch is read with) and, for each of its things, the `Thing`
/// object Python is given, so finding one twice gives the same object. Never changed once shared.
pub(crate) struct Held {
    registry: Registry,
    things: BTreeMap<String, Py<PyThing>>,
}

impl Held {
    /// The core's registry.
    pub(crate) const fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The `Thing` object of the thing called `name`, if there is one.
    pub(crate) fn thing(&self, name: &str) -> Option<&Py<PyThing>> {
        self.things.get(name)
    }

    /// A copy to change.
    fn copy(&self, py: Python<'_>) -> Self {
        Self {
            registry: self.registry.clone(),
            things: self
                .things
                .iter()
                .map(|(name, thing)| (name.clone(), thing.clone_ref(py)))
                .collect(),
        }
    }
}

/// A table of sources, shapers and operations by name: `Registry(builtin=True)` holds the things the Rust core runs,
/// `Registry(builtin=False)` nothing.
///
/// `register(function, *, kind, params, inputs, returns)` registers a Python function as the thing
/// `function.__name__` of `kind` (`"source"`, `"shaper"` or `"operation"`), whose parameters are those of `params`
/// (its `Params` class: its table checks the values, `params.parse(values)` makes them; anything else is a
/// `TypeError`), which takes `inputs` (a mapping of each input's name to whether it is required, in the order the
/// function declares them) and gives `returns` (`"signal"`,
/// `"events"` or `"series"`), and returns the `Thing`. The core checks every rule: the name, a parameter named like an
/// input or a patch key, what the kind returns, and the taken name: the same function registered again (or, for one
/// defined at the top of a module, one of the same module and qualified name, as reloading the module makes)
/// replaces itself, while another one, or one of another kind, is refused. Each refusal is a `ValueError` naming the
/// thing; nothing is registered then.
///
/// `get(name)` gives the `Thing` (the same object each time), `things()` every one by name, sorted, `unregister(name)`
/// removes one; an unknown name is an `UnknownNameError` listing those registered. `replace(other)` makes this hold
/// what `other` holds and gives back what it held, as a registry of its own. Any thread may use a registry at once,
/// a body of one of its things too.
#[pyclass(frozen, module = "bullterrier_control._native._core", name = "Registry")]
pub(crate) struct PyRegistry {
    held: Mutex<Arc<Held>>,
}

impl PyRegistry {
    /// What it holds now.
    pub(crate) fn held(&self) -> Arc<Held> {
        Arc::clone(&self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Arc<Held>> {
        // nothing panics while it is held: a poisoned lock still holds a whole registry
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `change` made to a copy of what it holds, which it then holds; nothing changes if `change` fails. What it held
    /// before is let go of after the lock: dropping a thing may run Python code, which may use this registry.
    fn change<T>(&self, py: Python<'_>, change: impl FnOnce(&mut Held) -> Result<T, Error>) -> Result<T, Error> {
        let mut held = self.lock();
        let mut next = held.copy(py);
        let made = change(&mut next);
        let before = made.is_ok().then(|| std::mem::replace(&mut *held, Arc::new(next)));
        drop(held);
        drop(before);
        made
    }
}

#[pymethods]
impl PyRegistry {
    #[new]
    #[pyo3(signature = (*, builtin))]
    fn new(py: Python<'_>, builtin: bool) -> PyResult<Self> {
        let registry = if builtin {
            Registry::builtin().clone()
        } else {
            Registry::new()
        };
        let things = registry
            .things()
            .map(|thing| {
                let made = PyThing::object(py, thing.clone(), thing.name().to_owned(), None)?;
                Ok((thing.name().to_owned(), made))
            })
            .collect::<PyResult<_>>()?;
        Ok(Self {
            held: Mutex::new(Arc::new(Held { registry, things })),
        })
    }

    #[pyo3(signature = (function, *, kind, params, inputs, returns))]
    fn register(
        &self,
        function: &Bound<'_, PyAny>,
        kind: &str,
        params: &Bound<'_, PyAny>,
        inputs: &Bound<'_, PyAny>,
        returns: &str,
    ) -> PyResult<Py<PyThing>> {
        let py = function.py();
        let table = params_table(params)?;
        let name: String = function.getattr("__name__")?.extract()?;
        let kind = kind_of(kind)?;
        let label = format!("{kind} '{name}'");
        let returns = returns_of(returns)?;
        let inspect = py.import("inspect")?;
        let doc = inspect.call_method1("getdoc", (function,))?;
        let params_doc = params.getattr("__doc__")?;
        let about = About {
            doc: if doc.is_none() { String::new() } else { doc.extract()? },
            params_title: params.getattr("__name__")?.extract()?,
            params_doc: if params_doc.is_truthy()? {
                inspect.call_method1("cleandoc", (params_doc,))?.extract()?
            } else {
                String::new()
            },
        };
        let declared = Declared {
            name: name.clone(),
            kind,
            identity: identity(function)?,
            shown: shown(function)?,
            table: table.get().table().clone(),
            inputs: declared_inputs(inputs)?,
            returns,
        };
        let run = body(function.clone().unbind(), params.clone().unbind(), returns, label);
        let thing = Thing::custom(about, declared, run).map_err(|error| raised(py, error))?;
        let object = PyThing::object(
            py,
            thing.clone(),
            table.get().name().to_owned(),
            Some((function.clone().unbind(), params.clone().unbind())),
        )?;
        let given = object.clone_ref(py);
        self.change(py, move |held| {
            held.registry.register(thing)?;
            held.things.insert(name, given);
            Ok(())
        })
        .map_err(|error| raised(py, error))?;
        Ok(object)
    }

    fn get(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyThing>> {
        let held = self.held();
        held.things
            .get(name)
            .map(|thing| thing.clone_ref(py))
            .ok_or_else(|| unknown(py, &held, name))
    }

    fn things<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (name, thing) in &self.held().things {
            dict.set_item(name, thing)?;
        }
        Ok(dict)
    }

    fn unregister(&self, py: Python<'_>, name: &str) -> PyResult<()> {
        self.change(py, |held| {
            held.registry.unregister(name)?;
            held.things.remove(name);
            Ok(())
        })
        .map_err(|error| match error {
            Error::UnknownName { .. } => unknown_name(py, error.to_string()),
            other => raised(py, other),
        })
    }

    fn replace(&self, other: &Bound<'_, Self>) -> Self {
        let incoming = other.get().held();
        let before = std::mem::replace(&mut *self.lock(), incoming);
        Self {
            held: Mutex::new(before),
        }
    }
}

/// The `UnknownNameError` for `name`, which `held` does not hold: the core's words.
fn unknown(py: Python<'_>, held: &Held, name: &str) -> PyErr {
    let error = Error::UnknownName {
        name: name.to_owned(),
        available: held.things.keys().cloned().collect(),
    };
    unknown_name(py, error.to_string())
}

/// The table of `params`, a `Params` class: the `ParamTable` it built when it was made (`Params._table`, which only
/// `Params` and this read); a `TypeError` for anything else.
fn params_table<'py>(params: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyParamTable>> {
    params
        .cast::<PyType>()
        .ok()
        .and_then(|class| class.getattr("_table").ok())
        .and_then(|table| table.cast_into::<PyParamTable>().ok())
        .ok_or_else(|| {
            PyTypeError::new_err(format!(
                "params must be a Params subclass (got {})",
                params
                    .repr()
                    .map_or_else(|_| type_name(params), |text| text.to_string())
            ))
        })
}

/// The kind called `name`.
fn kind_of(name: &str) -> PyResult<Kind> {
    match name {
        "source" => Ok(Kind::Source),
        "shaper" => Ok(Kind::Shaper),
        "operation" => Ok(Kind::Operation),
        other => Err(PyValueError::new_err(format!(
            "kind must be source, shaper or operation (got {other:?})"
        ))),
    }
}

/// What a call gives, called `name`.
fn returns_of(name: &str) -> PyResult<Returns> {
    match name {
        "signal" => Ok(Returns::Signal),
        "events" => Ok(Returns::Events),
        "series" => Ok(Returns::Series),
        other => Err(PyValueError::new_err(format!(
            "returns must be signal, events or series (got {other:?})"
        ))),
    }
}

/// The inputs `inputs` declares (a mapping of each input's name to whether it is required), in its order.
fn declared_inputs(inputs: &Bound<'_, PyAny>) -> PyResult<Vec<(InputKind, bool)>> {
    inputs
        .cast::<PyMapping>()?
        .items()?
        .iter()
        .map(|pair| {
            let (name, required) = pair.extract::<(String, bool)>()?;
            let kind = InputKind::from_str(&name).map_err(|error| PyValueError::new_err(error.to_string()))?;
            Ok((kind, required))
        })
        .collect()
}

/// Who defined `function`, telling apart every function that may be registered: its module and qualified name, and
/// for one made inside another function (`make.<locals>.made`, which a factory makes many of) its object too. So the
/// same function, or one defined at the top of a module again (a reload), is the same thing; two a factory made are not.
fn identity(function: &Bound<'_, PyAny>) -> PyResult<String> {
    let module = function.getattr("__module__")?.str()?.to_string();
    let qualname: String = function.getattr("__qualname__")?.extract()?;
    Ok(if qualname.contains("<locals>") {
        format!("{module}.{qualname} at {:#x}", function.as_ptr().addr())
    } else {
        format!("{module}.{qualname}")
    })
}

/// How an error shows who defined `function`: its module and qualified name, when [`identity`] adds to them what only
/// tells two functions apart (an address). `None` when the identity is that already.
fn shown(function: &Bound<'_, PyAny>) -> PyResult<Option<String>> {
    let module = function.getattr("__module__")?.str()?.to_string();
    let qualname: String = function.getattr("__qualname__")?.extract()?;
    Ok(qualname.contains("<locals>").then(|| format!("{module}.{qualname}")))
}

/// The body of the thing `label` whose Python function is `function`: called with the checked call, it attaches to
/// Python, calls `function(*signals, params.parse(values), **inputs)` and reads what it returns as `returns`. A Python
/// exception (the function's, or the `TypeError` of a result of another kind) is the error, raised again unchanged.
fn body(
    function: Py<PyAny>,
    params: Py<PyAny>,
    returns: Returns,
    label: String,
) -> impl Fn(&Call) -> Result<Output, Error> + Send + Sync + 'static {
    move |call| {
        Python::attach(|py| {
            let _nested = Nested::enter(py)?;
            let mut arguments = call
                .signals
                .iter()
                .map(|one| made(py, one.clone()))
                .collect::<PyResult<Vec<_>>>()?;
            arguments.push(
                params
                    .bind(py)
                    .call_method1("parse", (params::python(py, &call.values)?,))?,
            );
            let given = PyDict::new(py);
            for (name, input) in &call.inputs {
                given.set_item(name, inputs::python(py, input.clone())?)?;
            }
            let result = function.bind(py).call(PyTuple::new(py, arguments)?, Some(&given))?;
            output(&result, returns, &label)
        })
        .map_err(|error| Error::Node {
            source: Arc::new(error),
        })
    }
}

/// What a Python body gave, `result`, as what the thing `label` returns: a signal, a tuple (or list) of `Event`, or a
/// `Series`; anything else is a `TypeError`.
fn output(result: &Bound<'_, PyAny>, returns: Returns, label: &str) -> PyResult<Output> {
    let wrong = |wanted: &str| PyTypeError::new_err(format!("{label} returned {}, not {wanted}", type_name(result)));
    match returns {
        Returns::Signal => result
            .cast::<SignalBase>()
            .map(|one| Output::Signal(signal::signal(one)))
            .map_err(|_| wrong("a signal")),
        Returns::Events => {
            let items: Vec<Bound<'_, PyAny>> = if let Ok(tuple) = result.cast::<PyTuple>() {
                tuple.iter().collect()
            } else if let Ok(list) = result.cast::<PyList>() {
                list.iter().collect()
            } else {
                return Err(wrong("a tuple of events"));
            };
            // the core refuses events out of time order, naming the thing
            let events: Vec<Event> = items
                .iter()
                .map(|item| item.cast::<PyEvent>().map(|event| event.get().0.clone()))
                .collect::<Result<_, _>>()
                .map_err(|_| wrong("a tuple of events"))?;
            Ok(Output::Events(events))
        }
        Returns::Series => result
            .cast::<PySeries>()
            .map(|series| Output::Series(series.get().series().clone()))
            .map_err(|_| wrong("a series")),
    }
}

/// What a call gave, as the package's objects: a signal, a tuple of `Event` or a `Series`.
pub(crate) fn python(py: Python<'_>, output: Output) -> PyResult<Bound<'_, PyAny>> {
    match output {
        Output::Signal(one) => made(py, one),
        Output::Events(events) => Ok(PyTuple::new(py, events.into_iter().map(PyEvent))?.into_any()),
        Output::Series(series) => Ok(Bound::new(py, PySeries::wrap(py, series)?)?.into_any()),
        other => Err(PyValueError::new_err(format!("the bindings give no {other:?}"))),
    }
}

/// One source, shaper or operation of a registry. Call it with its signals (a shaper takes one), its inputs and its
/// parameter values by name: a source and a shaper give a signal, an operation its events (a tuple of `Event`) or its
/// series. A name in `INPUTS` is an input, any other a parameter; a value left out takes its default, and `None` for an
/// optional input is leaving it out.
///
/// Refused, in this order: another number of signals than the kind takes, or one that is not a signal (`TypeError`);
/// an input it does not take (`TypeError`); each input it takes, in its order: one that breaks its kind's rules
/// (`ParameterError`, under its path: `points[1].t`) or a required one left out (`TypeError`); then the values
/// (`ParameterError` listing every rule broken, under each field's name). Each message starts with the thing
/// (`shaper 'slew'`), and a refusal of a value with its table's name too (`shaper 'slew': slew: rise: ...`).
///
/// It says what it is: `name`, `kind`, `returns`, `summary` (the first paragraph of its doc), `inputs` (each input's
/// name to whether it is required, in its order, read-only), `__doc__` (what `help()` shows), `__signature__`,
/// `description()` (its data, for `bt-control list --json`), and for one whose body is Python its `function` and
/// `params_model` (both `None` for a thing of the Rust core).
#[pyclass(frozen, dict, module = "bullterrier_control._native._core", name = "Thing")]
pub(crate) struct PyThing {
    thing: Thing,
    /// What a refusal of its values names after the thing: its table's name (a plugin's `Params` class, a built-in's
    /// name).
    table: String,
    /// The Python function and the `Params` class of a thing whose body is Python.
    python: Option<(Py<PyAny>, Py<PyAny>)>,
}

impl PyThing {
    /// The object Python is given for `thing`: its `__doc__` is what `help()` shows (how to call it, its doc, its
    /// inputs, then each parameter in its own terms).
    fn object(
        py: Python<'_>,
        thing: Thing,
        table: String,
        python: Option<(Py<PyAny>, Py<PyAny>)>,
    ) -> PyResult<Py<Self>> {
        let help = thing.help();
        let object = Py::new(py, Self { thing, table, python })?;
        object.bind(py).setattr("__doc__", help)?;
        Ok(object)
    }

    /// The thing as an error names it: `shaper 'slew'`.
    fn label(&self) -> String {
        format!("{} '{}'", self.thing.kind(), self.thing.name())
    }

    /// The Python exception for `error`, a refusal of a call of this thing whose values were `given`: the call's
    /// shape a `TypeError`, its values a `ParameterError` naming the thing and its table; anything else as the core's
    /// errors are raised (a body's exception unchanged).
    pub(crate) fn refused(&self, py: Python<'_>, error: Error, given: Option<&Given>) -> PyErr {
        match error {
            Error::Call { message } => PyTypeError::new_err(message),
            Error::Params {
                mut problems, omitted, ..
            } => {
                if let Some(given) = given {
                    given.explain(&mut problems);
                }
                parameter_error(py, &format!("{}: {}", self.label(), self.table), &problems, omitted)
            }
            other => raised(py, other),
        }
    }

    /// The core's call of `signals` and `values` (inputs and parameters by name), read in the package's order of
    /// checks (see [`PyThing`]); the values as given, to explain their refusal.
    fn call_of(&self, signals: &Bound<'_, PyTuple>, values: Option<&Bound<'_, PyDict>>) -> PyResult<(Call, Given)> {
        let py = signals.py();
        let label = self.label();
        let wanted = self.thing.kind().signals();
        let read: Option<Vec<_>> = signals
            .iter()
            .map(|one| one.cast::<SignalBase>().ok().map(signal::signal))
            .collect();
        let signals = match read {
            Some(read) if read.len() == wanted => read,
            _ => {
                return Err(PyTypeError::new_err(format!(
                    "{label} takes {wanted} signal(s), not {}",
                    signals.len()
                )));
            }
        };
        let mut given = BTreeMap::new();
        let mut parameters = Vec::new();
        for (key, value) in values.into_iter().flat_map(PyDictMethods::iter) {
            let key = key.cast_into::<PyString>()?.to_str()?.to_owned();
            if InputKind::NAMES.contains(&key.as_str()) {
                given.insert(key, value);
            } else {
                parameters.push((key, value));
            }
        }
        let declared = self.thing.inputs();
        if let Some(name) = given
            .keys()
            .find(|name| !declared.iter().any(|(kind, _)| kind.name() == name.as_str()))
        {
            return Err(PyTypeError::new_err(format!("{label} takes no input '{name}'")));
        }
        let mut inputs = BTreeMap::new();
        for (kind, required) in declared {
            match given.remove(kind.name()) {
                Some(value) if *required || !value.is_none() => {
                    let input = match data::read(value, |data| kind.check(data))? {
                        Ok(input) => input,
                        Err(Error::Input { problems, omitted }) => {
                            return Err(parameter_error(py, &label, &problems, omitted));
                        }
                        Err(other) => return Err(raised(py, other)),
                    };
                    inputs.insert(kind.name().to_owned(), input);
                }
                // the core refuses the call, naming it
                None if *required => break,
                _ => {}
            }
        }
        let given = Given::read(self.thing.table(), parameters);
        let call = Call {
            signals,
            inputs,
            values: given.object(),
        };
        Ok((call, given))
    }
}

#[pymethods]
impl PyThing {
    #[pyo3(signature = (*signals, **values))]
    fn __call__<'py>(
        slf: &Bound<'py, Self>,
        signals: &Bound<'py, PyTuple>,
        values: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let this = slf.get();
        let (call, given) = this.call_of(signals, values)?;
        let thing = this.thing.clone();
        let output = py
            .detach(move || thing.call(&call))
            .map_err(|error| this.refused(py, error, Some(&given)))?;
        python(py, output)
    }

    /// Its name.
    #[getter]
    fn name(&self) -> &str {
        self.thing.name()
    }

    /// `source`, `shaper` or `operation`.
    #[getter]
    fn kind(&self) -> &'static str {
        self.thing.kind().name()
    }

    /// What a call gives: `signal`, `events` or `series`.
    #[getter]
    fn returns(&self) -> &'static str {
        self.thing.returns().name()
    }

    /// The first paragraph of its doc, on one line.
    #[getter]
    fn summary(&self) -> &str {
        self.thing.summary()
    }

    /// Each input it takes, by name, to whether it is required, in the order it declares them (read-only).
    #[getter]
    fn inputs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let dict = PyDict::new(py);
        for (kind, required) in self.thing.inputs() {
            dict.set_item(kind.name(), required)?;
        }
        py.import("types")?.getattr("MappingProxyType")?.call1((dict,))
    }

    /// The Python function of a thing whose body is Python; `None` for a thing of the Rust core.
    #[getter]
    fn function(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.python.as_ref().map(|(function, _)| function.clone_ref(py))
    }

    /// The `Params` class of a thing whose body is Python; `None` for a thing of the Rust core.
    #[getter]
    fn params_model(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.python.as_ref().map(|(_, params)| params.clone_ref(py))
    }

    /// How to call it, for `inspect.signature`: its signal (a shaper's) positional, then each input (`None` by default
    /// when optional) and each parameter with its default, by keyword, in the order `help()` lists them.
    #[getter]
    fn __signature__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inspect = py.import("inspect")?;
        let parameter = inspect.getattr("Parameter")?;
        let by_keyword = parameter.getattr("KEYWORD_ONLY")?;
        let defaulted = |name: &str, default: Bound<'py, PyAny>| {
            let default_is = PyDict::new(py);
            default_is.set_item("default", default)?;
            parameter.call((name, &by_keyword), Some(&default_is))
        };
        let mut parameters = Vec::new();
        if self.thing.kind() == Kind::Shaper {
            parameters.push(parameter.call1(("signal", parameter.getattr("POSITIONAL_ONLY")?))?);
        }
        for (kind, required) in self.thing.inputs() {
            parameters.push(if *required {
                parameter.call1((kind.name(), &by_keyword))?
            } else {
                defaulted(kind.name(), py.None().into_bound(py))?
            });
        }
        for param in self.thing.table().listed() {
            parameters.push(defaulted(&param.name, params::python(py, &param.default)?)?);
        }
        inspect.getattr("Signature")?.call1((parameters,))
    }

    /// It as plain data, for a tool that checks a pipeline before it runs (`bt-control list --json`): `kind`,
    /// `summary`, `returns`, `inputs` (each `{required, doc}`, in the order it declares them) and `parameters`, the JSON
    /// Schema of its parameters, their properties in the order `help()` lists them.
    fn description<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let described = params::python(py, &self.thing.description())?;
        let sorted = described.get_item("inputs")?;
        let inputs = PyDict::new(py);
        for (kind, _) in self.thing.inputs() {
            inputs.set_item(kind.name(), sorted.get_item(kind.name())?)?;
        }
        described.set_item("inputs", inputs)?;
        let schema = described.get_item("parameters")?;
        let sorted = schema.get_item("properties")?;
        let properties = PyDict::new(py);
        for param in self.thing.table().listed() {
            properties.set_item(&param.name, sorted.get_item(&param.name)?)?;
        }
        schema.set_item("properties", properties)?;
        Ok(described)
    }
}
