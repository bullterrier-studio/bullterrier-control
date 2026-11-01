//! The core's parameter tables for Python: a built-in's and a plugin's (`ParamTable`, from a list of declarations or
//! a thing's `params`). Every rule is the table's (`bullterrier_control::params`): the bindings read Python values as
//! the JSON a table takes and refuse what JSON cannot hold (NaN, an infinity, an integer beyond 64 bits, a lone
//! surrogate) as a problem of its field, listed with every other rule broken.
//!
//! Concurrency: `ParamTable` is frozen and holds no lock; nothing here runs without the GIL (a table's work is small).

use std::fmt;
use std::str::FromStr;

use pyo3::IntoPyObjectExt;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyMapping, PyString, PyTuple};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use bullterrier_control::formats::Problem;
use bullterrier_control::params::{Kind, Param, ParamTable, ParamUnit};

use crate::data::{Data, NOT_UTF8, beyond_64_bits, shown};
use crate::error::parameter_error;
use crate::formats::type_name;

/// The kinds a declaration names, as `kind` spells them.
const KINDS: &str = "float, whole, switch, text, choice";

/// A Python value as the JSON a table reads, through [`Data`]; what JSON cannot hold is refused, never turned into
/// something else (`serde_json` would make a NaN `null`).
struct Json(Value);

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(JsonVisitor).map(Self)
    }
}

/// Builds the [`Json`] of what [`Data`] reads.
struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a parameter value (a number, true or false, or text)")
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

    fn visit_u64<E: de::Error>(self, whole: u64) -> Result<Value, E> {
        // a whole number is an i64: one above it is refused for any field, a decimal included
        i64::try_from(whole)
            .map(Value::from)
            .map_err(|_| E::custom(beyond_64_bits(&whole)))
    }

    fn visit_f64<E: de::Error>(self, real: f64) -> Result<Value, E> {
        Number::from_f64(real)
            .map(Value::Number)
            .ok_or_else(|| E::custom(format!("must be a finite number (got {real})")))
    }

    fn visit_str<E>(self, text: &str) -> Result<Value, E> {
        Ok(Value::String(text.to_owned()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<Value, A::Error> {
        let mut read = Vec::new();
        while let Some(Json(item)) = items.next_element()? {
            read.push(item);
        }
        Ok(Value::Array(read))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut pairs: A) -> Result<Value, A::Error> {
        let mut read = Map::new();
        while let Some((key, Json(value))) = pairs.next_entry::<String, Json>()? {
            read.insert(key, value);
        }
        Ok(Value::Object(read))
    }
}

/// `value` as JSON, or the rule it breaks: a NaN or an infinity, an integer beyond 64 bits (signed), a `str` with a
/// lone surrogate, or a type with no JSON form (a `set`).
fn json(value: &Bound<'_, PyAny>) -> Result<Value, String> {
    Json::deserialize(Data::new(value.clone()))
        .map(|Json(read)| read)
        .map_err(|refusal| refusal.to_string())
}

/// `value` as Python data: `None`, `bool`, `int`, `float`, `str`, `list` and `dict`.
pub(crate) fn python<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Value::Null => Ok(py.None().into_bound(py)),
        Value::Bool(flag) => flag.into_bound_py_any(py),
        Value::Number(number) => match (number.as_i64(), number.as_u64()) {
            (Some(whole), _) => whole.into_bound_py_any(py),
            (None, Some(whole)) => whole.into_bound_py_any(py),
            // not an integer: a decimal (`as_f64` is `Some`)
            (None, None) => number.as_f64().into_bound_py_any(py),
        },
        Value::String(text) => text.into_bound_py_any(py),
        Value::Array(items) => {
            let items = items
                .iter()
                .map(|item| python(py, item))
                .collect::<PyResult<Vec<_>>>()?;
            Ok(PyList::new(py, items)?.into_any())
        }
        Value::Object(pairs) => {
            let dict = PyDict::new(py);
            for (key, item) in pairs {
                dict.set_item(key, python(py, item)?)?;
            }
            Ok(dict.into_any())
        }
    }
}

/// The description of `table` (see [`PyParamTable::describe`]).
fn described<'py>(py: Python<'py>, table: &ParamTable) -> PyResult<Bound<'py, PyDict>> {
    let properties = table.describe();
    let dict = PyDict::new(py);
    for param in table.listed() {
        let field = python(py, &properties[param.name.as_str()])?;
        if let Some(range) = param.range_text() {
            field.set_item("range", range)?;
        }
        dict.set_item(&param.name, field)?;
    }
    Ok(dict)
}

