//! The numeric core of bullterrier-control: control signals, numbers that change over time and drive the parameters of
//! anything. A [`Signal`] is a value readable at any time in seconds; sources make one, shapers turn one into another,
//! operations turn data into data, and a graph of them is saved as a [`Patch`]. Pure Rust, no Python, no `unsafe`, the
//! same result on every target (it builds for `wasm32-unknown-unknown`).
//!
//! ```
//! use bullterrier_control::builtin::{clip, slew};
//! use bullterrier_control::{FromValues, Interpolation, Signal};
//!
//! // samples at 0, 0.1, 0.2 and 0.3 s, read between them by straight lines
//! let energy = Signal::from_values(&[0.0, 1.0, 0.2, 0.0], 10.0, FromValues::default(), Interpolation::Linear)?;
//! let glow = energy
//!     .slew(slew::Params { rise: 0.2, fall: 0.5, ..Default::default() })?
//!     .clip(clip::Params { low: 0.1, high: 0.8, ..Default::default() })?;
//! // climbs 5 a second from the floor of 0.1
//! let values = glow.sample(&[0.0, 0.05, 0.1])?;
//! assert!(values.iter().zip([0.1, 0.25, 0.5]).all(|(value, expected)| (value - expected).abs() < 1e-9));
//! # Ok::<(), bullterrier_control::Error>(())
//! ```
//!
//! - [`formats`]: the plain data every package of the ecosystem reads and writes: `Series`, `Event`, `Grid`, `Bundle`.
//! - [`Signal`]: the graph, `sample` it at any times; `+ - *`, [`maximum`] and [`minimum`] combine signals.
//! - [`lib`]: every source as a function of its typed parameters; a shaper is a method of [`Signal`]
//!   ([`Signal::slew`], any [`Shaper`] by [`Signal::pipe`]).
//! - [`builtin`]: the 21 built-ins, each with its `Params`; [`params`]: how parameters are declared and checked.
//! - [`registry`]: every source, shaper and operation by name, the one checked door each is called through.
//! - [`inputs`]: the data a thing takes besides parameters (keyframes, events, a grid, values, a series).
//! - [`patch`]: a graph as data (TOML), read, built and saved.
//! - [`easing`], [`hashing`], [`rounding`], [`scaling`], [`cadence`]: the primitives the built-ins share.

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
///
/// ```
/// use bullterrier_control::builtin::{envelope, lfo, noise};
/// use bullterrier_control::formats::{Event, EventOptions, Grid};
/// use bullterrier_control::inputs::Keyframe;
/// use bullterrier_control::easing::Ease;
/// use bullterrier_control::cadence::Per;
/// use bullterrier_control::lib;
///
/// // an envelope on each kick: 1 at a kick, e^-1 a quarter second later
/// let kicks = [0.5, 1.0, 1.5]
///     .map(|t| Event::new(t, EventOptions { label: Some("kick".into()), ..Default::default() }))
///     .into_iter()
///     .collect::<Result<Vec<_>, _>>()?;
/// let flare = lib::envelope(&kicks, envelope::Params { decay: 0.25, ..Default::default() })?;
/// let flare = flare.sample(&[0.5, 0.75, 1.0])?;
/// assert_eq!(flare[0], 1.0);
/// assert!((flare[1] - (-1.0_f64).exp()).abs() < 1e-12);
/// assert_eq!(flare[2], 1.0);
///
/// // keyframes eased between: 0.9 * (3 * 0.2^2 - 2 * 0.2^3) = 0.0936 at 1 s, then held after 5 s
/// let ramp = lib::keyframes(&[Keyframe::at(0.0, 0.0).ease(Ease::InOut), Keyframe::at(5.0, 0.9)])?;
/// let ramp = ramp.sample(&[1.0, 2.5, 7.0])?;
/// assert!((ramp[0] - 0.0936).abs() < 1e-12 && (ramp[1] - 0.45).abs() < 1e-12 && ramp[2] == 0.9);
///
/// // an lfo per beat of a grid with a tempo change: the top of a sine at beat 0.25, the bottom at beat 0.75
/// let grid = Grid::new(vec![0.0, 0.5, 1.0, 1.4, 1.8], 4, 0)?;
/// let wobble = lib::lfo(lfo::Params { rate: 1.0, per: Per::Beat, ..Default::default() }, Some(&grid))?;
/// let wobble = wobble.sample(&[0.125, 0.375])?;
/// assert!((wobble[0] - 1.0).abs() < 1e-12 && wobble[1].abs() < 1e-12);
///
/// // seeded noise: the same seed gives the same values, in any reading order
/// let params = noise::Params { rate: 4.0, seed: 7, shape: noise::Shape::Smooth, ..Default::default() };
/// let drift = lib::noise(params.clone())?;
/// assert_eq!(drift.sample(&[2.0, 1.0])?, lib::noise(params)?.sample(&[2.0, 1.0])?);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub mod lib {
    include!(concat!(env!("OUT_DIR"), "/lib.rs"));
}

#[cfg(test)]
mod builtin_table;
