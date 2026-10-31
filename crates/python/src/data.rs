//! Python values read as the formats' data: [`Data`], a serde deserializer over a Python object that the core's readers
//! (`Event::from_deserializer`, ...) read exactly as they read JSON, and [`numbers`], the one conversion of an array-like
//! into owned float64 numbers. Arrays a computation reads and gives (times, values, progress) are [`float64s`] and
//! [`shaped`].
//!
//! Python can hold what JSON cannot, and each is refused or turned into its JSON form here, so a Python object is held to
//! the rules a file is: a `bool` is `true` or `false` (never a number), a `str` that cannot be written as UTF-8 (a lone
//! surrogate) is refused, a key is text, an integer beyond 64 bits is refused (one beyond 2^53 is refused by the core
//! where a number is wanted). A `float` NaN or infinity is passed on, so the core lists it beside the other rules broken.
//! A numpy array is read as its own copy (`tolist()`), never as a view of the caller's memory. One of the package's
//! objects (an `Event`, a `Keyframe`, a `Grid`, a `Series`) is read as the data a file holds for it. A value nested
//! more than 128 lists or mappings deep (one that holds itself) is refused, never read until the stack overflows. An
//! exception a value raises while read (a mapping's `__getitem__`) is a refusal with its message, or, through
//! [`read`], raised unchanged.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use numpy::{
    Element, PyArray1, PyArrayDescrMethods, PyArrayDyn, PyArrayMethods, PyUntypedArray, PyUntypedArrayMethods,
};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyMapping, PyString, PyTuple, PyType};
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Unexpected, Visitor};

use bullterrier_control::formats::Problem;
use bullterrier_control::inputs::Keyframe;

use crate::formats::{PyEvent, PyGrid, PySeries, PyUnit};
use crate::inputs::PyKeyframe;

/// The rule a `str` breaks that cannot be written as UTF-8 (it holds a lone surrogate).
pub(crate) const NOT_UTF8: &str = "text must be writable as UTF-8";

/// The largest whole number an `f64` holds exactly (2^53): the core refuses a larger JSON integer where a number is
/// wanted, and [`numbers`] refuses a larger Python one, so two integers never read as one value.
const LARGEST_EXACT: u64 = 1 << 53;

/// The rule an integer breaks that no 64-bit field can hold.
pub(crate) fn beyond_64_bits(whole: &impl fmt::Display) -> String {
    format!("integer {whole} is beyond 64 bits, which no field holds")
}

/// Why a Python value could not be read; the core names the field it was read for.
#[derive(Debug)]
pub(crate) struct Refusal(String);

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Refusal {}

impl de::Error for Refusal {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

/// Where one reading keeps the first Python exception a value raised while it was read, shared by every part read.
type Raised = Rc<RefCell<Option<PyErr>>>;

/// A Python failure while reading (a `tolist()` or `.items()` that raised): its message, as a refusal of that value;
/// the first one of a reading is kept in `raised`.
fn failed(raised: &Raised, error: PyErr) -> Refusal {
    let refusal = Refusal(error.to_string());
    raised.borrow_mut().get_or_insert(error);
    refusal
}

/// `read` run over `value` as [`Data`]: what it made of it, or the Python exception the value raised while it was read
/// (a mapping's `items()`, an array's `tolist()`), unchanged, whatever `read` made of that.
pub(crate) fn read<'py, T>(value: Bound<'py, PyAny>, read: impl FnOnce(Data<'py>) -> T) -> PyResult<T> {
    let data = Data::new(value);
    let raised = Rc::clone(&data.raised);
    let made = read(data);
    raised.take().map_or(Ok(made), Err)
}

/// The most lists, tuples or mappings a value may be nested in. A Python value can nest without bound, or hold itself,
/// and reading recurses once per level, so a deeper value would overflow the stack (about 40,000 levels crash the
/// interpreter): 128 is far beyond any format or parameter value (a bundle's deepest is 4) and far below that.
const DEEPEST: usize = 128;

/// A Python object as serde data: `None` is null, `bool`, `int`, `float` and `str` are themselves, a list or tuple is
/// a sequence, a mapping with text keys an object, a numpy array its `tolist()`, a numpy scalar its `item()`, a
/// `Unit` its name and an `Event`, `Keyframe`, `Grid` or `Series` the object a file holds for it. Anything else is a
/// wrong type for the field it is read for, and a list, tuple or mapping inside [`DEEPEST`] others (a list that holds
/// itself) is refused as nested too deeply.
pub(crate) struct Data<'py> {
    value: Bound<'py, PyAny>,
    /// How many lists, tuples or mappings `value` is inside.
    depth: usize,
    /// The first Python exception raised while reading: once there is one, nothing more is read.
    raised: Raised,
}

