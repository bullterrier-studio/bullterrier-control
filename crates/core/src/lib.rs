//! The numeric core of bullterrier-control: the shapers implemented in Rust and the primitives they share.
//!
//! Pure Rust, no Python: the `bullterrier-control-python` crate binds it to the package. A shaper's parameters arrive
//! already validated by the Python package, which declares them; the core trusts them and does not check their ranges
//! again.

#![forbid(unsafe_code)]

/// The names of the shapers the core implements, sorted.
#[must_use]
pub fn shapers() -> &'static [&'static str] {
    &[]
}
