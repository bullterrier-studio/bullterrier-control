//! [`Series`]: values at a fixed rate.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use super::error::FormatError;
use super::field::{Number, check, decode, push, required};
use super::unit::Unit;

/// Values at a fixed rate: sample *i* is at `start + i / rate` seconds. A `fraction` series states its `scaling` and
/// stays within 0..=1.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Series {
    start: f64,
    rate: f64,
    values: Vec<f64>,
    unit: Unit,
    #[serde(skip_serializing_if = "Option::is_none")]
    scaling: Option<String>,
}

/// The optional parts of a [`Series`]; end a literal with `..Default::default()`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesOptions {
    /// When the first sample is, in seconds, `>= 0`. Default 0.
    pub start: f64,
    /// How 0..1 was reached (`"5th to 98th percentile of the song"`); required, and not empty, for a `fraction` series.
    pub scaling: Option<String>,
}

impl Series {
    /// The series of `values` sampled `rate` times per second, measuring `unit`.
    ///
    /// # Errors
    /// A [`FormatError`] listing every rule broken.
    pub fn new(rate: f64, values: Vec<f64>, unit: Unit, options: SeriesOptions) -> Result<Self, FormatError> {
        Self::from_parts(options.start, Some(rate), Some(values), Some(unit), options.scaling)
    }

    /// The one place the rules are written; `None` is a field the data lacked. Problems come in the order of the
    /// fields they are about: a `fraction` series' range rule is about its values, its scaling rule about `scaling`.
    fn from_parts(
        start: f64,
        rate: Option<f64>,
        values: Option<Vec<f64>>,
        unit: Option<Unit>,
        scaling: Option<String>,
    ) -> Result<Self, FormatError> {
        let mut problems = Vec::new();
        check(&mut problems, "start", start, "must be >= 0", |x| x >= 0.0);
        match rate {
            Some(rate) => check(&mut problems, "rate", rate, "must be > 0", |x| x > 0.0),
            None => required(&mut problems, "rate"),
        }
        match &values {
            None => required(&mut problems, "values"),
            Some(values) if values.is_empty() => push(&mut problems, "values", "needs at least one value"),
            Some(values) => {
                for (index, &value) in values.iter().enumerate() {
                    let path = format!("values[{index}]");
                    if unit == Some(Unit::Fraction) {
                        let rule = "must be in [0, 1] when unit is \"fraction\"";
                        check(&mut problems, &path, value, rule, |x| (0.0..=1.0).contains(&x));
                    } else {
                        check(&mut problems, &path, value, "", |_| true);
                    }
                }
            }
        }
        if unit.is_none() {
            required(&mut problems, "unit");
        }
        if unit == Some(Unit::Fraction) && scaling.as_deref().is_none_or(str::is_empty) {
            push(
                &mut problems,
                "scaling",
                "is required when unit is \"fraction\": say how 0..1 was reached",
            );
        }
        match (rate, values, unit) {
            (Some(rate), Some(values), Some(unit)) if problems.is_empty() => Ok(Self {
                start,
                rate,
                values,
                unit,
                scaling,
            }),
            _ => Err(FormatError::new(problems)),
        }
    }

    /// Read a series from any serde format.
    ///
    /// # Errors
    /// A [`FormatError`].
    pub fn from_deserializer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, FormatError> {
        let raw: RawSeries = decode(deserializer)?;
        Self::from_parts(
            raw.start.map_or(0.0, |number| number.0),
            raw.rate.map(|number| number.0),
            raw.values
                .map(|values| values.into_iter().map(|number| number.0).collect()),
            raw.unit,
            raw.scaling,
        )
    }

    /// When the first sample is, in seconds.
    #[must_use]
    pub const fn start(&self) -> f64 {
        self.start
    }

    /// Samples per second.
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// The samples.
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// What the values measure.
    #[must_use]
    pub const fn unit(&self) -> Unit {
        self.unit
    }

    /// How 0..1 was reached, if said.
    #[must_use]
    pub fn scaling(&self) -> Option<&str> {
        self.scaling.as_deref()
    }
}

/// The fields as a file spells them, before any rule is checked; an absent field and a `null` one are both `None`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSeries {
    start: Option<Number>,
    rate: Option<Number>,
    values: Option<Vec<Number>>,
    unit: Option<Unit>,
    scaling: Option<String>,
}