impl<'py> Data<'py> {
    /// `value`, read as a whole. A Python exception it raises while read is a refusal with its message; [`read`] raises
    /// it instead.
    pub(crate) fn new(value: Bound<'py, PyAny>) -> Self {
        Self {
            value,
            depth: 0,
            raised: Raised::default(),
        }
    }
}

/// The depth of the items of a list, tuple or mapping that is inside `depth` others, or its refusal: nested too deeply.
fn inside(depth: usize) -> Result<usize, Refusal> {
    if depth < DEEPEST {
        Ok(depth + 1)
    } else {
        Err(Refusal(format!("nested too deeply (limit {DEEPEST})")))
    }
}

impl<'de> Deserializer<'de> for Data<'_> {
    type Error = Refusal;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Refusal> {
        let Self { value, depth, raised } = self;
        let failed = |error| failed(&raised, error);
        if raised.borrow().is_some() {
            Err(Refusal("not read: reading raised an exception".to_owned()))
        } else if value.is_none() {
            visitor.visit_unit()
        } else if let Ok(flag) = value.cast::<PyBool>() {
            visitor.visit_bool(flag.is_true())
        } else if let Ok(whole) = value.cast::<PyInt>() {
            if let Ok(number) = whole.extract::<i64>() {
                visitor.visit_i64(number)
            } else if let Ok(number) = whole.extract::<u64>() {
                visitor.visit_u64(number)
            } else {
                Err(Refusal(beyond_64_bits(&whole)))
            }
        } else if let Ok(real) = value.cast::<PyFloat>() {
            visitor.visit_f64(real.value())
        } else if let Ok(text) = value.cast::<PyString>() {
            visitor.visit_str(text.to_str().map_err(|_| Refusal(NOT_UTF8.to_owned()))?)
        } else if let Ok(list) = value.cast::<PyList>() {
            visitor.visit_seq(Items::new(list.iter().collect(), inside(depth)?, &raised))
        } else if let Ok(tuple) = value.cast::<PyTuple>() {
            visitor.visit_seq(Items::new(tuple.iter().collect(), inside(depth)?, &raised))
        } else if let Ok(dict) = value.cast::<PyDict>() {
            visitor.visit_map(Pairs::new(dict.iter().collect(), inside(depth)?, &raised))
        } else if let Ok(unit) = value.cast::<PyUnit>() {
            visitor.visit_str(unit.get().0.as_str())
        } else if let Some(held) = held(&value)? {
            held.deserialize_any(visitor).map_err(de::Error::custom)
        } else if value.cast::<PyUntypedArray>().is_ok() {
            let items = value.call_method0("tolist").map_err(failed)?;
            let raised = Rc::clone(&raised);
            Data {
                value: items,
                depth,
                raised,
            }
            .deserialize_any(visitor)
        } else if is_numpy_scalar(&value).map_err(failed)? {
            let item = value.call_method0("item").map_err(failed)?;
            // a numpy scalar with no Python equivalent (a long double) gives itself back
            if is_numpy_scalar(&item).map_err(failed)? {
                Err(wrong_type(&value, &visitor))
            } else {
                let raised = Rc::clone(&raised);
                Data {
                    value: item,
                    depth,
                    raised,
                }
                .deserialize_any(visitor)
            }
        } else if let Ok(mapping) = value.cast::<PyMapping>() {
            let inside = inside(depth)?;
            let items = mapping.items().map_err(failed)?;
            let pairs = items
                .iter()
                .map(|pair| pair.extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>())
                .collect::<PyResult<_>>()
                .map_err(failed)?;
            visitor.visit_map(Pairs::new(pairs, inside, &raised))
        } else {
            Err(wrong_type(&value, &visitor))
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Refusal> {
        if self.value.is_none() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit unit_struct
        newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any
    }
}

/// The data a file holds for `value` if it is one of the package's objects (an `Event`, a `Keyframe`, a `Grid`, a
/// `Series`): what it was read from, so it is read again as it was then.
fn held(value: &Bound<'_, PyAny>) -> Result<Option<serde_json::Value>, Refusal> {
    // every number an object holds is finite and every key text, so writing it cannot fail
    let written = if let Ok(event) = value.cast::<PyEvent>() {
        serde_json::to_value(&event.get().0)
    } else if let Ok(frame) = value.cast::<PyKeyframe>() {
        let Keyframe { t, value, ease } = frame.get().0;
        Ok(serde_json::json!({"t": t, "value": value, "ease": ease.to_string()}))
    } else if let Ok(grid) = value.cast::<PyGrid>() {
        serde_json::to_value(&grid.get().0)
    } else if let Ok(series) = value.cast::<PySeries>() {
        serde_json::to_value(series.get().series())
    } else {
        return Ok(None);
    };
    written.map(Some).map_err(de::Error::custom)
}

/// The refusal of `value`, a Python type the field (what `visitor` expects) cannot be: `invalid type: set, expected a
/// number`.
fn wrong_type<'de, V: Visitor<'de>>(value: &Bound<'_, PyAny>, visitor: &V) -> Refusal {
    let name = value
        .get_type()
        .name()
        .map_or_else(|_| "an unknown type".to_owned(), |name| name.to_string());
    de::Error::invalid_type(Unexpected::Other(&name), visitor)
}

/// Whether `value` is a numpy scalar (`numpy.int64(3)`, `numpy.bool_(True)`).
fn is_numpy_scalar(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    static GENERIC: PyOnceLock<Py<PyType>> = PyOnceLock::new();
    value.is_instance(GENERIC.import(value.py(), "numpy", "generic")?)
}

/// The items of a sequence, read in order, each inside `depth` lists, tuples or mappings.
struct Items<'py> {
    /// The items not read yet.
    rest: std::vec::IntoIter<Bound<'py, PyAny>>,
    depth: usize,
    raised: Raised,
}

