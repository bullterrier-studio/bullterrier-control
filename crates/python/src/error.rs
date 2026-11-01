//! The core's errors raised as the package's exceptions of `bullterrier_control.errors`: a `FormatError` and a
//! `ParamError` as `FormatError` and `ParameterError`, and an [`Error`] by [`raised`].

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyTuple, PyType};

use bullterrier_control::Error;
use bullterrier_control::formats::{FormatError, Problem};

/// How many problems a message lists; `problems` holds up to the first 100 (the core's cap), and the message says how
/// many more there are.
const SHOWN: usize = 20;

/// The package's `FormatError`, imported when the first one is raised (so importing the extension imports nothing of
/// the package) and kept.
static FORMAT_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// The package's `ParameterError`, imported and kept as [`FORMAT_ERROR`] is.
static PARAMETER_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// The package's `SignalError`, imported and kept as [`FORMAT_ERROR`] is.
static SIGNAL_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// The package's `InvalidError`, imported and kept as [`FORMAT_ERROR`] is.
static INVALID_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// The package's `UnknownNameError`, imported and kept as [`FORMAT_ERROR`] is.
static UNKNOWN_NAME_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// `bullterrier_control.errors.UnknownNameError` with `message`.
pub(crate) fn unknown_name(py: Python<'_>, message: String) -> PyErr {
    package(py, (&UNKNOWN_NAME_ERROR, "UnknownNameError"), message)
}

/// `bullterrier_control.errors.FormatError` with `message` (one the core wrote, listing them) and `problems`.
pub(crate) fn format_error_saying(py: Python<'_>, message: String, problems: &[Problem]) -> PyErr {
    listing(py, (&FORMAT_ERROR, "FormatError"), message, problems)
}

/// `error`, raised while the patch node `node` was built: an `InvalidError` (of any kind) made again as its own kind,
/// its message starting with the node (`low: ...`) and each problem's path too (`low.depth`); any other exception
/// unchanged.
pub(crate) fn at_node(py: Python<'_>, node: &str, error: PyErr) -> PyErr {
    match named(py, node, &error) {
        Ok(Some(named)) => named,
        Ok(None) => error,
        // the package's errors module cannot be imported, or the error cannot be made again: that is the error to see
        Err(failure) => failure,
    }
}

/// `error` made again naming the node `node`, if it is an `InvalidError` (see [`at_node`]).
fn named(py: Python<'_>, node: &str, error: &PyErr) -> PyResult<Option<PyErr>> {
    let value = error.value(py);
    if !value.is_instance(INVALID_ERROR.import(py, "bullterrier_control.errors", "InvalidError")?)? {
        return Ok(None);
    }
    let problems: Vec<(String, String)> = value.getattr("problems")?.extract()?;
    let prefixed = problems
        .into_iter()
        .map(|(path, rule)| (format!("{node}.{path}"), rule));
    let made = value
        .get_type()
        .call1((format!("{node}: {value}"), PyTuple::new(py, prefixed)?))?;
    Ok(Some(PyErr::from_value(made)))
}

/// The Python exception for `error`: a signal that cannot be read, or is too deep, is a `SignalError`, a text naming no
/// easing curve an `InvalidError`, data breaking a format's rules a `FormatError`, parameters or an input breaking theirs
/// a `ParameterError`; a callback's error is the exception the Python callback raised, unchanged. Anything else is a
/// bug of the caller: a `ValueError`.
pub(crate) fn raised(py: Python<'_>, error: Error) -> PyErr {
    match error {
        Error::Signal { message } | Error::Depth { message } => package(py, (&SIGNAL_ERROR, "SignalError"), message),
        Error::Ease { message } => package(py, (&INVALID_ERROR, "InvalidError"), message),
        Error::Format { source } => format_error(py, "", &source),
        Error::Params {
            shaper,
            problems,
            omitted,
        } => parameter_error(py, &shaper, &problems, omitted),
        Error::Input { problems, omitted } => parameter_error(py, "", &problems, omitted),
        // every callback the bindings give the core fails with the Python exception it raised
        Error::Node { source } => match source.downcast_ref::<PyErr>() {
            Some(raised) => raised.clone_ref(py),
            None => PyValueError::new_err(source.to_string()),
        },
        other => PyValueError::new_err(other.to_string()),
    }
}

/// The exception of `class` (kept in its lock, named as `bullterrier_control.errors` names it) with `message`.
fn package(py: Python<'_>, (class, name): (&'static PyOnceLock<Py<PyType>>, &str), message: String) -> PyErr {
    match class.import(py, "bullterrier_control.errors", name) {
        Ok(class) => PyErr::from_type(class.clone(), message),
        // the package's errors module cannot be imported: that failure is the error to see
        Err(error) => error,
    }
}

/// `bullterrier_control.errors.FormatError` for `error`, the data of `about` (`"Series"`, or empty for no class): its
/// message is `about: path: rule; path: rule` with the first [`SHOWN`] problems (and `... and N more`, N counting the
/// problems the error omits too), its `problems` the (path, rule) pairs the error keeps.
pub(crate) fn format_error(py: Python<'_>, about: &str, error: &FormatError) -> PyErr {
    list(
        py,
        (&FORMAT_ERROR, "FormatError"),
        about,
        &error.problems,
        error.omitted(),
    )
}

/// `bullterrier_control.errors.FormatError` listing the few `problems` found by the bindings themselves.
pub(crate) fn raise(py: Python<'_>, about: &str, problems: &[Problem]) -> PyErr {
    list(py, (&FORMAT_ERROR, "FormatError"), about, problems, 0)
}

/// `bullterrier_control.errors.ParameterError` for `problems` and `omitted` more (a `ParamError`'s, or a few the
/// bindings found), the parameters of `about` (`"lfo"`, a plugin's class name): its message and `problems` as
/// [`format_error`] makes them.
pub(crate) fn parameter_error(py: Python<'_>, about: &str, problems: &[Problem], omitted: usize) -> PyErr {
    list(py, (&PARAMETER_ERROR, "ParameterError"), about, problems, omitted)
}

/// The error of `class` (kept in its lock, named as `bullterrier_control.errors` names it) for `problems` and
/// `omitted` more that are not kept.
fn list(
    py: Python<'_>,
    (class, name): (&'static PyOnceLock<Py<PyType>>, &str),
    about: &str,
    problems: &[Problem],
    omitted: usize,
) -> PyErr {
    let mut message: Vec<String> = problems.iter().take(SHOWN).map(Problem::to_string).collect();
    let left = (problems.len() + omitted).saturating_sub(SHOWN);
    if left > 0 {
        message.push(format!("... and {left} more"));
    }
    let message = message.join("; ");
    let message = if about.is_empty() {
        message
    } else {
        format!("{about}: {message}")
    };
    listing(py, (class, name), message, problems)
}

/// The error of `class` (kept in its lock, named as `bullterrier_control.errors` names it) with `message` and
/// `problems` as its (path, rule) pairs.
fn listing(
    py: Python<'_>,
    (class, name): (&'static PyOnceLock<Py<PyType>>, &str),
    message: String,
    problems: &[Problem],
) -> PyErr {
    let pairs = problems
        .iter()
        .map(|problem| (problem.path.as_str(), problem.rule.as_str()));
    let made = PyTuple::new(py, pairs).and_then(|pairs| {
        class
            .import(py, "bullterrier_control.errors", name)?
            .call1((message, pairs))
    });
    match made {
        Ok(error) => PyErr::from_value(error),
        // the package's errors module cannot be imported: that failure is the error to see
        Err(error) => error,
    }
}
