//! The built-in `normalise` through its public face.

use serde_json::json;

use crate::common::problems::check_problems;
use bullterrier_control::Params as _;
use bullterrier_control::builtin::normalise::Params;
use bullterrier_control::formats::{Series, SeriesOptions, Unit};
use bullterrier_control::inputs::{Input, InputKind};
use bullterrier_control::registry::{Call, Kind, Output, Returns};
use bullterrier_control::{Error, Operation, Registry};

const LARGEST: f64 = f64::MAX;

/// `values` as a `ratio` series ten times a second from 0.
fn ratio(values: &[f64]) -> Result<Series, Error> {
    Series::new(10.0, values.to_vec(), Unit::Ratio, SeriesOptions::default()).map_err(|source| Error::Format { source })
}

fn series_of(output: Output) -> Result<Series, Error> {
    match output {
        Output::Series(series) => Ok(series),
        other => Err(Error::Call {
            message: format!("normalise gave {other:?}"),
        }),
    }
}

/// `series` normalised by the thing named `normalise`, through the registry: its input checked, its values read.
fn by_name(values: serde_json::Value, series: Series) -> Result<Series, Error> {
    let call = Call {
        inputs: [("series".to_owned(), Input::Series(series))].into(),
        values,
        ..Call::default()
    };
    series_of(Registry::builtin().get("normalise")?.call(&call)?)
}

/// `series` normalised by the body alone, with the typed parameters.
fn by_body(lower: f64, upper: f64, series: Series) -> Result<Series, Error> {
    series_of(Params { lower, upper }.run(series)?)
}

/// Both doors' values for `given` and the percentiles, which must agree to the bit.
fn normalised(given: &[f64], lower: f64, upper: f64) -> Result<Vec<f64>, Error> {
    let named = by_name(json!({"lower": lower, "upper": upper}), ratio(given)?)?;
    let body = by_body(lower, upper, ratio(given)?)?;
    let bits = |series: &Series| series.values().iter().map(|value| value.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&named), bits(&body), "{given:?} {lower} {upper}");
    Ok(body.values().to_vec())
}

/// `(id, values, lower, upper, outputs)`, each worked by hand: the q-th percentile of n values is read at index
/// q * (n - 1) of the sorted values, between the two around it in proportion; a value v becomes
/// (v - low) / (high - low), clipped to 0..1. Equal to 1e-12.
#[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
const BY_HAND: [(&str, &[f64], f64, f64, &[f64]); 11] = [
    // sorted 1 1 3 4 5: q 0.25 is index 1 (1); 0.9 index 3.6, 4 + 0.6 * (5 - 4) = 4.6; (v - 1) / 3.6
    (
        "five values, the 25th and 90th",
        &[3.0, 1.0, 4.0, 1.0, 5.0],
        0.25,
        0.9,
        &[2.0 / 3.6, 0.0, 3.0 / 3.6, 0.0, 1.0],
    ),
    // the median is index 2 (3): (v - 3) / 1.6
    (
        "five values, the median and the 90th",
        &[3.0, 1.0, 4.0, 1.0, 5.0],
        0.5,
        0.9,
        &[0.0, 0.0, 1.0 / 1.6, 0.0, 1.0],
    ),
    (
        "five values, the 25th and the median",
        &[3.0, 1.0, 4.0, 1.0, 5.0],
        0.25,
        0.5,
        &[1.0, 0.0, 1.0, 0.0, 1.0],
    ),
    // 11 values 0..10: the 10th percentile is at index 1 and the 90th at index 9: (v - 1) / 8
    (
        "eleven, the 10th and 90th",
        &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        0.1,
        0.9,
        &[0.0, 0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0, 1.0],
    ),
    // index 3.5 is halfway between 30 and 40: 35; index 1 is 10: (v - 10) / 25
    (
        "the 87.5th halfway between two",
        &[0.0, 10.0, 20.0, 30.0, 40.0],
        0.25,
        0.875,
        &[0.0, 0.0, 0.4, 0.8, 1.0],
    ),
    (
        "the 12.5th halfway between two",
        &[0.0, 10.0, 20.0, 30.0, 40.0],
        0.125,
        0.75,
        &[0.0, 0.2, 0.6, 1.0, 1.0],
    ),
    (
        "the order of the values only says where each lands",
        &[40.0, 0.0, 30.0, 10.0, 20.0],
        0.25,
        0.875,
        &[1.0, 0.0, 0.8, 0.0, 0.4],
    ),
    // the 98th is index 9.8, 5 + 0.8 * 5 = 9; the 5th is 0
    (
        "the defaults among zeros",
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0, 10.0],
        0.05,
        0.98,
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0 / 9.0, 1.0],
    ),
    (
        "the 0th and 100th are the lowest and highest",
        &[-4.0, -2.0, 0.0, 2.0, 4.0],
        0.0,
        1.0,
        &[0.0, 0.25, 0.5, 0.75, 1.0],
    ),
    ("two values, the ends", &[1.0, 3.0], 0.0, 1.0, &[0.0, 1.0]),
    // index 0.1 is 1 + 0.1 * 2 = 1.2; index 0.9 is 2.8: (v - 1.2) / 1.6
    ("two values, inside", &[1.0, 3.0], 0.1, 0.9, &[0.0, 1.0]),
];

