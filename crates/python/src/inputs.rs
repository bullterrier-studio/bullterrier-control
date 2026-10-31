//! The core's inputs for Python: `Keyframe`, `check_input` and `input_docs`. Every rule is the core's
//! (`bullterrier_control::inputs`): the bindings read a Python object as the data the core checks ([`Data`]) and give
//! the checked input back as the package's classes. A broken rule is a `ParameterError` listing every one, under the
//! path from the input's name (`points[1].ease`).
//!
//! Concurrency: `Keyframe` is frozen and holds no lock; nothing here runs without the GIL (it reads Python objects).

use std::hash::{DefaultHasher, Hash, Hasher};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple, PyType};

use bullterrier_control::Error;
use bullterrier_control::easing::Ease;
use bullterrier_control::inputs::{Input, InputKind, Keyframe};

use crate::data;
use crate::error::{parameter_error, raised};
use crate::formats::{PyEvent, PyGrid, PySeries, feed, fields, shown};
use crate::value_error;

/// One point of a keyframed signal: the `value` at `t` seconds, and the `ease` of the segment that starts here
/// (ignored on the last point; default linear).
///
/// `t >= 0` and `value` are finite numbers; `ease` is a name in `EASES` or `cubic-bezier(x1, y1, x2, y2)`. A broken
/// rule is a `ParameterError` listing every one.
#[pyclass(frozen, module = "bullterrier_control", name = "Keyframe")]
pub(crate) struct PyKeyframe(pub(crate) Keyframe);

impl PyKeyframe {
    fn read(data: Bound<'_, PyAny>) -> PyResult<Self> {
        let py = data.py();
        data::read(data, Keyframe::from_deserializer)?
            .map(Self)
            .map_err(|error| refused(py, "Keyframe", error))
    }

    /// The keyframe as data holds it: `{"t": 0.0, "value": 1.0, "ease": "linear"}`.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("t", self.0.t)?;
        dict.set_item("value", self.0.value)?;
        dict.set_item("ease", self.0.ease.to_string())?;
        Ok(dict)
    }
}

#[pymethods]
impl PyKeyframe {
    #[new]
    #[pyo3(signature = (*, t, value, ease = None), text_signature = "(*, t, value, ease='linear')")]
    fn new<'py>(
        py: Python<'py>,
        t: Bound<'py, PyAny>,
        value: Bound<'py, PyAny>,
        ease: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        let given = [("t", Some(t)), ("value", Some(value)), ("ease", ease)];
        Self::read(fields(py, &given)?.into_any())
    }

    /// The keyframe in `data` (a mapping of `t`, `value` and, if not linear, `ease`): a `ParameterError` lists every
    /// rule broken.
    #[classmethod]
    fn parse(_class: &Bound<'_, PyType>, data: Bound<'_, PyAny>) -> PyResult<Self> {
        Self::read(data)
    }

    /// When, in seconds from 0.
    #[getter]
    const fn t(&self) -> f64 {
        self.0.t
    }

    /// The value there.
    #[getter]
    const fn value(&self) -> f64 {
        self.0.value
    }

    /// The curve of the segment that starts here, as the text that names it (`in-out`, `cubic-bezier(0, 0, 1, 1)`).
    #[getter]
    fn ease(&self) -> String {
        self.0.ease.to_string()
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        feed(&mut hasher, self.0.t);
        feed(&mut hasher, self.0.value);
        match self.0.ease {
            // equal curves hash equally: a handle of 0 and one of -0 are the same curve
            Ease::Bezier { x1, y1, x2, y2 } => [x1, y1, x2, y2].into_iter().for_each(|x| feed(&mut hasher, x)),
            named => named.to_string().hash(&mut hasher),
        }
        hasher.finish()
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!(
            "Keyframe(t={}, value={}, ease={})",
            shown(py, self.0.t)?,
            shown(py, self.0.value)?,
            shown(py, self.0.ease.to_string())?
        ))
    }

    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyAny>, (Bound<'py, PyDict>,))> {
        Ok((slf.get_type().getattr("parse")?, (slf.get().to_dict(slf.py())?,)))
    }
}

/// `value` checked as the input `kind` (`points`, `events`, `grid`, `values` or `series`): keyframes as a tuple of
/// `Keyframe`, events as a tuple of `Event` in time order (those at the same time in the order given), a `Grid`,
/// values as a tuple of floats, a `Series`. A mapping, a list or tuple, or one of the package's objects (read as the
/// data it holds) is read as the core reads JSON: a `ParameterError` lists every rule broken, under the path from
/// `kind` (`events[2].strength`). An unknown `kind` is a `ValueError`.
#[pyfunction]
pub(crate) fn check_input<'py>(kind: &str, value: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let py = value.py();
    let kind: InputKind = kind.parse().map_err(value_error)?;
    match data::read(value, |data| kind.check(data))?.map_err(|error| raised(py, error))? {
        Input::Points(frames) => Ok(PyTuple::new(py, frames.into_iter().map(PyKeyframe))?.into_any()),
        Input::Events(events) => Ok(PyTuple::new(py, events.into_iter().map(PyEvent))?.into_any()),
        Input::Grid(grid) => Ok(Bound::new(py, PyGrid(grid))?.into_any()),
        Input::Values(values) => Ok(PyTuple::new(py, values)?.into_any()),
        Input::Series(series) => Ok(Bound::new(py, PySeries::wrap(py, series)?)?.into_any()),
        other => Err(value_error(format!("the bindings give no {other:?}"))),
    }
}

/// What `help()` says about each input, by name, in the core's order.
#[pyfunction]
pub(crate) fn input_docs(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let docs = PyDict::new(py);
    for name in InputKind::NAMES {
        let kind: InputKind = name.parse().map_err(value_error)?;
        docs.set_item(name, kind.doc())?;
    }
    Ok(docs)
}

/// The Python exception for `error`, data breaking a keyframe's rules a `ParameterError` about `about` (`Keyframe`).
fn refused(py: Python<'_>, about: &str, error: Error) -> PyErr {
    match error {
        Error::Input { problems, omitted } => parameter_error(py, about, &problems, omitted),
        other => raised(py, other),
    }
}
