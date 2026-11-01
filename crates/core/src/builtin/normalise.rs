//! A `fraction` series of the same start and rate, with the `lower` percentile of the values at 0 and the `upper`
//! one at 1, clipped to 0..1 beyond them.
//!
//! A value v becomes (v - low) / (high - low), where low and high are the percentiles read by linear interpolation
//! between the sorted values (Hyndman and Fan, type 7, the default of numpy.quantile). When the two percentiles are
//! equal (a flat series, or a stretch of equal values holding both) every value is 0. The series states how it was
//! scaled in its `scaling`.

use crate::formats::{Problem, Series, SeriesOptions, Unit};
use crate::registry::{Output, Returns};
use crate::{Error, Operation};

/// The percentiles of the series that become 0 and 1.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// The share of the values, 0 to 0.99, at or below the level that becomes 0: 0.05 puts the 5th percentile at 0,
    /// and lower values are clipped to 0. Raising it clips more of the quiet end; below `upper`.
    #[param(default = 0.05, unit = Fraction, min = 0, max = 0.99)]
    pub lower: f64,
    /// The share of the values, 0.01 to 1, at or below the level that becomes 1: 0.98 puts the 98th percentile at 1,
    /// and higher values are clipped to 1. Lowering it clips more of the loud end; above `lower`.
    #[param(default = 0.98, unit = Fraction, min = 0.01, max = 1)]
    pub upper: f64,
}

/// `upper` must lie above `lower`: the two percentiles that become 1 and 0 are in order.
fn rule(params: &Params) -> Vec<Problem> {
    if params.upper > params.lower {
        return Vec::new();
    }
    vec![Problem {
        path: "upper".to_owned(),
        rule: format!("must be above lower = {:?} (got {:?})", params.lower, params.upper),
    }]
}

impl Operation for Params {
    const NAME: &'static str = "normalise";
    type Inputs = Series;
    const RETURNS: Returns = Returns::Series;

    /// `series` scaled so that its `lower` percentile is 0 and its `upper` one 1, clipped to 0..1, as a `fraction`
    /// series of the same start and rate whose `scaling` names the two percentiles.
    ///
    /// ```
    /// use bullterrier_control::builtin::normalise;
    /// use bullterrier_control::formats::{Series, SeriesOptions, Unit};
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::Operation;
    ///
    /// let loudness = Series::new(10.0, vec![-30.0, -20.0, -10.0], Unit::Decibels, SeriesOptions::default())?;
    /// let scaled = (normalise::Params { lower: 0.0, upper: 1.0 }).run(loudness)?;
    /// assert!(matches!(scaled, Output::Series(scaled) if scaled.values() == [0.0, 0.5, 1.0]
    ///     && scaled.unit() == Unit::Fraction
    ///     && scaled.scaling() == Some("percentile 0 to 100 of the series, clipped to 0..1")));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None: every series can be normalised.
    fn run(&self, series: Series) -> Result<Output, Error> {
        let scaled = within_one(series.values());
        let mut sorted = scaled.clone();
        // stable, and 0.0 and -0.0 one value (`+ 0.0` makes -0.0 0.0): equal values stay in the order given, which is
        // where numpy's partition leaves them in a short series (in a long one its order of equal zeros is its own)
        sorted.sort_by(|a, b| (a + 0.0).total_cmp(&(b + 0.0)));
        let (low, high) = (quantile(&sorted, self.lower), quantile(&sorted, self.upper));
        let span = high - low;
        let normalised = if span == 0.0 {
            vec![0.0; scaled.len()]
        } else {
            // a value far above a tiny span divides to beyond the floats; clamping makes it the 1 it should be (and keeps
            // a -0.0, as numpy's `clip` does)
            scaled.iter().map(|value| ((value - low) / span).clamp(0.0, 1.0)).collect()
        };
        let scaling = format!(
            "percentile {} to {} of the series, clipped to 0..1",
            significant(self.lower * 100.0),
            significant(self.upper * 100.0)
        );
        let options = SeriesOptions { start: series.start(), scaling: Some(scaling) };
        Series::new(series.rate(), normalised, Unit::Fraction, options)
            .map(Output::Series)
            .map_err(|source| Error::Format { source })
    }
}

