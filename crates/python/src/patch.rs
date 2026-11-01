//! The core's patches for Python: `Patch`, a patch read and built by the core (`bullterrier_control::patch`), its
//! `Node`s, and `patch_schema`. Every rule is the core's; the bindings read a Python mapping as the entries the core
//! reads, in the mapping's order, and raise the core's refusals as the package's exceptions.
//!
//! A patch is read with a `Registry` (what it holds then: a thing registered later is not in it) and built from the
//! things it was read with, without the GIL: a node whose body is Python attaches to Python while it runs. Reading a
//! TOML text runs without the GIL too.

use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyMappingProxy;
use pyo3::types::{PyDict, PyMapping, PyString};
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use bullterrier_control::formats::{Problem, Problems};
use bullterrier_control::patch::{Given, Node, PatchError, Reference};
use bullterrier_control::{Patch, Signal};

use crate::data::{self, beyond_64_bits, shown};
use crate::error::{at_node, format_error_saying, raised, unknown_name};
use crate::formats::{PyBundle, type_name};
use crate::inputs;
use crate::params;
use crate::registry::{self, Held, PyRegistry};
use crate::signal::{self, SignalBase};

/// The nodes of a patch by name, in the order it gives them, read and built by the Rust core.
///
/// `Patch.parse(data, *, registry)` reads `data`, a mapping of the header (`format`, `version`) and the nodes by name,
/// in the mapping's order (a node uses only the nodes above it); `Patch.from_toml(text, *, registry)` reads a TOML
/// text. Each looks up what a node uses in `registry` as it holds it then, and refuses with one `FormatError` listing
/// every problem as `<node>.<key>: rule` (the first 100); a text that is not TOML is a `FormatError` saying where
/// (`not TOML (line 1, column 10: ...)`). A key that is not text is read as its `str`; a value JSON cannot hold (NaN, an
/// infinity, an integer beyond 64 bits, a `set`) is refused at its path, before anything else is checked. An exception
/// the mapping raises while read reaches the caller unchanged.
///
/// `Patch.from_signals(items, *, registry)` saves the signals of `items`, pairs of a node name and a signal (a dict's
/// `items()`), in the order given, as a patch for `registry`: every thing a signal is made by must be one `registry`
/// holds. A signal a patch cannot hold (an operator, a `Constant`, values
/// given as they are, a `compute`, a `Rendered(run=...)`, a thing `registry` does not hold) or a name no node may have
/// is a `FormatError` listing each as `<path>: reason` (`cannot save as a patch: glow.signal: ...`); `to_toml()` writes
/// the patch as text that reads back, with the same `registry`, as an equal patch.
///
/// `nodes` gives each node by name (a `Node`). `build(bundle=None, only=None)` gives every node (or those named
/// in `only` and none other, building only what they use), in file order: a signal, a tuple of `Event` or a `Series`.
/// A path into the bundle it does not hold (or any, without a bundle) is a `FormatError` at `<node>.<key>`, a name
/// `only` gives that the patch lacks an `UnknownNameError`. A refusal or exception of a node's thing keeps its kind:
/// an `InvalidError` is made again with the node before its message and its problems' paths (`low: ...`, `low.depth`),
/// any other exception reaches the caller unchanged. Two patches are equal when their nodes name the same things with
/// the same references, inputs and values.
#[pyclass(frozen, module = "bullterrier_control._native._core", name = "Patch")]
pub(crate) struct PyPatch {
    patch: Patch,
    /// What the registry held when the patch was read: the `Thing` objects that name a node's refusal.
    held: Arc<Held>,
}

#[pymethods]
impl PyPatch {
    #[staticmethod]
    #[pyo3(signature = (data, *, registry))]
    fn parse(data: &Bound<'_, PyAny>, registry: &Bound<'_, PyRegistry>) -> PyResult<Self> {
        let py = data.py();
        let held = registry.get().held();
        let read = entries(data)?.and_then(|entries| py.detach(|| Patch::parse_with(held.registry(), &entries)));
        let patch = read.map_err(|error| refused(py, error))?;
        Ok(Self { patch, held })
    }

