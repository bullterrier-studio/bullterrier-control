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

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "the range is promised exactly, not approximately")]
mod tests {
    use proptest::prelude::*;

    use super::between;

    #[test]
    fn a_fraction_of_zero_to_one_becomes_a_value_between_low_and_high() {
        for (low, high, fraction, expected, rule) in [
            (-1.0, 3.0, 0.0, -1.0, "from -1 to 3: the bottom"),
            (-1.0, 3.0, 0.25, 0.0, "from -1 to 3: low + 4 * 0.25"),
            (-1.0, 3.0, 0.5, 1.0, "from -1 to 3: low + 4 * 0.5"),
            (-1.0, 3.0, 1.0, 3.0, "from -1 to 3: the top"),
            (3.0, -1.0, 0.25, 2.0, "low above high runs downward: 3 - 4 * 0.25"),
            (3.0, -1.0, 0.5, 1.0, "low above high: the middle"),
            (3.0, -1.0, 1.0, -1.0, "low above high: the end is high"),
            (2.0, 2.0, 0.3, 2.0, "low equal to high is constant"),
            (
                0.3,
                0.9,
                1.0,
                0.9,
                "the top is exactly high where 0.3 + 0.6 * 1 is 0.9000000000000001",
            ),
            (
                0.9,
                0.3,
                1.0,
                0.3,
                "inverted, the end is exactly high, where the formula is an ulp below",
            ),
            (0.9, 0.3, 0.0, 0.9, "inverted, the start is exactly low"),
            (-1.0, 3.0, 2.0, 3.0, "a fraction past 1 is clipped to high"),
            (-1.0, 3.0, -1.0, -1.0, "a fraction below 0 is clipped to low"),
        ] {
            assert_eq!(between(low, high, fraction), expected, "{rule}");
        }
    }

    #[test]
    fn a_nan_stays_a_nan_and_never_panics() {
        for (low, high, fraction) in [(0.0, 1.0, f64::NAN), (f64::NAN, 1.0, 0.5), (0.0, f64::NAN, 0.5)] {
            assert!(between(low, high, fraction).is_nan(), "{low} {high} {fraction}");
        }
    }

    proptest! {
        #[test]
        fn the_result_is_exactly_inside_the_range(
            low in -1.0e6_f64..=1.0e6,
            high in -1.0e6_f64..=1.0e6,
            fraction in 0.0_f64..=1.0,
        ) {
            let value = between(low, high, fraction);
            prop_assert!(low.min(high) <= value && value <= low.max(high), "{} not in {}..{}", value, low, high);
        }

        #[test]
        fn the_ends_of_the_fraction_are_the_ends_of_the_range(low in -1.0e6_f64..=1.0e6, high in -1.0e6_f64..=1.0e6) {
            prop_assert_eq!(between(low, high, 0.0), low);
            prop_assert!((between(low, high, 1.0) - high).abs() <= 1e-9 * (1.0 + high.abs()));
        }
    }
}