impl<'py> Items<'py> {
    fn new(items: Vec<Bound<'py, PyAny>>, depth: usize, raised: &Raised) -> Self {
        Self {
            rest: items.into_iter(),
            depth,
            raised: Rc::clone(raised),
        }
    }
}

impl<'de> SeqAccess<'de> for Items<'_> {
    type Error = Refusal;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, Refusal> {
        let depth = self.depth;
        self.rest
            .next()
            .map(|value| {
                let raised = Rc::clone(&self.raised);
                seed.deserialize(Data { value, depth, raised })
            })
            .transpose()
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.rest.len())
    }
}

/// The (key, value) pairs of a mapping, read in its order, each value inside `depth` lists, tuples or mappings. A key
/// must be text: a field or a name is.
struct Pairs<'py> {
    /// The pairs not read yet.
    rest: std::vec::IntoIter<(Bound<'py, PyAny>, Bound<'py, PyAny>)>,
    value: Option<Bound<'py, PyAny>>,
    depth: usize,
    raised: Raised,
}

impl<'py> Pairs<'py> {
    fn new(pairs: Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)>, depth: usize, raised: &Raised) -> Self {
        Self {
            rest: pairs.into_iter(),
            value: None,
            depth,
            raised: Rc::clone(raised),
        }
    }
}

impl<'de> MapAccess<'de> for Pairs<'_> {
    type Error = Refusal;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Refusal> {
        let Some((key, value)) = self.rest.next() else {
            return Ok(None);
        };
        if key.cast::<PyString>().is_err() {
            let shown = key.repr().map_or_else(|_| "?".to_owned(), |shown| shown.to_string());
            return Err(Refusal(format!("a key must be text (got {shown})")));
        }
        self.value = Some(value);
        let (depth, raised) = (self.depth, Rc::clone(&self.raised));
        seed.deserialize(Data {
            value: key,
            depth,
            raised,
        })
        .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Refusal> {
        // serde asks for a value only after its key
        let value = self
            .value
            .take()
            .ok_or_else(|| Refusal("a value was read before its key".to_owned()))?;
        seed.deserialize(Data {
            value,
            depth: self.depth,
            raised: Rc::clone(&self.raised),
        })
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.rest.len())
    }
}

/// Numbers read from an array-like: owned float64 values, row-major, and the shape they had.
pub(crate) struct Numbers {
    /// The values, in row-major order.
    pub(crate) values: Vec<f64>,
    /// The shape of the input: empty for a single number.
    pub(crate) shape: Vec<usize>,
}

