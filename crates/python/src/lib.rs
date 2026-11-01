//! `bullterrier_control._native._core`: gives the Rust core's signals (`SignalBase`, the class the package's `Signal`
//! inherits), its primitives (easing, hashing, scaling, cadence,
//! rounding) and its formats (`Unit`, `Event`, `Series`, `Grid`, `Bundle`) to Python as the package's classes.
//! Generic: it names no shaper, so porting one never changes it.
//!
//! It also gives the core's parameter tables (`ParamTable`, a built-in's through `ParamTable.builtin`)
//! and its inputs (`Keyframe`, `check_input`, `input_docs`). A format that breaks a rule raises the package's
//! `bullterrier_control.errors.FormatError`, parameters or an input that do `ParameterError`, each imported when the
//! first one is raised: importing the extension imports nothing of the package.

mod callback;
mod data;
mod error;
mod formats;
mod inputs;
mod params;
mod patch;
mod primitives;
mod registry;
mod signal;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// bullterrier-control's Rust core: its signals, shapers, primitives and formats.
#[pymodule]
mod _core {
    use pyo3::prelude::*;

    #[pymodule_export]
    use super::formats::{PyBundle, PyEvent, PyGrid, PySeries, PyUnit, bundle_schema, dumps, is_bundle_name};
    #[pymodule_export]
    use super::inputs::{PyKeyframe, check_input, input_docs};
    #[pymodule_export]
    use super::params::PyParamTable;
    #[pymodule_export]
    use super::patch::{PyNode, PyPatch, PyReference, patch_schema};
    #[pymodule_export]
    use super::primitives::{Counter, PyEase, at_least, between, counter, ease, more_than, uniform};
    #[pymodule_export]
    use super::registry::{PyRegistry, PyThing};
    #[pymodule_export]
    use super::signal::{Constant, Rendered, SignalBase, from_series, from_values, make_signals_as, maximum, minimum};

    /// Every unit of the core becomes a member of `Unit`; the primitives' constants are added.
    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        super::formats::add_units(module.py())?;
        super::primitives::add_constants(module)
    }
}

/// A `ValueError` carrying `error`'s message.
#[expect(clippy::needless_pass_by_value, reason = "map_err passes the error by value")]
pub(crate) fn value_error(error: impl ToString) -> PyErr {
    PyValueError::new_err(error.to_string())
}
