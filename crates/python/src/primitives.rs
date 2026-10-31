//! The core's primitives for Python, each over numpy arrays of any shape (the result has the input's shape, 0-d for a
//! number): `ease` (and `EASES`), `uniform`, `between`, `counter`, `at_least` and `more_than` (and `TOLERANCE`).

use bullterrier_control::cadence::{Per, positions};
use bullterrier_control::easing::Ease;
use bullterrier_control::formats::Grid;
use bullterrier_control::{Error, hashing, rounding, scaling};
use numpy::{PyArrayDyn, PyArrayMethods, PyUntypedArray};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyTuple};

use crate::data::{float64s, shaped, shown};
use crate::error;
use crate::formats::PyGrid;

/// Adds the constants to `module`: `EASES`, the names of the named easing curves in their order, and `TOLERANCE`, the
/// seconds within which a time is the bound it is compared with.
pub(crate) fn add_constants(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("EASES", PyTuple::new(module.py(), Ease::NAMES)?)?;
    module.add("TOLERANCE", rounding::TOLERANCE)
}

/// An easing curve: called with progress (a number or an array), it gives the curve at each, progress clipped to
/// 0..1 first; 0 gives exactly 0.0 and 1 exactly 1.0.
#[pyclass(frozen, module = "bullterrier_control._native._core", name = "Ease")]
pub(crate) struct PyEase(Ease);

#[pymethods]
impl PyEase {
    fn __call__<'py>(&self, progress: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let (values, shape) = float64s(progress)?;
        shaped(progress.py(), self.0.apply_all(&values), &shape)
    }
}

/// The easing curve `name`: a name in `EASES` or a `cubic-bezier(x1, y1, x2, y2)` (whitespace is free; x1 and x2 in
/// 0..1, y1 and y2 finite), parsed once. Anything else is an `InvalidError` listing the valid forms.
#[pyfunction]
pub(crate) fn ease(py: Python<'_>, name: &str) -> PyResult<PyEase> {
    name.parse().map(PyEase).map_err(|error| error::raised(py, error))
}

/// The value at each `index` (an array of whole numbers from 0, of any integer dtype: a negative one is a `ValueError`,
/// another dtype a `TypeError`) of the seeded stream of `seed` (0 to 2^64 - 1): a float64 in [0, 1), `SplitMix64`
/// started at state `seed`, the same on every machine.
#[pyfunction]
pub(crate) fn uniform<'py>(seed: u64, index: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let py = index.py();
    let numpy = py.import("numpy")?;
    let raw = numpy.call_method1("asarray", (index,))?;
    let kind: String = raw.getattr("dtype")?.getattr("kind")?.extract()?;
    if !matches!(kind.as_str(), "i" | "u") {
        return Err(PyTypeError::new_err(format!(
            "index must be an array of whole numbers (got {})",
            shown(index)
        )));
    }
    if kind == "i"
        && numpy
            .call_method1("less", (&raw, 0))?
            .call_method0("any")?
            .is_truthy()?
    {
        return Err(PyValueError::new_err("index must not be negative"));
    }
    let layout = PyDict::new(py);
    layout.set_item("dtype", "uint64")?;
    layout.set_item("order", "C")?;
    let copy = numpy
        .call_method("array", (&raw,), Some(&layout))?
        .cast_into::<PyArrayDyn<u64>>()?;
    let values = copy
        .to_vec()?
        .iter()
        .map(|&index| hashing::uniform(seed, index))
        .collect();
    shaped(py, values, &copy.getattr("shape")?)
}

/// Each of `fraction` (0 to 1) as a value from `low` to `high`, kept exactly inside them; `low` above `high` inverts
/// it.
#[pyfunction]
pub(crate) fn between<'py>(low: f64, high: f64, fraction: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let (fractions, shape) = float64s(fraction)?;
    let values = fractions
        .iter()
        .map(|&fraction| scaling::between(low, high, fraction))
        .collect();
    shaped(fraction.py(), values, &shape)
}

/// Where each time falls in a periodic source: called with times (finite, from 0 up), it gives `rate * t` per second
/// or `rate * grid.beat_at(t)` per beat. A position that is not a finite number is a `SignalError`.
#[pyclass(frozen, module = "bullterrier_control._native._core", name = "Counter")]
pub(crate) struct Counter {
    rate: f64,
    per: Per,
    grid: Option<Grid>,
}

#[pymethods]
impl Counter {
    fn __call__<'py>(&self, times: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let py = times.py();
        let (times, shape) = float64s(times)?;
        let counted =
            positions(self.rate, self.per, self.grid.as_ref(), &times).map_err(|error| error::raised(py, error))?;
        shaped(py, counted, &shape)
    }
}

/// The counter of `rate` cycles or steps per `"second"` or per `"beat"` of `grid` (ignored per second). Per beat
/// without a grid is a `ParameterError` naming `per`.
#[pyfunction]
pub(crate) fn counter(py: Python<'_>, rate: f64, per: &str, grid: Option<&Bound<'_, PyGrid>>) -> PyResult<Counter> {
    let per = match per {
        "second" => Per::Second,
        "beat" => Per::Beat,
        other => {
            return Err(PyValueError::new_err(format!(
                "per must be 'second' or 'beat' (got {other:?})"
            )));
        }
    };
    let counter = Counter {
        rate,
        per,
        grid: grid.map(|grid| grid.get().0.clone()),
    };
    // the core decides which counters it can count with: one that counts no times is refused as one that counts any
    positions(counter.rate, counter.per, counter.grid.as_ref(), &[])
        .map_err(|error: Error| error::raised(py, error))?;
    Ok(counter)
}

/// Whether `value` (a number, or an array: then an array of bools) reaches `bound`, or is short of it by no more than
/// `TOLERANCE`.
#[pyfunction]
pub(crate) fn at_least<'py>(value: &Bound<'py, PyAny>, bound: f64) -> PyResult<Bound<'py, PyAny>> {
    decided(value, |value| rounding::at_least(value, bound))
}

/// Whether `value` (a number, or an array: then an array of bools) is beyond `bound` by more than `TOLERANCE`.
#[pyfunction]
pub(crate) fn more_than<'py>(value: &Bound<'py, PyAny>, bound: f64) -> PyResult<Bound<'py, PyAny>> {
    decided(value, |value| rounding::more_than(value, bound))
}

/// `decide` of `value`: a `bool` for a number, an array of bools of its shape for an array.
fn decided<'py>(value: &Bound<'py, PyAny>, decide: impl Fn(f64) -> bool) -> PyResult<Bound<'py, PyAny>> {
    let py = value.py();
    if value.cast::<PyUntypedArray>().is_err() {
        return Ok(PyBool::new(py, decide(value.extract()?)).to_owned().into_any());
    }
    let (values, shape) = float64s(value)?;
    shaped(py, values.into_iter().map(decide).collect::<Vec<bool>>(), &shape)
}