impl<'de> Deserialize<'de> for Series {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_deserializer(deserializer).map_err(D::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "the getters give back exactly what was given")]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::{Value, json};

    fn check_against<T: std::fmt::Debug>(id: &str, result: Result<T, FormatError>, expected: &[(&str, &str)]) {
        match result {
            Ok(value) => assert!(expected.is_empty(), "{id}: accepted {value:?}, wanted {expected:?}"),
            Err(error) => {
                assert!(!expected.is_empty(), "{id}: refused with {error}");
                let got: Vec<(&str, &str)> = error
                    .problems
                    .iter()
                    .map(|p| (p.path.as_str(), p.rule.as_str()))
                    .collect();
                let paths: Vec<&str> = got.iter().map(|(path, _)| *path).collect();
                let wanted: Vec<&str> = expected.iter().map(|(path, _)| *path).collect();
                assert_eq!(paths, wanted, "{id}: {error}");
                for ((path, rule), (_, part)) in got.iter().zip(expected) {
                    assert!(rule.contains(part), "{id}: {path}: {rule:?} lacks {part:?}");
                }
            }
        }
    }

    /// The problems wanted: each a path and a part of its rule.
    type Expected = &'static [(&'static str, &'static str)];
    /// A case: its name, `rate`, the values, the unit, the options and the problems wanted.
    type Row = (&'static str, f64, Vec<f64>, Unit, SeriesOptions, Expected);

    fn scaled(text: &str) -> SeriesOptions {
        SeriesOptions {
            scaling: Some(text.to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn each_rule_holds_on_one_side_of_its_boundary_and_breaks_on_the_other() {
        let some = SeriesOptions::default;
        #[rustfmt::skip]
        let rows: Vec<Row> = vec![
            ("one sample is enough", 100.0, vec![0.5], Unit::Ratio, some(), &[]),
            ("negative values are fine for a ratio", 100.0, vec![-3.0, 1e9], Unit::Decibels, some(), &[]),
            ("rate: tiny but positive", 0.001, vec![1.0], Unit::Hertz, some(), &[]),
            ("rate: zero", 0.0, vec![1.0], Unit::Hertz, some(), &[("rate", "> 0 (got 0.0)")]),
            ("rate: negative", -10.0, vec![1.0], Unit::Hertz, some(), &[("rate", "> 0 (got -10.0)")]),
            ("rate: infinite", f64::INFINITY, vec![1.0], Unit::Hertz, some(), &[("rate", "finite")]),
            ("start: zero", 10.0, vec![1.0], Unit::Midi, SeriesOptions { start: 0.0, ..some() }, &[]),
            ("start: negative zero", 10.0, vec![1.0], Unit::Midi, SeriesOptions { start: -0.0, ..some() }, &[]),
            ("start: below zero", 10.0, vec![1.0], Unit::Midi, SeriesOptions { start: -0.1, ..some() }, &[("start", ">= 0 (got -0.1)")]),
            ("values: none", 10.0, vec![], Unit::Seconds, some(), &[("values", "at least one")]),
            ("values: NaN at index 1", 10.0, vec![1.0, f64::NAN, 3.0], Unit::Beats, some(), &[("values[1]", "finite (got NaN)")]),
            ("values: two infinities", 10.0, vec![f64::NEG_INFINITY, 0.0, f64::INFINITY], Unit::Beats, some(),
                &[("values[0]", "got -Infinity"), ("values[2]", "got Infinity")]),
            ("fraction: scaled and inside, ends included", 10.0, vec![0.0, 0.5, 1.0], Unit::Fraction, scaled("p5 to p98"), &[]),
            ("fraction: just above one", 10.0, vec![0.0, 1.000_000_1], Unit::Fraction, scaled("p5 to p98"), &[("values[1]", "[0, 1]")]),
            ("fraction: just below zero", 10.0, vec![-0.000_000_1], Unit::Fraction, scaled("p5 to p98"), &[("values[0]", "[0, 1]")]),
            ("fraction: no scaling", 10.0, vec![0.5], Unit::Fraction, some(), &[("scaling", "required")]),
            ("fraction: empty scaling", 10.0, vec![0.5], Unit::Fraction, scaled(""), &[("scaling", "required")]),
            ("other units: scaling is optional, even empty", 10.0, vec![0.5], Unit::Ratio, scaled(""), &[]),
            ("other units: values may leave 0..1", 10.0, vec![5.0], Unit::Ratio, scaled("whatever"), &[]),
            (
                "several broken: all listed in field order",
                0.0,
                vec![0.5, 1.5, 0.2, 0.3, 0.4, -1.0],
                Unit::Fraction,
                SeriesOptions { start: -1.0, scaling: None },
                &[("start", ">= 0"), ("rate", "> 0"), ("values[1]", "1.5"), ("values[5]", "-1.0"), ("scaling", "required")],
            ),
            (
                "nothing to read and nothing said",
                -1.0,
                vec![],
                Unit::Fraction,
                SeriesOptions { start: f64::NAN, scaling: None },
                &[("start", "finite"), ("rate", "> 0"), ("values", "at least one"), ("scaling", "required")],
            ),
        ];
        for (id, rate, values, unit, options, expected) in rows {
            check_against(id, Series::new(rate, values, unit, options), expected);
        }
    }

    #[test]
    fn a_series_keeps_what_it_was_given() -> Result<(), FormatError> {
        let series = Series::new(
            50.0,
            vec![0.25, 0.75],
            Unit::Fraction,
            SeriesOptions {
                start: 1.5,
                scaling: Some("p5 to p98".to_owned()),
            },
        )?;
        assert_eq!(
            (series.start(), series.rate(), series.values()),
            (1.5, 50.0, &[0.25, 0.75][..])
        );
        assert_eq!((series.unit(), series.scaling()), (Unit::Fraction, Some("p5 to p98")));
        let plain = Series::new(10.0, vec![1.0], Unit::Hertz, SeriesOptions::default())?;
        assert_eq!((plain.start(), plain.scaling()), (0.0, None));
        Ok(())
    }

    fn read(text: &str) -> Result<Series, FormatError> {
        Series::from_deserializer(&mut serde_json::Deserializer::from_str(text))
    }

    #[test]
    fn json_is_read_strictly_and_every_broken_rule_is_listed_with_its_path() {
        #[rustfmt::skip]
        let rows: &[(&str, &str, Expected)] = &[
            ("the smallest series", r#"{"rate": 100, "values": [0.1, 2], "unit": "ratio"}"#, &[]),
            ("everything", r#"{"start": 1, "rate": 10, "values": [0, 1], "unit": "fraction", "scaling": "p5"}"#, &[]),
            ("null is absent", r#"{"start": null, "rate": 1, "values": [1], "unit": "hertz", "scaling": null}"#, &[]),
            ("nothing said", "{}", &[("rate", "is required"), ("values", "is required"), ("unit", "is required")]),
            ("null values are missing", r#"{"rate": 1, "values": null, "unit": "hertz"}"#, &[("values", "is required")]),
            ("a string rate", r#"{"rate": "1", "values": [1], "unit": "hertz"}"#, &[("rate", "string")]),
            ("a bool value", r#"{"rate": 1, "values": [1, true, 3], "unit": "hertz"}"#, &[("values[1]", "boolean")]),
            ("a null value", r#"{"rate": 1, "values": [1, null], "unit": "hertz"}"#, &[("values[1]", "")]),
            ("2^53 + 1 in a value", r#"{"rate": 1, "values": [1, 9007199254740993], "unit": "hertz"}"#, &[("values[1]", "2^53")]),
            ("values not an array", r#"{"rate": 1, "values": "abc", "unit": "hertz"}"#, &[("values", "sequence")]),
            ("a capitalised unit", r#"{"rate": 1, "values": [1], "unit": "Hertz"}"#, &[("unit", "fraction, decibels")]),
            ("a number for the unit", r#"{"rate": 1, "values": [1], "unit": 3}"#, &[("unit", "string")]),
            ("an array of the fields is not a series", r#"[null, 1, [1], "hertz", null]"#, &[("", "an object")]),
            ("an unknown field", r#"{"rate": 1, "values": [1], "unit": "hertz", "name": "x"}"#, &[("name", "unknown field")]),
            ("the wrong type stops at its field", r#"{"rate": true, "values": [], "unit": "hertz"}"#, &[("rate", "boolean")]),
            ("rules broken in several fields", r#"{"start": -1, "rate": 0, "values": [], "unit": "fraction"}"#,
                &[("start", ">= 0"), ("rate", "> 0"), ("values", "at least one"), ("scaling", "required")]),
            ("the fraction range reaches the values", r#"{"rate": 1, "values": [0, 2, 0, 3], "unit": "fraction", "scaling": "x"}"#,
                &[("values[1]", "[0, 1]"), ("values[3]", "[0, 1]")]),
        ];
        for (id, text, expected) in rows {
            check_against(id, read(text), expected);
        }
    }

    #[test]
    fn serde_json_reads_and_writes_a_series_leaving_unset_scaling_out() -> Result<(), serde_json::Error> {
        let series: Series = serde_json::from_str(r#"{"rate": 4, "values": [1, 2.5], "unit": "decibels"}"#)?;
        assert_eq!(
            serde_json::to_value(&series)?,
            json!({"start": 0.0, "rate": 4.0, "values": [1.0, 2.5], "unit": "decibels"})
        );
        let refused = serde_json::from_str::<Series>(r#"{"rate": 0, "values": [1], "unit": "hertz"}"#).err();
        assert!(refused.is_some_and(|error| error.to_string().contains("rate: must be > 0 (got 0.0)")));
        Ok(())
    }

    fn valid_series() -> impl Strategy<Value = Series> {
        let scaling = proptest::option::of(".{0,12}");
        let others = prop::sample::select(
            Unit::ALL
                .iter()
                .copied()
                .filter(|unit| *unit != Unit::Fraction)
                .collect::<Vec<_>>(),
        );
        let wide = (
            0.0..1.0e6_f64,
            1.0e-3..1.0e6_f64,
            prop::collection::vec(-1.0e12..1.0e12_f64, 1..40),
            others,
            scaling.clone(),
        )
            .prop_map(|(start, rate, values, unit, scaling)| {
                Series::new(rate, values, unit, SeriesOptions { start, scaling })
            });
        let fraction = (
            0.0..1.0e6_f64,
            1.0e-3..1.0e6_f64,
            prop::collection::vec(0.0..=1.0_f64, 1..40),
            ".{1,12}",
        )
            .prop_map(|(start, rate, values, scaling)| {
                Series::new(
                    rate,
                    values,
                    Unit::Fraction,
                    SeriesOptions {
                        start,
                        scaling: Some(scaling),
                    },
                )
            });
        prop_oneof![wide, fraction].prop_filter_map("generated in range", Result::ok)
    }

    proptest! {
        #[test]
        fn a_valid_series_survives_json_text_exactly(series in valid_series()) {
            let text = serde_json::to_string(&series).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(read(&text), Ok(series), "{}", text);
        }

        #[test]
        fn a_valid_series_survives_a_json_value(series in valid_series()) {
            let value: Value = serde_json::to_value(&series).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(Series::from_deserializer(value), Ok(series));
        }

        #[test]
        fn a_fraction_value_is_accepted_exactly_when_it_lies_in_zero_to_one(value in any::<f64>()) {
            let result = Series::new(1.0, vec![value], Unit::Fraction, scaled("x"));
            prop_assert_eq!(result.is_ok(), (0.0..=1.0).contains(&value), "{}", value);
        }

        #[test]
        fn a_refused_series_names_each_broken_value_once(
            values in prop::collection::vec(any::<f64>(), 0..30),
        ) {
            let broken: Vec<String> = values
                .iter()
                .enumerate()
                .filter(|(_, v)| !(0.0..=1.0).contains(*v))
                .map(|(i, _)| format!("values[{i}]"))
                .collect();
            let result = Series::new(1.0, values.clone(), Unit::Fraction, scaled("x"));
            let paths: Vec<String> = match result {
                Ok(_) => Vec::new(),
                Err(error) => error.problems.into_iter().map(|p| p.path).collect(),
            };
            let expected = if values.is_empty() { vec!["values".to_owned()] } else { broken };
            prop_assert_eq!(paths, expected);
        }
    }
}
