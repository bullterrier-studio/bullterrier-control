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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{at_least, more_than};

    #[test]
    fn at_least_holds_a_value_that_is_the_bound_to_within_float_rounding() {
        for (value, bound, expected, rule) in [
            (0.3 - 0.1, 0.2, true, "0.3 - 0.1 is 0.19999999999999998: at least 0.2"),
            (0.2, 0.2, true, "equal"),
            (0.25, 0.2, true, "above"),
            (0.199_999, 0.2, false, "a millionth under"),
            (-2.2e-16, 0.0, true, "a rounding error under zero is at least zero"),
            (-1e-6, 0.0, false, "a millionth under zero is not"),
            (-1e-9, 0.0, true, "exactly a nanosecond under is still the bound"),
            (-1.1e-9, 0.0, false, "a nanosecond and a tenth under is not"),
        ] {
            assert_eq!(at_least(value, bound), expected, "{rule}");
        }
    }

    #[test]
    fn more_than_needs_a_value_beyond_the_bound_by_more_than_float_rounding() {
        for (value, bound, expected, rule) in [
            (0.1 + 0.2 - 0.3, 0.0, false, "5.5e-17 is not more than zero"),
            (0.0, 0.0, false, "equal"),
            (1e-6, 0.0, true, "a millionth over zero is"),
            (-1e-6, 0.0, false, "below"),
            (0.5, 0.2, true, "well over"),
            (1e-9, 0.0, false, "exactly a nanosecond over is still the bound"),
            (1.1e-9, 0.0, true, "a nanosecond and a tenth over is more"),
            (0.5e-9, 0.0, false, "half a nanosecond over is not (and not under)"),
        ] {
            assert_eq!(more_than(value, bound), expected, "{rule}");
        }
    }

    proptest! {
        // the two never both hold the same side of a bound: a value more than the bound is at least it, and a value
        // below the bound by more than the tolerance is neither
        #[test]
        fn more_than_implies_at_least_and_a_value_far_below_is_neither(bound in -1.0e5_f64..=1.0e5, offset in 1.0e-6_f64..=1.0) {
            prop_assert!(more_than(bound + offset, bound) && at_least(bound + offset, bound));
            prop_assert!(!more_than(bound - offset, bound) && !at_least(bound - offset, bound));
        }

        #[test]
        fn a_bound_the_decimals_land_next_to_is_held(whole in 0_u32..=86_400, tenths in 0_u32..10) {
            // times under a day built by adding and subtracting decimals are the bound they are meant to be
            let bound = f64::from(whole) + f64::from(tenths) / 10.0;
            let rebuilt = (bound + 0.1) - 0.1;
            prop_assert!(at_least(rebuilt, bound) && !more_than(rebuilt, bound), "{} vs {}", rebuilt, bound);
        }
    }
}
