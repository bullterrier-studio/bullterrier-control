//! The core's `FormatError` raised as the package's `bullterrier_control.errors.FormatError`.

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyTuple, PyType};

use bullterrier_control::formats::{FormatError, Problem};

/// How many problems a message lists; `problems` holds every one.
const SHOWN: usize = 20;

/// The package's `FormatError`, imported when the first one is raised (so importing the extension imports nothing of
/// the package) and kept.
static FORMAT_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// `bullterrier_control.errors.FormatError` for `error`, the data of `about` (`"Series"`, or empty for no class).
pub(crate) fn format_error(py: Python<'_>, about: &str, error: &FormatError) -> PyErr {
    raise(py, about, &error.problems)
}

/// `bullterrier_control.errors.FormatError` listing `problems`: its message is `about: path: rule; path: rule` with the
/// first [`SHOWN`] problems (and `... and N more`), its `problems` every (path, rule) pair.
pub(crate) fn raise(py: Python<'_>, about: &str, problems: &[Problem]) -> PyErr {
    let mut message: Vec<String> = problems.iter().take(SHOWN).map(Problem::to_string).collect();
    if problems.len() > SHOWN {
        message.push(format!("... and {} more", problems.len() - SHOWN));
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
