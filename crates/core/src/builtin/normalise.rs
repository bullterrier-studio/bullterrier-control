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
        // stable, and 0.0 and -0.0 one value (`+ 0.0` makes -0.0 0.0): equal zeros stay in the order given. numpy's
        // partition leaves equal zeros in an order of its own, so from 5 values the sign of an output zero can differ
        // from the Python body this replaced; the values never differ
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
    use super::significant;

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
}