    #[staticmethod]
    #[pyo3(signature = (text, *, registry))]
    fn from_toml(py: Python<'_>, text: &str, registry: &Bound<'_, PyRegistry>) -> PyResult<Self> {
        let held = registry.get().held();
        let patch = py
            .detach(|| Patch::from_toml_with(held.registry(), text))
            .map_err(|error| refused(py, error))?;
        Ok(Self { patch, held })
    }

    #[staticmethod]
    #[pyo3(signature = (items, *, registry))]
    fn from_signals(items: &Bound<'_, PyAny>, registry: &Bound<'_, PyRegistry>) -> PyResult<Self> {
        let py = items.py();
        let held = registry.get().held();
        let given = signals(items)?;
        let refs: Vec<(&str, &Signal)> = given.iter().map(|(name, signal)| (name.as_str(), signal)).collect();
        let patch = py
            .detach(|| Patch::from_signals_with(held.registry(), &refs))
            .map_err(|error| refused(py, error))?;
        Ok(Self { patch, held })
    }

    /// The patch as TOML text, which reads back with the same registry as an equal patch.
    fn to_toml(&self, py: Python<'_>) -> String {
        py.detach(|| self.patch.to_toml())
    }

    /// Each node by name, in file order.
    #[getter]
    fn nodes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (name, node) in self.patch.nodes() {
            dict.set_item(name, PyNode(node.clone()))?;
        }
        Ok(dict)
    }

    #[pyo3(signature = (bundle = None, only = None))]
    fn build<'py>(
        &self,
        py: Python<'py>,
        bundle: Option<&Bound<'py, PyBundle>>,
        only: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let names = only.map(names).transpose()?;
        let bundle = bundle.map(|bundle| bundle.get().bundle());
        let built = py
            .detach(|| match &names {
                None => self.patch.build(bundle),
                Some(names) => self
                    .patch
                    .build_only(bundle, &names.iter().map(String::as_str).collect::<Vec<_>>()),
            })
            .map_err(|error| match error {
                PatchError::Node { node, source } => {
                    let error = match self.thing(&node) {
                        Some(thing) => thing.get().refused(py, source, None),
                        None => raised(py, source),
                    };
                    at_node(py, &node, error)
                }
                other => refused(py, other),
            })?;
        let dict = PyDict::new(py);
        for (name, output) in built {
            dict.set_item(name, registry::python(py, output)?)?;
        }
        Ok(dict)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.patch == other.patch
    }
}

impl PyPatch {
    /// The `Thing` object of what the node `node` uses.
    fn thing(&self, node: &str) -> Option<&Py<registry::PyThing>> {
        let (_, found) = self.patch.nodes().find(|(name, _)| *name == node)?;
        self.held.thing(found.uses())
    }
}

/// One node of a patch, read only: `use`, the name of the thing it uses; `returns`, what it gives when built (`signal`,
/// `events` or `series`); `signal`, the `Reference` it shapes (`None` unless it is a shaper); `inputs`, a read-only
/// mapping of each input's name to a `Reference` or to the data written inline (as the package's classes: a tuple of
/// `Keyframe` or `Event`, a `Grid`, a tuple of numbers, a `Series`), in the order its thing declares them; and
/// `parameters`, a read-only mapping of the values written (those left out are not there).
#[pyclass(frozen, module = "bullterrier_control._native._core", name = "Node")]
pub(crate) struct PyNode(Node);

#[pymethods]
impl PyNode {
    /// The name of the source, shaper or operation it uses.
    #[getter]
    #[pyo3(name = "use")]
    fn uses(&self) -> &str {
        self.0.uses()
    }

    /// What it gives when built: `signal`, `events` or `series`.
    #[getter]
    fn returns(&self) -> &'static str {
        self.0.returns().name()
    }

    /// The signal it shapes, as it writes it; `None` for a source or an operation.
    #[getter]
    fn signal(&self) -> Option<PyReference> {
        self.0.signal().cloned().map(PyReference)
    }

    /// Its inputs by name: a `Reference`, or the data written inline.
    #[getter]
    fn inputs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyMappingProxy>> {
        let dict = PyDict::new(py);
        for (kind, given) in self.0.inputs() {
            let value = match given {
                Given::Reference(reference) => Bound::new(py, PyReference(reference.clone()))?.into_any(),
                Given::Data(input) => inputs::python(py, input.clone())?,
            };
            dict.set_item(kind.name(), value)?;
        }
        Ok(PyMappingProxy::new(py, dict.as_mapping()))
    }

    /// Its parameter values as written, by name.
    #[getter]
    fn parameters<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyMappingProxy>> {
        let dict = PyDict::new(py);
        for (name, value) in self.0.values() {
            dict.set_item(name, params::python(py, value)?)?;
        }
        Ok(PyMappingProxy::new(py, dict.as_mapping()))
    }
}

