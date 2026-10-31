//! The core's `FormatError` raised as the package's `bullterrier_control.errors.FormatError`.

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

/// `bullterrier_control.errors.FormatError` for `error`, the data of `about` (`"Series"`, or empty for no class): its
/// message is `about: path: rule; path: rule` with the first [`SHOWN`] problems (and `... and N more`, N counting the
/// problems the error omits too), its `problems` the (path, rule) pairs the error keeps.
pub(crate) fn format_error(py: Python<'_>, about: &str, error: &FormatError) -> PyErr {
    list(py, about, &error.problems, error.omitted())
}

/// `bullterrier_control.errors.FormatError` listing the few `problems` found by the bindings themselves.
pub(crate) fn raise(py: Python<'_>, about: &str, problems: &[Problem]) -> PyErr {
    list(py, about, problems, 0)
}

/// The error for `problems` and `omitted` more that are not kept.
fn list(py: Python<'_>, about: &str, problems: &[Problem], omitted: usize) -> PyErr {
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
        FORMAT_ERROR
            .import(py, "bullterrier_control.errors", "FormatError")?
            .call1((message, pairs))
    });
    match made {
        Ok(error) => PyErr::from_value(error),
        // the package's errors module cannot be imported: that failure is the error to see
        Err(error) => error,
    }
}
