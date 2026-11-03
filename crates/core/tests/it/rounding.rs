//! The tolerance of a decision at a time boundary, through its public functions.

use proptest::prelude::*;

use bullterrier_control::rounding::{at_least, more_than};

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