/// A name written where data goes: a node above (`root` is `"node"`, `name` its name) or an entry of the bundle (`root`
/// is `"events"`, `"series"` or `"grid"`; `name` is the entry, empty for the grid). `text` is how a patch writes it
/// (`glow`, `events.kick`, `series.energy`, `grid`). Two references are equal when they name the same thing.
#[pyclass(frozen, eq, hash, module = "bullterrier_control._native._core", name = "Reference")]
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct PyReference(Reference);

#[pymethods]
impl PyReference {
    /// The reference as a patch writes it.
    #[getter]
    fn text(&self) -> String {
        self.0.to_string()
    }

    /// `"node"`, `"events"`, `"series"` or `"grid"`.
    #[getter]
    const fn root(&self) -> &'static str {
        match self.0 {
            Reference::Node(_) => "node",
            Reference::Events(_) => "events",
            Reference::Series(_) => "series",
            Reference::Grid => "grid",
        }
    }

    /// The node or entry named; empty for the grid.
    #[getter]
    fn name(&self) -> &str {
        match &self.0 {
            Reference::Node(name) | Reference::Events(name) | Reference::Series(name) => name,
            Reference::Grid => "",
        }
    }

    fn __repr__(&self) -> String {
        format!("Reference({:?})", self.0.to_string())
    }
}

/// The JSON Schema of a patch (as decoded from TOML), for tools in other languages: the header, the shape of a node
/// and its name. What a schema cannot state, `Patch.parse` checks.
#[pyfunction]
pub(crate) fn patch_schema(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    params::python(py, &bullterrier_control::patch::patch_schema())
}

/// The Python exception for `error`, a patch that cannot be read or built: a `FormatError` (with the problems it
/// lists), an `UnknownNameError` for a node `only` names that the patch lacks.
fn refused(py: Python<'_>, error: PatchError) -> PyErr {
    let message = error.to_string();
    match error {
        PatchError::Toml { .. } => format_error_saying(py, message, &[]),
        PatchError::Invalid { problems, .. } | PatchError::Unsaved { problems, .. } => {
            format_error_saying(py, message, &problems)
        }
        PatchError::Bundle { problem } => format_error_saying(py, message, &[problem]),
        PatchError::UnknownNode { .. } => unknown_name(py, message),
        PatchError::Node { node, source } => at_node(py, &node, raised(py, source)),
        _ => PyValueError::new_err(message),
    }
}

/// The signals of `items`: pairs of a node name and a signal, in the order given.
fn signals(items: &Bound<'_, PyAny>) -> PyResult<Vec<(String, Signal)>> {
    let refused = |what: String| PyTypeError::new_err(format!("items are pairs of a node name and a signal ({what})"));
    let items = items
        .try_iter()
        .map_err(|_| refused(format!("items is {}, not an iterable", type_name(items))))?;
    items
        .map(|item| {
            let item = item?;
            let pair = item
                .extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>()
                .map_err(|_| refused(format!("got {}, not a pair", type_name(&item))))?;
            let name = pair
                .0
                .extract::<String>()
                .map_err(|_| refused(format!("got {} as a name", type_name(&pair.0))))?;
            let signal = pair
                .1
                .cast::<SignalBase>()
                .map(signal::signal)
                .map_err(|_| refused(format!("got {} as the signal of {name:?}", type_name(&pair.1))))?;
            Ok((name, signal))
        })
        .collect()
}

/// The names `only` gives: an iterable of text (not text itself).
fn names(only: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let refused = || PyTypeError::new_err(format!("only is a list of node names (got {})", type_name(only)));
    if only.cast::<PyString>().is_ok() {
        return Err(refused());
    }
    only.try_iter()?
        .map(|name| name?.extract::<String>().map_err(|_| refused()))
        .collect()
}