/// A problem at `path`.
fn problem(path: &str, rule: String) -> Problem {
    Problem {
        path: path.to_owned(),
        rule,
    }
}

/// The values `given` sets in `table` (see [`PyParamTable::parse`]); a refusal's message starts with `about`.
fn parse<'py>(
    py: Python<'py>,
    table: &ParamTable,
    about: &str,
    given: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let Ok(mapping) = given.cast::<PyMapping>() else {
        let rule = format!(
            "must be a mapping of parameter values by name (got {})",
            type_name(given)
        );
        return Err(parameter_error(py, about, &[problem("", rule)], 0));
    };
    let mut pairs = Vec::new();
    let mut keys = Vec::new();
    for pair in mapping.items()?.iter() {
        let (key, value) = pair.extract::<(Bound<'py, PyAny>, Bound<'py, PyAny>)>()?;
        match text("a field name", &key) {
            Ok(name) => pairs.push((name, value)),
            Err(rule) => keys.push(problem("", rule)),
        }
    }
    if !keys.is_empty() {
        return Err(parameter_error(py, about, &keys, 0));
    }
    let given = Given::read(table, pairs);
    match table.parse(&given.object()) {
        Ok(values) => {
            let dict = PyDict::new(py);
            for param in table.listed() {
                if let Some(value) = values.get(&param.name) {
                    dict.set_item(&param.name, python(py, value)?)?;
                }
            }
            Ok(dict)
        }
        Err(mut error) => {
            given.explain(&mut error.problems);
            Err(parameter_error(py, about, &error.problems, error.omitted()))
        }
    }
}

/// Parameter values given by name, as the JSON object a table reads. A value JSON cannot hold (NaN, an infinity, an
/// integer beyond 64 bits, a lone surrogate, a `set`) is read as `null`, which every field refuses; the rule it breaks
/// is kept, so the table's refusal of that `null` can say why instead ([`Given::explain`]).
pub(crate) struct Given {
    object: Map<String, Value>,
    /// The rule each value JSON cannot hold breaks, by the name of the field the table has.
    unreadable: Vec<(String, String)>,
}

impl Given {
    /// The values `pairs` gives `table`, by field name.
    pub(crate) fn read<'py>(table: &ParamTable, pairs: impl IntoIterator<Item = (String, Bound<'py, PyAny>)>) -> Self {
        let mut object = Map::new();
        let mut unreadable = Vec::new();
        for (name, value) in pairs {
            let read = json(&value).unwrap_or_else(|rule| {
                // an unknown field is refused as unknown, whatever its value
                if table.params().iter().any(|param| param.name == name) {
                    unreadable.push((name.clone(), rule));
                }
                Value::Null
            });
            object.insert(name, read);
        }
        Self { object, unreadable }
    }

    /// The values, as a JSON object by field name.
    pub(crate) fn object(&self) -> Value {
        Value::Object(self.object.clone())
    }

    /// `problems`, a table's refusal of these values, each refusing a value JSON cannot hold saying why.
    pub(crate) fn explain(&self, problems: &mut [Problem]) {
        for problem in problems {
            if let Some((_, rule)) = self.unreadable.iter().find(|(name, _)| *name == problem.path) {
                problem.rule.clone_from(rule);
            }
        }
    }
}

/// `value` as text, or the rule it breaks as `what` (`"doc"`): not a `str`, or one with a lone surrogate.
fn text(what: &str, value: &Bound<'_, PyAny>) -> Result<String, String> {
    let text = value
        .cast::<PyString>()
        .map_err(|_| format!("{what} must be text (got {})", shown(value)))?;
    text.to_str()
        .map(str::to_owned)
        .map_err(|_| format!("{what}: {NOT_UTF8}"))
}

/// `value` as one end of a range, `what` (`"min"`), or the rule it breaks: `None` for no end, else a number (an
/// `int` or a `float`, never a `bool`).
fn end(what: &str, value: &Bound<'_, PyAny>) -> Result<Option<f64>, String> {
    if value.is_none() {
        return Ok(None);
    }
    let refused = || format!("{what} must be a number (got {})", shown(value));
    if value.cast::<PyBool>().is_ok() {
        return Err(refused());
    }
    value.extract::<f64>().map(Some).map_err(|_| refused())
}

