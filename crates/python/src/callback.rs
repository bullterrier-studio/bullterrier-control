//! Python code a Rust signal calls: a subclass's `compute` ([`computed`]) and a `Rendered`'s `run` ([`rendered`]).
//!
//! Each call attaches to Python for as long as the Python code runs and no longer; no Rust lock is held meanwhile, so
//! the code may read signals again, from this thread or another. What the code is given is a fresh numpy array of its
//! own; what it returns is copied (as float64, whatever its dtype or layout) before Rust reads it, so an array it keeps
//! and changes later changes nothing read. An exception it raises is the error of the read, and reaches the caller of
//! `sample` unchanged. numpy's floating-point warnings are off while it runs: a value that is not finite is refused by
//! the read as a `SignalError`, not warned about. Each is a callback of the core, which counts the callbacks nested on
//! a thread (with thing bodies, Python's and Rust's) and refuses one too deep with a `SignalError` before the stack
//! can overflow (Python's own recursion limit counts its frames, not the stack).

use bullterrier_control::{Error, Rendering, Signal};
use numpy::PyArray1;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::data::float64s;

/// The signal whose values `instance.compute(times)` gives: called with a fresh 1-D float64 array of finite times
/// from 0 up, returning values of its shape (anything numpy makes float64 of). The signal keeps `instance` alive.
pub(crate) fn computed(instance: Py<PyAny>) -> Signal {
    Signal::from_fn(move |times, out| {
        Python::attach(|py| {
            let given = PyArray1::from_slice(py, times);
            let values = quietly(py, || instance.bind(py).call_method1("compute", (given,)))?;
            let values = numbers(&values, times.len(), |shape| {
                format!(
                    "compute of {} returned an array of shape {shape} for times of shape ({},)",
                    instance.bind(py).get_type(),
                    times.len()
                )
            })?;
            out.copy_from_slice(&values);
            Ok::<(), PyErr>(())
        })
    })
}

/// [`Signal::rendered`] over `input` with `run(inputs)` as its recurrence: called with a fresh 1-D, C-contiguous
/// float64 array of the input's samples, returning as many outputs. The signal keeps `run` alive.
///
/// # Errors
/// As [`Signal::rendered`].
pub(crate) fn rendered(input: &Signal, rendering: Rendering, run: Py<PyAny>) -> Result<Signal, Error> {
    Signal::rendered(input, rendering, move |inputs| {
        Python::attach(|py| {
            let given = PyArray1::from_slice(py, inputs);
            let outputs = quietly(py, || run.bind(py).call1((given,)))?;
            numbers(&outputs, inputs.len(), |shape| {
                format!(
                    "the recurrence returned an array of shape {shape} for an input of shape ({},)",
                    inputs.len()
                )
            })
        })
    })
}

/// What `call` gives, called with numpy's floating-point warnings off (`numpy.errstate(all="ignore")`, made for this
/// call: one is not reentrant, and calls nest).
fn quietly<'py>(py: Python<'py>, call: impl FnOnce() -> PyResult<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    let ignore = PyDict::new(py);
    ignore.set_item("all", "ignore")?;
    let quiet = py.import("numpy")?.call_method("errstate", (), Some(&ignore))?;
    quiet.call_method0("__enter__")?;
    let called = call();
    quiet.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
    called
}

/// `values` as `length` float64 numbers ([`float64s`]); a `ValueError` with `wrong(shape)` if numpy gives another
/// shape than `(length,)`.
fn numbers(values: &Bound<'_, PyAny>, length: usize, wrong: impl FnOnce(String) -> String) -> PyResult<Vec<f64>> {
    let (numbers, shape) = float64s(values)?;
    if shape.extract::<Vec<usize>>()? != [length] {
        return Err(PyValueError::new_err(wrong(shape.repr()?.to_string())));
    }
    Ok(numbers)
}