/// `values` times the power of two that puts the largest magnitude in 0.5..1 (numpy's `ldexp(values,
/// -frexp(largest))`), so that no difference of two of them can overflow however large they are. Exact, but for a value
/// so much smaller than the largest that it falls among the subnormals, which is rounded once.
fn within_one(values: &[f64]) -> Vec<f64> {
    let largest = values.iter().fold(0.0_f64, |largest, value| largest.max(value.abs()));
    let exponent = frexp_exponent(largest);
    // 2^-exponent is beyond the floats when the exponent is below -1023: two steps then, each exact (scaling up)
    let (first, second) = if exponent < -1023 {
        (two_to(1023), two_to(-exponent - 1023))
    } else {
        (two_to(-exponent), 1.0)
    };
    values.iter().map(|value| value * first * second).collect()
}

/// The exponent `e` of `x` (finite, `>= 0`) as C's `frexp` gives it: `x = m * 2^e` with `m` in 0.5..1; 0 for 0.
fn frexp_exponent(x: f64) -> i32 {
    if x == 0.0 {
        return 0;
    }
    if x < f64::MIN_POSITIVE {
        // a subnormal, made normal exactly
        return frexp_exponent(x * two_to(64)) - 64;
    }
    // the sign bit is 0: the 11 bits of the biased exponent are all that is left above the 52 of the mantissa
    let biased = (x.to_bits() >> 52) as i32;
    biased - 1022
}

/// 2^`k`, exactly, for `k` in -1074..=1023 (the powers of two that are floats).
#[expect(clippy::cast_sign_loss, reason = "k + 1023 is 1 to 2046 and k + 1074 is 0 to 51 where each is used")]
fn two_to(k: i32) -> f64 {
    if k >= -1022 {
        f64::from_bits(((k + 1023) as u64) << 52)
    } else {
        f64::from_bits(1_u64 << (k + 1074) as u64)
    }
}

/// The `q` quantile of `sorted` (ascending, at least one value) as numpy 2.5's `quantile` reads it by default (method
/// "linear": Hyndman and Fan, type 7), to the bit: at index `q * (n - 1)`, between the values around it in proportion.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    let last = sorted.len() - 1;
    #[expect(clippy::cast_precision_loss, reason = "an index past 2^53 does not fit in memory")]
    let end = last as f64;
    let at = end * q;
    if at >= end {
        // numpy takes the last value as both neighbours and measures the weight from index -1
        return lerp(sorted[last], sorted[last], at + 1.0);
    }
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "0 <= at < last: its floor")]
    let below = at as usize;
    // the weight against the index as a whole number: an `at` of -0.0 keeps its sign (`at - at.floor()` would not)
    #[expect(clippy::cast_precision_loss, reason = "below last")]
    let weight = at - below as f64;
    lerp(sorted[below], sorted[below + 1], weight)
}

/// `a` to `b` by `t` as numpy's `_lerp` computes it: from `b` back when `t` is at least a half, so that a `t` of 1 gives
/// `b` exactly.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    let span = b - a;
    if t >= 0.5 { b - span * (1.0 - t) } else { a + span * t }
}

