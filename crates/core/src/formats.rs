//! The plain-data formats of `docs/formats.md` that the rest of the ecosystem reads and writes: [`Unit`], [`Event`],
//! [`Series`], [`Grid`] and the [`Bundle`] that carries them (with [`bundle_schema`], its JSON Schema).
//!
//! Each is built through a validating constructor that returns every rule it finds broken, not only the first: a
//! [`FormatError`] lists one [`Problem`] per rule, in field order, each with the path of the field (`t`, `values[2]`).
//! Reading JSON goes through the same constructor, so a file and a call are held to one set of rules. A value of a wrong
//! JSON type (`true`, `"1"`) for a number, an unknown field and a whole number beyond 2^53 (within 64 bits; a larger integer is
//! read like a decimal) are problems of their own that stop the reading at the field they name: the rules cannot be checked on a value that was not read.

mod bundle;
mod error;
mod event;
mod field;
mod grid;
mod series;
#[cfg(test)]
mod test_support;
mod unit;

pub use bundle::{Bundle, BundleOptions, bundle_schema, is_name};
pub use error::{FormatError, Problem};
pub use event::{Event, EventOptions};
pub use grid::Grid;
pub use series::{Series, SeriesOptions};
pub use unit::Unit;
