//! Putting a value from 0 to 1 onto a range: the one place a source's `low` and `high` are applied.
//!
//! ```
//! use bullterrier_control::scaling::between;
//!
//! assert_eq!(between(-1.0, 3.0, 0.25), 0.0);
//! assert_eq!(between(3.0, -1.0, 0.25), 2.0); // `low` above `high` inverts it
//! // the plain formula `0.3 + (0.9 - 0.3) * 1.0` is 0.9000000000000001; the range is exact
//! assert_eq!(between(0.3, 0.9, 1.0), 0.9);
//! ```

/// `fraction` (0 to 1) as a value from `low` to `high`: `low + (high - low) * fraction`, kept exactly inside them.
///
/// Promises:
/// - `low` above `high` inverts the fraction: 0 gives `low`, 1 gives `high`.
/// - For a `fraction` in `[0, 1]` the result is within `[min(low, high), max(low, high)]` exactly: the formula can land
///   an ulp beyond `high` (`0.3 + (0.9 - 0.3)` is `0.9000000000000001`), and a consumer relies on the range
///   (`round(127 * value)`). A `fraction` beyond 0 to 1 extrapolates, clipped to the range as well.
/// - A NaN in is a NaN out; nothing panics.
///
/// ```
/// use bullterrier_control::scaling::between;
///
/// assert_eq!(between(2.0, 2.0, 0.3), 2.0); // `low` equal to `high` is constant
/// assert_eq!(between(0.9, 0.3, 0.0), 0.9);
/// ```
#[must_use]
pub fn between(low: f64, high: f64, fraction: f64) -> f64 {
    let (lowest, highest) = if low <= high { (low, high) } else { (high, low) };
    let value = low + (high - low) * fraction;
    if value < lowest {
        lowest
    } else if value > highest {
        highest
    } else {
        value
    }
}
