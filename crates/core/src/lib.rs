//! The numeric core of bullterrier-control: a [`Signal`] (a value readable at any time in seconds, a graph of nodes
//! that combine), the shapers implemented in Rust and the primitives they share, and the plain-data [`formats`] (Unit,
//! Event, Series, Grid, Bundle) that every package of the ecosystem reads and writes.
//! A thing's parameters are declared as a [`params`] table (in Rust by `#[derive(Params)]`), which holds every rule
//! they follow.
//!
//! Pure Rust, no Python: the `bullterrier-control-python` crate binds it to the package. Every built-in source, shaper
//! and operation is defined in Rust ([`builtin`]); its thing gives its table ([`Thing::table`](registry::Thing::table)),
//! and [`shape`] shapes a signal with a shaper the core implements and values it reads through that table, so a value
//! is checked the same way wherever it comes from. A shaper is also a method of [`Signal`] taking its typed parameters
//! ([`Signal::slew`]), and any [`Shaper`] goes into a chain by [`Signal::pipe`]. The [`registry`] holds every source,
//! shaper and operation by name ([`Registry::builtin`]: the built-ins), with the rules for getting in and the
//! one checked door through which each is called.

#![forbid(unsafe_code)]

// the derives write `::bullterrier_control::..` paths, which resolve inside this crate too
extern crate self as bullterrier_control;

pub mod builtin;
pub mod cadence;
pub mod easing;
mod error;
pub mod formats;
pub mod hashing;
pub mod inputs;
pub mod params;
pub mod patch;
mod recurrence;
pub mod registry;
pub mod rounding;
pub mod scaling;
mod signal;

pub use error::Error;
pub use params::{Choice, Params};
pub use patch::Patch;
pub use registry::{Operation, Registry, Shaper, Source};
pub use signal::{FromValues, Interpolation, MAX_DEPTH, Origin, Rendering, Signal, maximum, minimum};

/// The signal the shaper called `name` of [`Registry::builtin`] makes of `input` with `params` (an object of the
/// shaper's parameters by name; one left out takes its default): its [`Thing::call`](registry::Thing::call).
///
/// ```
/// use bullterrier_control::{FromValues, Interpolation, Signal, shape};
/// use serde_json::json;
///
/// let step = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
/// let ramp = shape("slew", &step, &json!({"rise": 0.5, "fall": 0.5, "rate": 10.0}))?;
/// assert_eq!(ramp.sample(&[0.0, 0.1, 0.2])?, [0.0, 0.2, 0.4]);
///
/// let refused = shape("slew", &step, &json!({"rise": "slow"})).map_err(|error| error.to_string());
/// assert_eq!(refused.err().as_deref(), Some(r#"the parameters of "slew" do not fit: rise: must be a number (got "slow")"#));
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
///
/// # Errors
/// As [`Registry::get`] and [`Thing::call`](registry::Thing::call): [`Error::UnknownName`] if the core runs nothing
/// of that name, [`Error::Call`] if it is not a shaper (it takes no signal), [`Error::Params`] listing every rule
/// `params` breaks (a field of another type, out of its range, or one the shaper does not have), and [`Error::Depth`]
/// if the graph would be deeper than [`MAX_DEPTH`].
pub fn shape(name: &str, input: &Signal, params: &serde_json::Value) -> Result<Signal, Error> {
    Registry::builtin().get(name)?.signal(registry::Call {
        signals: vec![input.clone()],
        values: params.clone(),
        ..registry::Call::default()
    })
}

/// The sources the core runs, as functions of their typed parameters (`lib::constant(constant::Params { .. })`), each
/// made through the [`registry`]'s one door (checked, named and recorded as a call by name is); a shaper is a
/// method of [`Signal`] ([`Signal::pipe`]), and every thing the core runs is in [`Registry::builtin`] by name.
pub mod lib {
    include!(concat!(env!("OUT_DIR"), "/lib.rs"));
}

#[cfg(test)]
mod builtin_table;