#[test]
fn normalise_puts_the_percentiles_at_0_and_1_and_clips_what_lies_beyond() -> Result<(), Error> {
    for (id, given, lower, upper, expected) in BY_HAND {
        let got = normalised(given, lower, upper)?;
        assert_eq!(got.len(), expected.len(), "{id}");
        assert!(
            got.iter().zip(expected).all(|(got, want)| (got - want).abs() <= 1e-12),
            "{id}: {got:?}"
        );
    }
    Ok(())
}

#[test]
fn by_the_defaults_normalise_reads_the_5th_and_98th_percentiles() -> Result<(), Error> {
    // 101 values 0..100: the 5th percentile is value 5 and the 98th value 98: (v - 5) / 93
    let ramp: Vec<f64> = (0..=100).map(f64::from).collect();
    let out = by_name(json!({}), ratio(&ramp)?)?;
    let picked: Vec<f64> = [0, 5, 6, 50, 98, 100]
        .iter()
        .map(|index| out.values()[*index])
        .collect();
    let expected = [0.0, 0.0, 1.0 / 93.0, 45.0 / 93.0, 1.0, 1.0];
    assert!(
        picked
            .iter()
            .zip(expected)
            .all(|(got, want)| (got - want).abs() <= 1e-12),
        "{picked:?}"
    );
    assert_eq!(out.scaling(), Some("percentile 5 to 98 of the series, clipped to 0..1"));
    Ok(())
}

/// `(id, values, lower, upper, outputs)` to the bit, the outputs as numpy 2.5.4 gives them through the Python
/// body this replaced (the sign of each zero too), but for the row that says otherwise.
#[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
const EXACT: [(&str, &[f64], f64, f64, &[f64]); 17] = [
    // index 1.6 between 0.3 and 1 (scaled by 1/2): from 1 back, 1 - 0.7 * 0.4, not 0.3 + 0.7 * 0.6
    (
        "a weight of a half or more is read back from the upper value",
        &[0.0, 0.3, 1.0],
        0.0,
        0.8,
        &[0.0, 0.416_666_666_666_666_63, 1.0],
    ),
    // the 0th percentile is -0.0 + 0 * 0.0 = 0.0, and -0.0 - 0.0 is -0.0
    (
        "-0.0 below a lower percentile of 0.0 stays -0.0",
        &[-0.0, -0.0, 1.0],
        0.0,
        1.0,
        &[-0.0, -0.0, 1.0],
    ),
    // a weight of -0.0 keeps the -0.0, and -0.0 - -0.0 is 0.0
    (
        "-0.0 below a lower percentile of -0.0 is 0.0",
        &[-0.0, -0.0, 1.0],
        -0.0,
        1.0,
        &[0.0, 0.0, 1.0],
    ),
    // index 0.4: -0.0 + 0 * 0.4 = 0.0
    (
        "a weight below a half between two -0.0 gives 0.0",
        &[-0.0, -0.0, 1.0],
        0.2,
        1.0,
        &[-0.0, -0.0, 1.0],
    ),
    // index 0.5: -0.0 - 0 * 0.5 = -0.0
    (
        "a weight of a half between two -0.0 gives -0.0",
        &[-0.0, -0.0, 1.0],
        0.25,
        1.0,
        &[0.0, 0.0, 1.0],
    ),
    // 0.0 and -0.0 are one value to the sort, so the one given first is the 0th percentile: 0.0 + -0.0 * -0.0 is 0.0
    (
        "0.0 given before -0.0 is the lowest",
        &[0.0, -0.0, 1.0],
        -0.0,
        1.0,
        &[0.0, -0.0, 1.0],
    ),
    // index 0.8 between the first two zeros, read back from the second: 0.0 - 0 * 0.2 = 0.0
    (
        "the zeros stay in the order given",
        &[-0.0, 0.0, -0.0, 0.0, 2.0],
        0.2,
        1.0,
        &[-0.0, 0.0, -0.0, 0.0, 1.0],
    ),
    // index 1.6, between the 0.0 and the -0.0 given after it, read back from the -0.0: -0.0 - -0.0 * 0.4 is 0.0,
    // and -0.0 - 0.0 is -0.0
    (
        "numpy's order of equal zeros is not reproduced: only the sign of a zero differs from the old body (all 0.0)",
        &[0.0, -0.0, -0.0, -1.0, 1.0],
        0.4,
        0.9,
        &[0.0, -0.0, -0.0, 0.0, 1.0],
    ),
    ("a flat series is all 0", &[3.0, 3.0, 3.0], 0.05, 0.98, &[0.0, 0.0, 0.0]),
    ("one value is 0", &[3.0], 0.05, 0.98, &[0.0]),
    ("one -0.0 is 0.0", &[-0.0], 0.0, 1.0, &[0.0]),
    ("flat -0.0 is 0.0", &[-0.0, -0.0], 0.0, 1.0, &[0.0, 0.0]),
    ("a flat negative series is 0", &[-2.5, -2.5], 0.0, 1.0, &[0.0, 0.0]),
    (
        "equal percentiles though one value differs",
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0],
        0.05,
        0.9,
        &[0.0; 11],
    ),
    (
        "the whole range of floats",
        &[-LARGEST, 0.0, LARGEST],
        0.0,
        1.0,
        &[0.0, 0.5, 1.0],
    ),
    ("subnormals only", &[5e-324, 0.0, 1e-323], 0.0, 1.0, &[0.5, 0.0, 1.0]),
    // the 50th percentile is 1e-320 (scaled by 1/2, as all), the span too: 1 / 1e-320 is beyond the floats
    (
        "a value far above a tiny span is 1, not an overflow",
        &[0.0, 0.0, 1e-320, 1e-320, 1.0],
        0.0,
        0.5,
        &[0.0, 0.0, 1.0, 1.0, 1.0],
    ),
];