/// The entries of `data`, a mapping of the header and the nodes by name, in its order, as the core reads them: each
/// key as text (one that is not text as its `str`), each value as JSON. A value JSON cannot hold refuses the patch,
/// each at its path; a `TypeError` if `data` is not a mapping, and an exception it raises while read unchanged.
fn entries(data: &Bound<'_, PyAny>) -> PyResult<Result<Vec<(String, Value)>, PatchError>> {
    let mapping = data.cast::<PyMapping>().map_err(|_| {
        PyTypeError::new_err(format!(
            "a patch is a mapping of its header and its nodes by name (got {})",
            type_name(data)
        ))
    })?;
    let spoiled = RefCell::new(Problems::new());
    let mut entries = Vec::new();
    for pair in mapping.items()?.iter() {
        let (key, value) = pair.extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>()?;
        let name = match key.cast::<PyString>() {
            Ok(text) => text.to_str().map_or_else(|_| shown(&key), str::to_owned),
            Err(_) => key.str()?.to_string(),
        };
        let seed = Valued {
            path: name.clone(),
            spoiled: &spoiled,
        };
        // a value that cannot be read at all is `null`, its problem in `spoiled`
        let value = data::read(value, |data| seed.deserialize(data))?.unwrap_or(Value::Null);
        entries.push((name, value));
    }
    Ok(match spoiled.into_inner().into_error() {
        Some(error) => {
            let omitted = error.omitted();
            Err(PatchError::Invalid {
                problems: error.problems,
                omitted,
            })
        }
        None => Ok(entries),
    })
}

/// A Python value of a patch at `path`, read as the JSON the core reads: one JSON cannot hold (or [`Data`](data::Data)
/// refuses: a `set`, a key that is not text) is `null`, and its problem is kept in `spoiled`.
struct Valued<'a> {
    path: String,
    spoiled: &'a RefCell<Problems>,
}

impl Valued<'_> {
    /// `null`, the value refused for breaking `rule`.
    fn spoil(&self, rule: String) -> Value {
        self.spoiled.borrow_mut().push(Problem {
            path: self.path.clone(),
            rule,
        });
        Value::Null
    }

    /// The value at `path`, inside this one.
    fn at(&self, path: String) -> Self {
        Self {
            path,
            spoiled: self.spoiled,
        }
    }
}

impl<'de> DeserializeSeed<'de> for Valued<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, data: D) -> Result<Value, D::Error> {
        let kept = self.at(self.path.clone());
        Ok(data
            .deserialize_any(self)
            .unwrap_or_else(|refusal| kept.spoil(refusal.to_string())))
    }
}

impl<'de> Visitor<'de> for Valued<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a value a patch holds (a number, text, true or false, a list or a table)")
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E>(self, flag: bool) -> Result<Value, E> {
        Ok(Value::Bool(flag))
    }

    fn visit_i64<E>(self, whole: i64) -> Result<Value, E> {
        Ok(Value::from(whole))
    }

    fn visit_u64<E>(self, whole: u64) -> Result<Value, E> {
        Ok(i64::try_from(whole).map_or_else(|_| self.spoil(beyond_64_bits(&whole)), Value::from))
    }

    fn visit_f64<E>(self, real: f64) -> Result<Value, E> {
        // as Python writes it: nan, inf, -inf
        let written = if real.is_nan() {
            "nan".to_owned()
        } else {
            real.to_string()
        };
        Ok(Number::from_f64(real).map_or_else(|| self.spoil(format!("must be finite (got {written})")), Value::Number))
    }

    fn visit_str<E>(self, text: &str) -> Result<Value, E> {
        Ok(Value::String(text.to_owned()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<Value, A::Error> {
        let mut read = Vec::new();
        while let Some(item) = items.next_element_seed(self.at(format!("{}[{}]", self.path, read.len())))? {
            read.push(item);
        }
        Ok(Value::Array(read))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut pairs: A) -> Result<Value, A::Error> {
        let mut read = Map::new();
        loop {
            let key = match pairs.next_key::<String>() {
                Ok(Some(key)) => key,
                Ok(None) => break,
                // a key that is not text: its pair is skipped, and the rest are read
                Err(refusal) => {
                    self.spoil(refusal.to_string());
                    continue;
                }
            };
            let value = pairs.next_value_seed(self.at(format!("{}.{key}", self.path)))?;
            read.insert(key, value);
        }
        Ok(Value::Object(read))
    }
}