/// The significant digits of `x` (finite) the scaling text says: Python's `format(x, ".12g")`. 12 digits keep any
/// percentage typed and drop the noise of multiplying by 100 (0.29 * 100 is 28.999999999999996, which reads 29).
fn significant(x: f64) -> String {
    const DIGITS: usize = 12;
    let sign = if x.is_sign_negative() { "-" } else { "" };
    // rounded once, to the digits (`{:e}` writes `d.ddddddddddde<exponent>`)
    let scientific = format!("{:.*e}", DIGITS - 1, x.abs());
    let Some((digits, Ok(exponent))) = scientific
        .split_once('e')
        .map(|(mantissa, exponent)| (mantissa.replace('.', ""), exponent.parse::<i32>()))
    else {
        return scientific;
    };
    let point = |whole: &str, fraction: &str| match fraction.trim_end_matches('0') {
        "" => whole.to_owned(),
        fraction => format!("{whole}.{fraction}"),
    };
    let size = exponent.unsigned_abs() as usize;
    let text = match exponent {
        // as %g: plain from 1e-4 to below 1e12, in the exponent's notation beyond
        0..12 => point(&digits[..=size], &digits[size + 1..]),
        -4..0 => point("0", &format!("{}{digits}", "0".repeat(size - 1))),
        _ => format!("{}e{}{size:02}", point(&digits[..1], &digits[1..]), if exponent < 0 { '-' } else { '+' }),
    };
    format!("{sign}{text}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Params, significant};
    use crate::Params as _;
    use crate::formats::{Series, SeriesOptions, Unit, check_problems};
    use crate::inputs::{Input, InputKind};
    use crate::registry::{Call, Kind, Output, Returns};
    use crate::{Error, Operation, Registry};

    const LARGEST: f64 = f64::MAX;

    /// `values` as a `ratio` series ten times a second from 0.
    fn ratio(values: &[f64]) -> Result<Series, Error> {
        Series::new(10.0, values.to_vec(), Unit::Ratio, SeriesOptions::default()).map_err(|source| Error::Format { source })
    }

    fn series_of(output: Output) -> Result<Series, Error> {
        match output {
            Output::Series(series) => Ok(series),
            other => Err(Error::Call { message: format!("normalise gave {other:?}") }),
        }
    }

    /// `series` normalised by the thing named `normalise`, through the registry: its input checked, its values read.
    fn by_name(values: serde_json::Value, series: Series) -> Result<Series, Error> {
        let call = Call { inputs: [("series".to_owned(), Input::Series(series))].into(), values, ..Call::default() };
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
        ("five values, the 25th and 90th", &[3.0, 1.0, 4.0, 1.0, 5.0], 0.25, 0.9, &[2.0 / 3.6, 0.0, 3.0 / 3.6, 0.0, 1.0]),
        // the median is index 2 (3): (v - 3) / 1.6
        ("five values, the median and the 90th", &[3.0, 1.0, 4.0, 1.0, 5.0], 0.5, 0.9, &[0.0, 0.0, 1.0 / 1.6, 0.0, 1.0]),
        ("five values, the 25th and the median", &[3.0, 1.0, 4.0, 1.0, 5.0], 0.25, 0.5, &[1.0, 0.0, 1.0, 0.0, 1.0]),
        // 11 values 0..10: the 10th percentile is at index 1 and the 90th at index 9: (v - 1) / 8
        ("eleven, the 10th and 90th", &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0], 0.1, 0.9,
            &[0.0, 0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0, 1.0]),
        // index 3.5 is halfway between 30 and 40: 35; index 1 is 10: (v - 10) / 25
        ("the 87.5th halfway between two", &[0.0, 10.0, 20.0, 30.0, 40.0], 0.25, 0.875, &[0.0, 0.0, 0.4, 0.8, 1.0]),
        ("the 12.5th halfway between two", &[0.0, 10.0, 20.0, 30.0, 40.0], 0.125, 0.75, &[0.0, 0.2, 0.6, 1.0, 1.0]),
        ("the order of the values only says where each lands", &[40.0, 0.0, 30.0, 10.0, 20.0], 0.25, 0.875, &[1.0, 0.0, 0.8, 0.0, 0.4]),
        // the 98th is index 9.8, 5 + 0.8 * 5 = 9; the 5th is 0
        ("the defaults among zeros", &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0, 10.0], 0.05, 0.98,
            &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0 / 9.0, 1.0]),
        ("the 0th and 100th are the lowest and highest", &[-4.0, -2.0, 0.0, 2.0, 4.0], 0.0, 1.0, &[0.0, 0.25, 0.5, 0.75, 1.0]),
        ("two values, the ends", &[1.0, 3.0], 0.0, 1.0, &[0.0, 1.0]),
        // index 0.1 is 1 + 0.1 * 2 = 1.2; index 0.9 is 2.8: (v - 1.2) / 1.6
        ("two values, inside", &[1.0, 3.0], 0.1, 0.9, &[0.0, 1.0]),
    ];

    #[test]
    fn normalise_puts_the_percentiles_at_0_and_1_and_clips_what_lies_beyond() -> Result<(), Error> {
        for (id, given, lower, upper, expected) in BY_HAND {
            let got = normalised(given, lower, upper)?;
            assert_eq!(got.len(), expected.len(), "{id}");
            assert!(got.iter().zip(expected).all(|(got, want)| (got - want).abs() <= 1e-12), "{id}: {got:?}");
        }
        Ok(())
    }

    #[test]
    fn by_the_defaults_normalise_reads_the_5th_and_98th_percentiles() -> Result<(), Error> {
        // 101 values 0..100: the 5th percentile is value 5 and the 98th value 98: (v - 5) / 93
        let ramp: Vec<f64> = (0..=100).map(f64::from).collect();
        let out = by_name(json!({}), ratio(&ramp)?)?;
        let picked: Vec<f64> = [0, 5, 6, 50, 98, 100].iter().map(|index| out.values()[*index]).collect();
        let expected = [0.0, 0.0, 1.0 / 93.0, 45.0 / 93.0, 1.0, 1.0];
        assert!(picked.iter().zip(expected).all(|(got, want)| (got - want).abs() <= 1e-12), "{picked:?}");
        assert_eq!(out.scaling(), Some("percentile 5 to 98 of the series, clipped to 0..1"));
        Ok(())
    }

    /// `(id, values, lower, upper, outputs)` to the bit, the outputs as numpy 2.5.4 gives them through the Python
    /// body this replaced (the sign of each zero too).
    #[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
    const EXACT: [(&str, &[f64], f64, f64, &[f64]); 16] = [
        // index 1.6 between 0.3 and 1 (scaled by 1/2): from 1 back, 1 - 0.7 * 0.4, not 0.3 + 0.7 * 0.6
        ("a weight of a half or more is read back from the upper value", &[0.0, 0.3, 1.0], 0.0, 0.8,
            &[0.0, 0.416_666_666_666_666_63, 1.0]),
        // the 0th percentile is -0.0 + 0 * 0.0 = 0.0, and -0.0 - 0.0 is -0.0
        ("-0.0 below a lower percentile of 0.0 stays -0.0", &[-0.0, -0.0, 1.0], 0.0, 1.0, &[-0.0, -0.0, 1.0]),
        // a weight of -0.0 keeps the -0.0, and -0.0 - -0.0 is 0.0
        ("-0.0 below a lower percentile of -0.0 is 0.0", &[-0.0, -0.0, 1.0], -0.0, 1.0, &[0.0, 0.0, 1.0]),
        // index 0.4: -0.0 + 0 * 0.4 = 0.0
        ("a weight below a half between two -0.0 gives 0.0", &[-0.0, -0.0, 1.0], 0.2, 1.0, &[-0.0, -0.0, 1.0]),
        // index 0.5: -0.0 - 0 * 0.5 = -0.0
        ("a weight of a half between two -0.0 gives -0.0", &[-0.0, -0.0, 1.0], 0.25, 1.0, &[0.0, 0.0, 1.0]),
        // 0.0 and -0.0 are one value to the sort, so the one given first is the 0th percentile: 0.0 + -0.0 * -0.0 is 0.0
        ("0.0 given before -0.0 is the lowest", &[0.0, -0.0, 1.0], -0.0, 1.0, &[0.0, -0.0, 1.0]),
        // index 0.8 between the first two zeros, read back from the second: 0.0 - 0 * 0.2 = 0.0
        ("the zeros stay in the order given", &[-0.0, 0.0, -0.0, 0.0, 2.0], 0.2, 1.0, &[-0.0, 0.0, -0.0, 0.0, 1.0]),
        ("a flat series is all 0", &[3.0, 3.0, 3.0], 0.05, 0.98, &[0.0, 0.0, 0.0]),
        ("one value is 0", &[3.0], 0.05, 0.98, &[0.0]),
        ("one -0.0 is 0.0", &[-0.0], 0.0, 1.0, &[0.0]),
        ("flat -0.0 is 0.0", &[-0.0, -0.0], 0.0, 1.0, &[0.0, 0.0]),
        ("a flat negative series is 0", &[-2.5, -2.5], 0.0, 1.0, &[0.0, 0.0]),
        ("equal percentiles though one value differs", &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0], 0.05, 0.9,
            &[0.0; 11]),
        ("the whole range of floats", &[-LARGEST, 0.0, LARGEST], 0.0, 1.0, &[0.0, 0.5, 1.0]),
        ("subnormals only", &[5e-324, 0.0, 1e-323], 0.0, 1.0, &[0.5, 0.0, 1.0]),
        // the 50th percentile is 1e-320 (scaled by 1/2, as all), the span too: 1 / 1e-320 is beyond the floats
        ("a value far above a tiny span is 1, not an overflow", &[0.0, 0.0, 1e-320, 1e-320, 1.0], 0.0, 0.5, &[0.0, 0.0, 1.0, 1.0, 1.0]),
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
        let given = Series::new(4.0, vec![0.2, 0.4, 0.6], Unit::Fraction, SeriesOptions {
            start: 2.5,
            scaling: Some("by hand".to_owned()),
        })
        .map_err(|source| Error::Format { source })?;
        let out = by_name(json!({"lower": 0.1, "upper": 0.9}), given)?;
        assert_eq!((out.start(), out.rate(), out.unit()), (2.5, 4.0, Unit::Fraction));
        assert_eq!(out.scaling(), Some("percentile 10 to 90 of the series, clipped to 0..1"));
        let at_minus_zero = Series::new(1.0, vec![1.0], Unit::Hertz, SeriesOptions { start: -0.0, scaling: None })
            .map_err(|source| Error::Format { source })?;
        assert_eq!(by_body(0.0, 1.0, at_minus_zero)?.start().to_bits(), (-0.0_f64).to_bits());
        Ok(())
    }

    /// `(id, lower, upper, the percentiles said)`.
    const SAID: [(&str, f64, f64, &str); 9] = [
        ("whole percentages", 0.1, 0.9, "10 to 90"),
        ("a seven digit fraction is not rounded", 0.123_456_7, 0.9, "12.34567 to 90"),
        ("0.29 * 100 is 28.999999999999996, which reads 29", 0.29, 0.5, "29 to 50"),
        ("tenths of a percent", 0.001, 0.999, "0.1 to 99.9"),
        ("a seven digit upper fraction", 0.5, 0.987_654_3, "50 to 98.76543"),
        ("the ends", 0.0, 1.0, "0 to 100"),
        ("-0.0 is said as Python says it", -0.0, 1.0, "-0 to 100"),
        ("a small percentage in the exponent's notation", 1e-7, 0.5, "1e-05 to 50"),
        ("the smallest float", 5e-324, 0.01, "4.94065645841e-322 to 1"),
    ];

    #[test]
    fn normalise_says_the_percentiles_as_they_were_given() -> Result<(), Error> {
        for (id, lower, upper, said) in SAID {
            let out = by_name(json!({"lower": lower, "upper": upper}), ratio(&[0.0, 1.0, 2.0])?)?;
            assert_eq!(out.scaling(), Some(format!("percentile {said} of the series, clipped to 0..1").as_str()), "{id}");
        }
        Ok(())
    }

    #[test]
    fn the_scaling_text_writes_a_number_as_pythons_12_digit_g_format() {
        // each `format(x, ".12g")` as Python 3.13 writes it
        for (x, python) in [
            (0.1, "0.1"),
            (1e-5, "1e-05"),
            (1e-4, "0.0001"),
            (1e16, "1e+16"),
            (123_456_789_012_345.0, "1.23456789012e+14"),
            (123_456_789_012.0, "123456789012"),
            (999_999_999_999.5, "1e+12"),
            (1_000_000_000_005.0, "1e+12"),
            (1_000_000_000_015.0, "1.00000000002e+12"),
            (-0.0, "-0"),
            (0.0, "0"),
            (5.0, "5"),
            (-12.5, "-12.5"),
            (28.999_999_999_999_996, "29"),
            (99.999_999_999_999_99, "100"),
            (0.000_123_456_789_012_34, "0.000123456789012"),
            (1.5e-300, "1.5e-300"),
            (4.94e-322, "4.94065645841e-322"),
        ] {
            assert_eq!(significant(x), python, "{x:e}");
        }
    }

    #[test]
    fn normalise_refuses_percentiles_that_do_not_rise_or_are_out_of_range() {
        for (id, values, named) in [
            ("equal percentiles", json!({"lower": 0.05, "upper": 0.05}), "upper"),
            ("the upper below the lower", json!({"lower": 0.5, "upper": 0.25}), "upper"),
            ("a lower percentile above its range", json!({"lower": 0.995, "upper": 1.0}), "lower"),
            ("an upper percentile below its range", json!({"lower": 0.0, "upper": 0.005}), "upper"),
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
            ("upper equal to lower", json!({"lower": 0.3, "upper": 0.3}), &[("upper", "must be above lower = 0.3 (got 0.3)")]),
            ("upper below lower", json!({"lower": 0.5, "upper": 0.2}), &[("upper", "must be above lower = 0.5 (got 0.2)")]),
            ("lower past its own range", json!({"lower": 2.0, "upper": 1.0}), &[("lower", "<= 0.99")]),
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
        assert!(matches!(&missing, Err(Error::Call { message }) if message.contains("normalise") && message.contains("'series'")), "{missing:?}");
        let values = Call { inputs: [("series".to_owned(), Input::from_values(vec![1.0])?)].into(), ..Call::default() };
        let wrong = thing.call(&values);
        assert!(matches!(&wrong, Err(Error::Call { message }) if message.contains("'series' holds values")), "{wrong:?}");
        Ok(())
    }

    #[test]
    fn it_is_an_operation_giving_a_series() -> Result<(), Error> {
        let thing = Registry::builtin().get("normalise")?;
        assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Series));
        assert_eq!(thing.inputs(), [(InputKind::Series, true)]);
        assert!(thing.summary().starts_with("A `fraction` series of the same start and rate"), "{}", thing.summary());
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
}