/// `value` as a flag, `what` (`"advanced"`), or the rule it breaks: `True` or `False` only.
fn flag(what: &str, value: &Bound<'_, PyAny>) -> Result<bool, String> {
    value
        .cast::<PyBool>()
        .map(PyBoolMethods::is_true)
        .map_err(|_| format!("{what} must be true or false (got {})", shown(value)))
}

/// `value` as the names of a choice, or the rule it breaks: a list or tuple of text.
fn names(value: &Bound<'_, PyAny>) -> Result<Vec<String>, String> {
    let refused = || format!("choices must be a list of names (got {})", shown(value));
    let items: Vec<Bound<'_, PyAny>> = if let Ok(list) = value.cast::<PyList>() {
        list.iter().collect()
    } else if let Ok(tuple) = value.cast::<PyTuple>() {
        tuple.iter().collect()
    } else {
        return Err(refused());
    };
    items
        .iter()
        .map(|item| text("a choice", item).map_err(|_| refused()))
        .collect()
}

/// The declarations of `spec` (see [`PyParamTable::new`]) as the core's, or every problem in reading them.
fn declarations(spec: &Bound<'_, PyAny>) -> Result<Vec<Param>, Vec<Problem>> {
    let entries: Vec<Bound<'_, PyAny>> = if let Ok(list) = spec.cast::<PyList>() {
        list.iter().collect()
    } else if let Ok(tuple) = spec.cast::<PyTuple>() {
        tuple.iter().collect()
    } else {
        let rule = format!("must be a list of parameter declarations (got {})", type_name(spec));
        return Err(vec![problem("", rule)]);
    };
    let mut params = Vec::new();
    let mut problems = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        match declaration(index, entry) {
            Ok(param) => params.push(param),
            Err(mut found) => problems.append(&mut found),
        }
    }
    if problems.is_empty() { Ok(params) } else { Err(problems) }
}

/// The declaration `entry`, at `index` in its list, as the core's, or the problems in reading it: each under the
/// field's name, or `[index]` while it has none.
fn declaration(index: usize, entry: &Bound<'_, PyAny>) -> Result<Param, Vec<Problem>> {
    let at = format!("[{index}]");
    let refused = |rule: String| vec![problem(&at, rule)];
    let Ok(mapping) = entry.cast::<PyMapping>() else {
        return Err(refused(format!(
            "must be a mapping declaring one parameter (got {})",
            type_name(entry)
        )));
    };
    let mut pairs = Vec::new();
    for pair in mapping.items().map_err(|error| refused(error.to_string()))?.iter() {
        let (key, value) = pair
            .extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>()
            .map_err(|error| refused(error.to_string()))?;
        pairs.push((text("a key", &key).map_err(refused)?, value));
    }
    let name = match pairs.iter().find(|(key, _)| key == "name") {
        None => return Err(refused("needs a name".to_owned())),
        Some((_, name)) => text("name", name).map_err(refused)?,
    };
    let mut problems = Vec::new();
    let mut broken = |rule: String| problems.push(problem(&name, rule));
    let mut param = Param {
        name: name.clone(),
        kind: Kind::Text,
        default: Value::Null,
        doc: String::new(),
        unit: None,
        min: None,
        max: None,
        advanced: false,
        cyclic: false,
    };
    let (mut kind, mut choices) = (None, None);
    for (key, value) in &pairs {
        let read = match key.as_str() {
            // read above; a unit or choices of `None` are none
            "name" => Ok(()),
            "unit" | "choices" if value.is_none() => Ok(()),
            "kind" => text("kind", value).map(|read| kind = Some(read)),
            "default" => json(value)
                .map(|read| param.default = read)
                .map_err(|rule| format!("the default is not valid: {rule}")),
            "doc" => text("doc", value).map(|read| param.doc = read),
            "unit" => text("unit", value).and_then(|read| {
                let unit = ParamUnit::from_str(&read).map_err(|error| {
                    let rules: Vec<String> = error.problems.into_iter().map(|problem| problem.rule).collect();
                    rules.join("; ")
                })?;
                param.unit = Some(unit);
                Ok(())
            }),
            "min" => end("min", value).map(|read| param.min = read),
            "max" => end("max", value).map(|read| param.max = read),
            "advanced" => flag("advanced", value).map(|read| param.advanced = read),
            "cyclic" => flag("cyclic", value).map(|read| param.cyclic = read),
            "choices" => names(value).map(|read| choices = Some(read)),
            other => Err(format!("unknown key {other:?} in a declaration")),
        };
        if let Err(rule) = read {
            broken(rule);
        }
    }
    let kind = match (kind.as_deref(), choices) {
        (None, _) => Err(format!("needs a kind, one of {KINDS}")),
        (Some("choice"), choices) => Ok(Kind::Choice(choices.unwrap_or_default())),
        (Some(_), Some(_)) => Err("choices are only for a choice".to_owned()),
        (Some("float"), None) => Ok(Kind::Float),
        (Some("whole"), None) => Ok(Kind::Whole),
        (Some("switch"), None) => Ok(Kind::Switch),
        (Some("text"), None) => Ok(Kind::Text),
        (Some(other), None) => Err(format!("kind must be one of {KINDS} (got {other:?})")),
    };
    match kind {
        Ok(kind) => param.kind = kind,
        Err(rule) => broken(rule),
    }
    if problems.is_empty() { Ok(param) } else { Err(problems) }
}

