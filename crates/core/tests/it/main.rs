//! The integration tests of the core: one binary, a module per area, through the public API only.
//! A test stays next to its code only when no public route reaches what it checks (a private kernel, a `pub(crate)`
//! item); an item is never made `pub` to move a test out. Run one area: `cargo test -p bullterrier-control --test it signal::`.

mod common;

mod architecture;
mod builtin;
mod builtin_beats;
mod builtin_clip;
mod builtin_constant;
mod builtin_delay;
mod builtin_envelope;
mod builtin_every;
mod builtin_hold;
mod builtin_keyframes;
mod builtin_latch;
mod builtin_lfo;
mod builtin_noise;
mod builtin_normalise;
mod builtin_quantize;
mod builtin_remap;
mod builtin_repeat;
mod builtin_select;
mod builtin_shift;
mod builtin_steps;
mod builtin_thin;
mod builtin_window;
mod cadence;
mod formats_unit;
mod hashing;
mod inputs;
mod params;
mod params_unit;
mod patch;
mod patch_save;
mod registry;
mod rounding;
mod scaling;
mod shape;
mod signal;
mod signal_rendered;