#[test]
fn normalise_gives_numpys_bits_and_the_sign_of_each_zero() -> Result<(), Error> {
    for (id, given, lower, upper, expected) in EXACT {
        let bits = |values: &[f64]| values.iter().map(|value| value.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&normalised(given, lower, upper)?), bits(expected), "{id}");
    }
    Ok(())
}

#[test]
fn normalise_keeps_the_start_and_rate_and_gives_a_fraction_series() -> Result<(), Error> {
    let given = Series::new(
        4.0,
        vec![0.2, 0.4, 0.6],
        Unit::Fraction,
        SeriesOptions {
            start: 2.5,
            scaling: Some("by hand".to_owned()),
        },
    )
    .map_err(|source| Error::Format { source })?;
    let out = by_name(json!({"lower": 0.1, "upper": 0.9}), given)?;
    assert_eq!((out.start(), out.rate(), out.unit()), (2.5, 4.0, Unit::Fraction));
    assert_eq!(
        out.scaling(),
        Some("percentile 10 to 90 of the series, clipped to 0..1")
    );
    let at_minus_zero = Series::new(
        1.0,
        vec![1.0],
        Unit::Hertz,
        SeriesOptions {
            start: -0.0,
            scaling: None,
        },
    )
    .map_err(|source| Error::Format { source })?;
    assert_eq!(
        by_body(0.0, 1.0, at_minus_zero)?.start().to_bits(),
        (-0.0_f64).to_bits()
    );
    Ok(())
}

/// `(id, lower, upper, the percentiles said)`.
const SAID: [(&str, f64, f64, &str); 9] = [
    ("whole percentages", 0.1, 0.9, "10 to 90"),
    (
        "a seven digit fraction is not rounded",
        0.123_456_7,
        0.9,
        "12.34567 to 90",
    ),
    (
        "0.29 * 100 is 28.999999999999996, which reads 29",
        0.29,
        0.5,
        "29 to 50",
    ),
    ("tenths of a percent", 0.001, 0.999, "0.1 to 99.9"),
    ("a seven digit upper fraction", 0.5, 0.987_654_3, "50 to 98.76543"),
    ("the ends", 0.0, 1.0, "0 to 100"),
    ("-0.0 is said as Python says it", -0.0, 1.0, "-0 to 100"),
    (
        "a small percentage in the exponent's notation",
        1e-7,
        0.5,
        "1e-05 to 50",
    ),
    ("the smallest float", 5e-324, 0.01, "4.94065645841e-322 to 1"),
];

#[test]
fn normalise_says_the_percentiles_as_they_were_given() -> Result<(), Error> {
    for (id, lower, upper, said) in SAID {
        let out = by_name(json!({"lower": lower, "upper": upper}), ratio(&[0.0, 1.0, 2.0])?)?;
        assert_eq!(
            out.scaling(),
            Some(format!("percentile {said} of the series, clipped to 0..1").as_str()),
            "{id}"
        );
    }
    Ok(())
}

