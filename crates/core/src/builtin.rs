//! The built-in shapers, one module per file in `src/builtin/`, and their table; the build script writes both.
//!
//! A built-in file has two items: `Params`, the shaper's parameters (`#[derive(Deserialize)]`,
//! `#[serde(deny_unknown_fields)]`, no defaults, the field names of the Python model, so a struct that drifts from the
//! model fails its first call naming the field), and `fn run(inputs: &[f64], params: &Params) -> Vec<f64>`, the causal
//! recurrence over the input sampled at the shaper's rate. Built-ins never refer to one another: what two of them share
//! goes in a module of the crate named for its concept.

use serde::Deserialize as _;

use crate::Error;

/// The function that runs one shaper: reads its parameters, then runs it over the inputs.
pub(crate) type Run = fn(&[f64], &serde_json::Value) -> Result<Vec<f64>, Error>;

include!(concat!(env!("OUT_DIR"), "/builtin.rs"));
