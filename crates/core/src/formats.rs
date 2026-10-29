//! The plain-data formats of `docs/formats.md` that the rest of the ecosystem reads and writes: [`Unit`], [`Event`]
//! and [`Series`].
//!
//! Each is built through a validating constructor that returns every rule it finds broken, not only the first: a
//! [`FormatError`] lists one [`Problem`] per rule, in field order, each with the path of the field (`t`, `values[2]`).
//! Reading JSON goes through the same constructor, so a file and a call are held to one set of rules. A value of a wrong
//! JSON type (`true`, `"1"`) for a number, an unknown field and an integer beyond 2^53 are problems of their own that
//! stop the reading at the field they name: the rules cannot be checked on a value that was not read.

mod error;
mod event;
mod field;
mod series;
#[cfg(test)]
mod test_support;
mod unit;

pub use error::{FormatError, Problem};
pub use event::{Event, EventOptions};
pub use series::{Series, SeriesOptions};
pub use unit::Unit;