/// `value` (a number, a list or tuple, nested or not, or a numpy array of any shape, dtype and memory layout) as owned
/// float64 numbers in a new, aligned, C-contiguous buffer: no reference into the caller's array is kept. A NaN or an
/// infinity is passed on for the caller's rules to refuse. Refused, as the problem at `field` (`field[3]` for an item,
/// counted row-major): a `bool` (a numpy bool array too), text or any other value that is not a number, an integer
/// beyond 2^53, and something that is no array at all.
pub(crate) fn numbers(field: &str, value: &Bound<'_, PyAny>) -> Result<Numbers, Problem> {
    let refused = |rule: String| Problem {
        path: field.to_owned(),
        rule,
    };
    let numpy = value.py().import("numpy").map_err(|error| refused(error.to_string()))?;
    let array = if value.cast::<PyUntypedArray>().is_ok() {
        value.clone()
    } else {
        let object = numpy.getattr("object_").map_err(|error| refused(error.to_string()))?;
        numpy.call_method1("asarray", (value, object)).map_err(|_| {
            refused(format!(
                "must be a number or an array of numbers (got {})",
                shown(value)
            ))
        })?
    };
    let untyped = array
        .cast::<PyUntypedArray>()
        .map_err(|error| refused(error.to_string()))?;
    let shape = untyped.shape().to_vec();
    let values = if untyped.dtype().kind() == b'f' {
        // any float width: a new float64 copy in C order, read whole
        let copy = array
            .call_method1("astype", ("float64", "C"))
            .map_err(|error| refused(error.to_string()))?;
        copy.cast::<PyArrayDyn<f64>>()
            .map_err(|error| refused(error.to_string()))?
            .to_vec()
            .map_err(|error| refused(error.to_string()))?
    } else {
        let items = array
            .call_method0("ravel")
            .and_then(|flat| flat.call_method0("tolist"))
            .map_err(|error| refused(error.to_string()))?;
        let items = items.cast::<PyList>().map_err(|error| refused(error.to_string()))?;
        let mut values = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let number = number(&item).map_err(|rule| Problem {
                path: if shape.is_empty() {
                    field.to_owned()
                } else {
                    format!("{field}[{index}]")
                },
                rule,
            })?;
            values.push(number);
        }
        values
    };
    Ok(Numbers { values, shape })
}

/// One item of an array as a float64, or the rule it breaks.
fn number(item: &Bound<'_, PyAny>) -> Result<f64, String> {
    if item.cast::<PyBool>().is_err()
        && let Ok(whole) = item.cast::<PyInt>()
    {
        let exact = whole
            .extract::<i64>()
            .ok()
            .filter(|number| number.unsigned_abs() <= LARGEST_EXACT);
        #[expect(clippy::cast_precision_loss, reason = "at most 2^53 in size: exact")]
        return exact
            .map(|number| number as f64)
            .ok_or_else(|| format!("integer {whole} is beyond 2^53, which a number cannot hold exactly"));
    }
    if let Ok(real) = item.cast::<PyFloat>() {
        return Ok(real.value());
    }
    if is_numpy_scalar(item).map_err(|error| error.to_string())? {
        let plain = item.call_method0("item").map_err(|error| error.to_string())?;
        // a numpy scalar with no Python equivalent (a long double) gives itself back
        if !is_numpy_scalar(&plain).map_err(|error| error.to_string())? {
            return number(&plain);
        }
    }
    Err(format!("must be a finite number (got {})", shown(item)))
}

/// `value` as Python shows it (`repr`), cut to a length a message can carry.
pub(crate) fn shown(value: &Bound<'_, PyAny>) -> String {
    const SHOWN: usize = 60;
    let text = value.repr().map_or_else(|_| "?".to_owned(), |shown| shown.to_string());
    if text.chars().count() > SHOWN {
        format!("{}...", text.chars().take(SHOWN).collect::<String>())
    } else {
        text
    }
}

/// `values` (anything numpy makes float64 of: an array of any dtype, shape and layout, a list, a number) as float64
/// numbers in row-major order, and the shape numpy gave them. numpy copies them into an array no other code holds
/// before Rust reads it, so no other thread can change them meanwhile; numpy's own error if it cannot.
pub(crate) fn float64s<'py>(values: &Bound<'py, PyAny>) -> PyResult<(Vec<f64>, Bound<'py, PyAny>)> {
    let py = values.py();
    let layout = PyDict::new(py);
    layout.set_item("dtype", "float64")?;
    layout.set_item("order", "C")?;
    let copy = py
        .import("numpy")?
        .call_method("array", (values,), Some(&layout))?
        .cast_into::<PyArrayDyn<f64>>()?;
    let shape = copy.getattr("shape")?;
    Ok((copy.to_vec()?, shape))
}

/// `values` as a numpy array of `shape` (a tuple numpy gave, of as many items in all).
pub(crate) fn shaped<'py, T: Element>(
    py: Python<'py>,
    values: Vec<T>,
    shape: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    PyArray1::from_vec(py, values).call_method1("reshape", (shape,))
}
