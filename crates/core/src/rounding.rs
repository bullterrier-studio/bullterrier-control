//! Deciding at a boundary when times are decimals: a time that is a bound to within float rounding is the bound.
//!
//! ```
//! use bullterrier_control::rounding::{at_least, more_than};
//!
//! assert!(at_least(0.3 - 0.1, 0.2)); // 0.3 - 0.1 is 0.19999999999999998
//! assert!(!more_than(0.1 + 0.2 - 0.3, 0.0)); // that is 5.5e-17, a rounding error, not a time after 0
//! ```
//!
//! Promises:
//! - Times and durations are in seconds. A value within [`TOLERANCE`] (a nanosecond) of a bound counts as the bound;
//!   anything farther is on its side of it.
//! - A nanosecond is far above the error of adding and subtracting times under a day (the spacing of floats near 1e5 s
//!   is 1.5e-11 s) and far below anything a person or a beat tracker means by a different time, so a bound a decimal
//!   lands on is held, and one a millionth of a second off is not.
//! - Time at the edge of the floats is not covered: beyond about 1e7 s the spacing of floats exceeds the tolerance, and a
//!   comparison is as exact as the numbers are.
//! - Not for positions in samples: a `Rendered` signal has a tolerance in shares of a sample for those.

/// Seconds within which a time is the bound it is compared with: a nanosecond; see the module's promises.
pub const TOLERANCE: f64 = 1e-9;

/// Whether `value` reaches `bound`, or is short of it by no more than [`TOLERANCE`].
///
/// ```
/// use bullterrier_control::rounding::at_least;
///
/// assert!(at_least(-2.2e-16, 0.0)); // a rounding error under zero is at least zero
/// assert!(!at_least(-1e-6, 0.0)); // a millionth under is not
/// ```
#[must_use]
pub fn at_least(value: f64, bound: f64) -> bool {
    value >= bound - TOLERANCE
}

/// Whether `value` is beyond `bound` by more than [`TOLERANCE`].
///
/// ```
/// use bullterrier_control::rounding::more_than;
///
/// assert!(more_than(1e-6, 0.0)); // a millionth over zero is
/// assert!(!more_than(0.0, 0.0));
/// ```
#[must_use]
pub fn more_than(value: f64, bound: f64) -> bool {
    value > bound + TOLERANCE
}
