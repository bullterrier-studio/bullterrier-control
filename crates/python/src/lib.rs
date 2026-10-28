//! `bullterrier_control._native._core`: the shapers of the Rust core, for the Python package. Generic: it names no
//! shaper, so porting one never changes it.

use pyo3::prelude::*;

/// The shapers of bullterrier-control's Rust core.
#[pymodule]
mod _core {
    #[pymodule_export]
    use super::shapers;
}

/// The names of the shapers the core implements, sorted.
#[pyfunction]
fn shapers() -> Vec<&'static str> {
    bullterrier_control::shapers().to_vec()
}
