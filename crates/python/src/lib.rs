//! `bullterrier_control._native._core`: runs the shapers of the Rust core on numpy series. Generic: it names no shaper,
//! so porting one never changes it.

use numpy::{PyArray1, PyArrayDescrMethods, PyArrayMethods, PyUntypedArray, PyUntypedArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyList, PyMapping, PyString, PyTuple};
use serde_json::Value;

/// The shapers of bullterrier-control's Rust core, run on numpy series.
#[pymodule]
mod _core {
    #[pymodule_export]
    use super::{render, shapers};
}

/// Run the shaper called `name` over `inputs` (its input sampled at the shaper's rate from time 0: a 1-D, C-contiguous
/// float64 array, read in place) with `params` (exactly the shaper's parameters, validated); a new float64 array of
/// the same length. The shaper runs without the GIL.
///
/// `ValueError` if the name, the parameters or the array are not what the core expects: a programming error, since the
/// package validates a user's values before they get here.
#[pyfunction]
fn render<'py>(
    py: Python<'py>,
    name: &str,
    inputs: &Bound<'py, PyAny>,
    params: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let readonly = series(inputs)?.try_readonly().map_err(value_error)?;
    let values = readonly.as_slice().map_err(value_error)?;
    let params = to_json(params)?;
    let rendered = py
        .detach(|| bullterrier_control::render(name, values, &params))
        .map_err(value_error)?;
    Ok(PyArray1::from_vec(py, rendered))
}

/// The names of the shapers the core implements, sorted.
#[pyfunction]
fn shapers() -> Vec<&'static str> {
    bullterrier_control::shapers()
}

/// A `ValueError` carrying `error`'s message.
#[expect(clippy::needless_pass_by_value, reason = "map_err passes the error by value")]
fn value_error(error: impl ToString) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// `inputs` as the array the core reads in place, or a `ValueError` naming the rule it breaks: a numpy array, 1-D,
/// float64, C-contiguous.
fn series<'a, 'py>(inputs: &'a Bound<'py, PyAny>) -> PyResult<&'a Bound<'py, PyArray1<f64>>> {
    let array = inputs
        .cast::<PyUntypedArray>()
        .map_err(|_| PyValueError::new_err(format!("inputs must be a numpy array, got {}", inputs.get_type())))?;
    let rule = if array.ndim() != 1 {
        Some(format!("inputs must be 1-D, got shape {:?}", array.shape()))
    } else if !array.dtype().is_equiv_to(&numpy::dtype::<f64>(inputs.py())) {
        Some(format!("inputs must be float64, got {}", array.dtype()))
    } else if !array.is_c_contiguous() {
        Some("inputs must be C-contiguous, got a strided view".to_owned())
    } else {
        None
    };
    match rule {
        Some(message) => Err(PyValueError::new_err(message)),
        None => inputs.cast::<PyArray1<f64>>().map_err(value_error),
    }
}

/// `value` (parameters as Pydantic's `model_dump` gives them) as JSON, for the core to read with serde.
fn to_json(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if value.is_none() {
        Ok(Value::Null)
    } else if let Ok(flag) = value.cast::<PyBool>() {
        Ok(Value::Bool(flag.is_true()))
    } else if let Ok(whole) = value.cast::<PyInt>() {
        let number = whole
            .extract::<i64>()
            .map_err(|_| PyValueError::new_err(format!("a parameter is out of range for 64 bits: {whole}")))?;
        Ok(Value::from(number))
    } else if let Ok(real) = value.cast::<PyFloat>() {
        serde_json::Number::from_f64(real.value())
            .map(Value::Number)
            .ok_or_else(|| PyValueError::new_err(format!("a parameter is not finite: {}", real.value())))
    } else if let Ok(text) = value.cast::<PyString>() {
        Ok(Value::String(text.to_str()?.to_owned()))
    } else if let Ok(list) = value.cast::<PyList>() {
        list.iter()
            .map(|item| to_json(&item))
            .collect::<PyResult<_>>()
            .map(Value::Array)
    } else if let Ok(tuple) = value.cast::<PyTuple>() {
        tuple
            .iter()
            .map(|item| to_json(&item))
            .collect::<PyResult<_>>()
            .map(Value::Array)
    } else if let Ok(mapping) = value.cast::<PyMapping>() {
        mapping
            .items()?
            .iter()
            .map(|item| {
                let (key, item) = item.extract::<(String, Bound<'_, PyAny>)>()?;
                Ok((key, to_json(&item)?))
            })
            .collect::<PyResult<_>>()
            .map(Value::Object)
    } else {
        Err(PyValueError::new_err(format!(
            "a parameter of type {} has no JSON form",
            value.get_type()
        )))
    }
}