/// The parameters of a source, shaper or operation: a table of fields, each declaration checked once, here.
///
/// `spec` lists one mapping per field, in declaration order: `name`, `kind` (`"float"`, `"whole"`, `"switch"`,
/// `"text"` or `"choice"`), `default`, `doc`, and as the kind needs `unit` (a unit's name: `"seconds"`, ...), `min`
/// and `max` (numbers), `choices` (a list of names, for a choice), `advanced` and `cyclic` (`True` or `False`, both
/// `False` when left out). A declaration that breaks a rule (a number without a unit, a range that does not hold the
/// default, a discouraged name, ...) is a `ParameterError` listing every rule broken under its field's name, the
/// message starting with `name` (the class the table is for); one that cannot be read (an unknown key, kind or unit,
/// a default JSON cannot hold) is refused before the rules are checked.
#[pyclass(frozen, module = "bullterrier_control", name = "ParamTable")]
pub(crate) struct PyParamTable {
    table: ParamTable,
    name: String,
}

impl PyParamTable {
    /// `table`, its refusals' messages starting with `name`.
    pub(crate) const fn of(table: ParamTable, name: String) -> Self {
        Self { table, name }
    }

    /// The table.
    pub(crate) const fn table(&self) -> &ParamTable {
        &self.table
    }

    /// What a refusal's message starts with: the class the table is for, or the built-in's name.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

#[pymethods]
impl PyParamTable {
    #[new]
    #[pyo3(signature = (spec, *, name))]
    fn new(py: Python<'_>, spec: &Bound<'_, PyAny>, name: String) -> PyResult<Self> {
        let params = declarations(spec).map_err(|problems| parameter_error(py, &name, &problems, 0))?;
        let table =
            ParamTable::new(params).map_err(|error| parameter_error(py, &name, &error.problems, error.omitted()))?;
        Ok(Self { table, name })
    }

    /// The values `values` (a mapping of values by field name) sets, as a dict of every field (a field left out
    /// takes its default) in the order `describe` lists them. Read strictly: a number is an `int` or a `float` (not
    /// a `bool`, not `"1"`), a whole number an `int` (not `1.0`), a switch a `bool` (not `1`); a number is finite,
    /// within 64 bits and in its range (a cyclic one outside it is wrapped into it: 1.25 on 0 to 1 is 0.25), a choice
    /// one of its names, text writable as UTF-8; a field the table does not have is refused.
    ///
    /// A `ParameterError` lists every rule broken, each field's in table order, then each unknown field; a key that
    /// is not text, or `values` not a mapping, is refused before any value is read.
    fn parse<'py>(&self, py: Python<'py>, values: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
        parse(py, &self.table, &self.name, values)
    }

    /// The fields as a consumer reads them, by name: the basic fields, then the advanced ones, each in declaration
    /// order. Each is a JSON Schema property (`type`, `enum` for a choice or `const` for a choice of one, `default`,
    /// `description`, `title`, `unit`, `advanced`, `cyclic`, and for a number `minimum` and `maximum`) and, for a
    /// number, `range`: the range as help text shows it (`"0 to 3600"`).
    fn describe<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        described(py, &self.table)
    }
}
