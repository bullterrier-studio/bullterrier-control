//! The core's `FormatError` and `ParamError` raised as the package's `bullterrier_control.errors.FormatError` and
//! `ParameterError`.

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyTuple, PyType};

use bullterrier_control::formats::{FormatError, Problem};

/// How many problems a message lists; `problems` holds up to the first 100 (the core's cap), and the message says how
/// many more there are.
const SHOWN: usize = 20;

/// The package's `FormatError`, imported when the first one is raised (so importing the extension imports nothing of
/// the package) and kept.
static FORMAT_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// The package's `ParameterError`, imported and kept as [`FORMAT_ERROR`] is.
static PARAMETER_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

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
