//! The core's primitives for Python, each over numpy arrays of any shape (the result has the input's shape, 0-d for a
//! number): `ease` (and `EASES`), `uniform`, `at_least` and `more_than` (and `TOLERANCE`).

use bullterrier_control::easing::Ease;
use bullterrier_control::{hashing, rounding};
use numpy::{PyArrayDyn, PyArrayMethods, PyUntypedArray};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyTuple};

use crate::data::{float64s, shaped, shown};
use crate::error;

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

/// Whether `value` reaches `bound`, or is short of it by no more than `TOLERANCE`: two numbers give a `bool`, else
/// (either an array) an array of bools of the shape numpy broadcasts them to.
#[pyfunction]
pub(crate) fn at_least<'py>(value: &Bound<'py, PyAny>, bound: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    decided(value, bound, rounding::at_least)
}

/// Whether `value` is beyond `bound` by more than `TOLERANCE`; numbers and arrays as for `at_least`.
#[pyfunction]
pub(crate) fn more_than<'py>(value: &Bound<'py, PyAny>, bound: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    decided(value, bound, rounding::more_than)
}

/// `decide(value, bound)`: a `bool` for two numbers, an array of bools for arrays, broadcast against each other.
fn decided<'py>(
    value: &Bound<'py, PyAny>,
    bound: &Bound<'py, PyAny>,
    decide: impl Fn(f64, f64) -> bool,
) -> PyResult<Bound<'py, PyAny>> {
    let py = value.py();
    if value.cast::<PyUntypedArray>().is_err() && bound.cast::<PyUntypedArray>().is_err() {
        return Ok(PyBool::new(py, decide(value.extract()?, bound.extract()?))
            .to_owned()
            .into_any());
    }
    let pair = py.import("numpy")?.call_method1("broadcast_arrays", (value, bound))?;
    let (values, shape) = float64s(&pair.get_item(0)?)?;
    let (bounds, _) = float64s(&pair.get_item(1)?)?;
    shaped(
        py,
        values
            .into_iter()
            .zip(bounds)
            .map(|(value, bound)| decide(value, bound))
            .collect::<Vec<bool>>(),
        &shape,
    )
}
