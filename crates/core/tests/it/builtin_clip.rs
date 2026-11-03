//! The built-in `clip` through its public face.

use proptest::prelude::*;
use serde_json::json;

use crate::common::problems::check_problems;
use bullterrier_control::Params as _;
use bullterrier_control::builtin::clip::*;
use bullterrier_control::{Error, Signal};
use bullterrier_control::{FromValues, Interpolation};

fn check(id: &str, given: &serde_json::Value, expected: &'static [(&'static str, &'static str)]) {
    match Params::parse(given) {
        Ok(params) => assert!(expected.is_empty(), "{id}: accepted {params:?}, wanted {expected:?}"),
        Err(error) => check_problems(id, &error.problems, expected),
    }
}

#[test]
fn the_high_end_must_not_be_below_the_low_end() {
    for (id, given, expected) in [
        ("the defaults", json!({}), &[][..]),
        ("high above low", json!({"low": 0.25, "high": 0.75}), &[]),
        ("high equal to low", json!({"low": 0.5, "high": 0.5}), &[]),
        (
            "high below low",
            json!({"low": 0.5, "high": 0.25}),
            &[("high", "must not be below low = 0.5 (got 0.25)")],
        ),
        (
            "high far below low",
            json!({"low": 1000, "high": -3}),
            &[("high", "low = 1000.0 (got -3.0)")],
        ),
    ] {
        check(id, &given, expected);
    }
}

#[test]
fn a_typed_struct_is_checked_by_the_rule_between_its_fields_too() {
    let refused = Params { low: 0.5, high: 0.25 }.check();
    assert!(refused.is_err(), "accepted {refused:?}");
    if let Err(error) = refused {
        check_problems("typed", &error.problems, &[("high", "must not be below low = 0.5")]);
    }
}

fn params(low: f64, high: f64) -> Params {
    Params { low, high }
}

/// `values` one a second, held (so each reads as itself in the middle of its second), clipped, read at `times`.
fn clipped(values: &[f64], range: Params, times: &[f64]) -> Result<Vec<f64>, Error> {
    Signal::from_values(values, 1.0, FromValues::default(), Interpolation::Hold)?
        .clip(range)?
        .sample(times)
}

#[test]
fn values_below_low_and_above_high_are_pulled_in_and_the_rest_stay_even_on_the_bounds() -> Result<(), Error> {
    // (low, high, input) -> expected, each worked by hand
    let rows = [
        (0.0, 1.0, vec![-2.0, 0.25, 0.5, 3.0], vec![0.0, 0.25, 0.5, 1.0]),
        (0.0, 1.0, vec![0.0, 1.0, -0.5, 1.5], vec![0.0, 1.0, 0.0, 1.0]), // the bounds themselves stay
        (2.0, 2.0, vec![0.0, 2.0, 5.0], vec![2.0, 2.0, 2.0]),            // low equal to high: one value
        (-3.0, -1.0, vec![-4.0, -2.0, 0.0], vec![-3.0, -2.0, -1.0]),     // a range below 0
        (-1e6, 1e6, vec![-1e9, 1e6, 1e300], vec![-1e6, 1e6, 1e6]),       // the ends of the parameter range
    ];
    for (low, high, input, expected) in rows {
        let times: Vec<f64> = (0..input.len())
            .map(|index| f64::from(u32::try_from(index).unwrap_or(0)))
            .collect();
        assert_eq!(
            clipped(&input, params(low, high), &times)?,
            expected,
            "{low}..{high} over {input:?}"
        );
    }
    Ok(())
}

#[test]
fn between_samples_the_clip_applies_to_the_signal_not_to_the_samples() -> Result<(), Error> {
    // a line from -1 to 3 over a second crosses 0 at 0.25 s and 1 at 0.5 s
    let line = Signal::from_values(&[-1.0, 3.0], 1.0, FromValues::default(), Interpolation::Linear)?;
    let out = line.clip(params(0.0, 1.0))?.sample(&[0.125, 0.25, 0.375, 0.5, 0.75])?;
    assert_eq!(out, [0.0, 0.0, 0.5, 1.0, 1.0]);
    Ok(())
}

#[test]
fn the_defaults_clip_to_0_and_1() -> Result<(), Error> {
    let kept =
        Signal::from_values(&[-1.0, 2.0], 1.0, FromValues::default(), Interpolation::Hold)?.pipe(Params::default())?;
    assert_eq!(kept.sample(&[0.0, 1.0])?, [0.0, 1.0]);
    Ok(())
}

#[test]
fn a_high_below_low_is_refused_naming_high() -> Result<(), Error> {
    let signal = Signal::constant(0.5)?;
    let refused = signal.clip(params(1.0, 0.5));
    assert!(
        matches!(&refused, Err(Error::Params { shaper, problems, .. })
            if shaper == "clip" && problems.len() == 1 && problems[0].path == "high"),
        "{refused:?}"
    );
    Ok(())
}

proptest! {
    #[test]
    fn the_output_never_leaves_the_range_and_leaves_what_is_inside_exactly_as_it_was(
        values in prop::collection::vec(-50.0_f64..=50.0, 1..10),
        low in -20.0_f64..=0.0,
        high in 0.0_f64..=20.0,
        times in prop::collection::vec(0.0_f64..=12.0, 1..15),
    ) {
        let input = Signal::from_values(&values, 1.0, FromValues::default(), Interpolation::Linear)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let out = input.clip(params(low, high)).and_then(|kept| kept.sample(&times))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let before = input.sample(&times).map_err(|error| TestCaseError::fail(error.to_string()))?;
        for (kept, original) in out.iter().zip(&before) {
            prop_assert!(*kept >= low && *kept <= high, "{kept} outside {low}..{high}");
            if (low..=high).contains(original) {
                prop_assert!((kept - original).abs() <= 0.0, "{original} inside the range became {kept}");
            }
        }
    }
}