#[test]
fn normalise_refuses_percentiles_that_do_not_rise_or_are_out_of_range() {
    for (id, values, named) in [
        ("equal percentiles", json!({"lower": 0.05, "upper": 0.05}), "upper"),
        (
            "the upper below the lower",
            json!({"lower": 0.5, "upper": 0.25}),
            "upper",
        ),
        (
            "a lower percentile above its range",
            json!({"lower": 0.995, "upper": 1.0}),
            "lower",
        ),
        (
            "an upper percentile below its range",
            json!({"lower": 0.0, "upper": 0.005}),
            "upper",
        ),
        ("a lower that is not a number", json!({"lower": "low"}), "lower"),
    ] {
        let refused = ratio(&[0.0, 1.0]).and_then(|series| by_name(values, series));
        let paths: Vec<String> = match &refused {
            Err(Error::Params { shaper, problems, .. }) if shaper == "normalise" => {
                problems.iter().map(|problem| problem.path.clone()).collect()
            }
            other => vec![format!("not normalise's parameter error: {other:?}")],
        };
        assert_eq!(paths, [named], "{id}");
    }
}

#[test]
fn the_upper_percentile_must_be_above_the_lower_one() {
    for (id, given, expected) in [
        ("the defaults", json!({}), &[][..]),
        ("upper above lower", json!({"lower": 0.2, "upper": 0.5}), &[]),
        (
            "upper equal to lower",
            json!({"lower": 0.3, "upper": 0.3}),
            &[("upper", "must be above lower = 0.3 (got 0.3)")],
        ),
        (
            "upper below lower",
            json!({"lower": 0.5, "upper": 0.2}),
            &[("upper", "must be above lower = 0.5 (got 0.2)")],
        ),
        (
            "lower past its own range",
            json!({"lower": 2.0, "upper": 1.0}),
            &[("lower", "<= 0.99")],
        ),
    ] {
        match Params::parse(&given) {
            Ok(params) => assert!(expected.is_empty(), "{id}: accepted {params:?}, wanted {expected:?}"),
            Err(error) => check_problems(id, &error.problems, expected),
        }
    }
}

#[test]
fn normalise_needs_its_series_and_takes_it_as_a_series() -> Result<(), Error> {
    let thing = Registry::builtin().get("normalise")?;
    let missing = thing.call(&Call::default());
    assert!(
        matches!(&missing, Err(Error::Call { message }) if message.contains("normalise") && message.contains("'series'")),
        "{missing:?}"
    );
    let values = Call {
        inputs: [("series".to_owned(), Input::from_values(vec![1.0])?)].into(),
        ..Call::default()
    };
    let wrong = thing.call(&values);
    assert!(
        matches!(&wrong, Err(Error::Call { message }) if message.contains("'series' holds values")),
        "{wrong:?}"
    );
    Ok(())
}

#[test]
fn it_is_an_operation_giving_a_series() -> Result<(), Error> {
    let thing = Registry::builtin().get("normalise")?;
    assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Series));
    assert_eq!(thing.inputs(), [(InputKind::Series, true)]);
    assert!(
        thing
            .summary()
            .starts_with("A `fraction` series of the same start and rate"),
        "{}",
        thing.summary()
    );
    Ok(())
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn the_output_lies_in_0_to_1_and_never_falls_where_the_input_rises(
            mut given in prop::collection::vec(-1e6..1e6_f64, 1..30),
            lower in 0.0..0.5_f64,
            upper in 0.5..=1.0_f64,
        ) {
            given.sort_by(f64::total_cmp);
            let out = normalised(&given, lower, upper).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert!(out.iter().all(|value| (0.0..=1.0).contains(value)), "{out:?}");
            prop_assert!(out.windows(2).all(|pair| pair[0] <= pair[1]), "{out:?}");
        }

        #[test]
        fn scaling_and_shifting_whole_numbers_leaves_the_output_as_it_was(
            given in prop::collection::vec(-1000_i32..1000, 1..30),
            gain in prop_oneof![Just(0.5), Just(2.0), Just(3.0), Just(7.0), Just(1000.0)],
            shift in -1000_i32..1000,
        ) {
            let plain: Vec<f64> = given.iter().map(|value| f64::from(*value)).collect();
            let moved: Vec<f64> = plain.iter().map(|value| gain * value + f64::from(shift)).collect();
            let fail = |error: Error| TestCaseError::fail(error.to_string());
            let (plain, moved) = (normalised(&plain, 0.05, 0.98).map_err(fail)?, normalised(&moved, 0.05, 0.98).map_err(fail)?);
            prop_assert!(plain.iter().zip(&moved).all(|(a, b)| (a - b).abs() <= 1e-9), "{plain:?} {moved:?}");
        }
    }
}
