//! What the formats share about a number field: how it is read from JSON, shown in a message and checked.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, Deserialize, Deserializer, Visitor};

use super::error::{FormatError, Problem};

/// The largest whole number a JSON number can carry into an `f64` without rounding (2^53): beyond it, two different
/// integers in a file would read as one value.
const LARGEST_EXACT: u64 = 1 << 53;

/// A JSON number read strictly: an integer or a decimal, never `true`, a string or `null`, and a whole number only
/// while it is exact in an `f64`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Number(pub(crate) f64);

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NumberVisitor)
    }
}

struct NumberVisitor;

impl Visitor<'_> for NumberVisitor {
    type Value = Number;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a number")
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Number, E> {
        if value.is_finite() {
            Ok(Number(value))
        } else {
            Err(E::custom(format!("must be finite (got {})", show(value))))
        }
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Number, E> {
        if value <= LARGEST_EXACT {
            #[expect(clippy::cast_precision_loss, reason = "at most 2^53: exact")]
            Ok(Number(value as f64))
        } else {
            Err(E::custom(format!(
                "integer {value} is beyond 2^53, which a number cannot hold exactly"
            )))
        }
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Number, E> {
        if value.unsigned_abs() <= LARGEST_EXACT {
            #[expect(clippy::cast_precision_loss, reason = "at most 2^53 in size: exact")]
            Ok(Number(value as f64))
        } else {
            Err(E::custom(format!(
                "integer {value} is beyond 2^53, which a number cannot hold exactly"
            )))
        }
    }
}

/// The number as a message shows it: JSON where JSON can write it (`0.5`, `-1.0`), else `NaN`, `Infinity`,
/// `-Infinity`.
pub(crate) fn show(value: f64) -> String {
    serde_json::Number::from_f64(value).map_or_else(
        || {
            if value.is_nan() {
                "NaN".to_owned()
            } else if value > 0.0 {
                "Infinity".to_owned()
            } else {
                "-Infinity".to_owned()
            }
        },
        |number| number.to_string(),
    )
}

/// A struct read from a JSON object only: serde's derived `Deserialize` also reads a struct from an array (field by
/// field, in order), which a file must not be allowed to do.
struct Object<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = Object<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object")
            }

            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Object<T>, A::Error> {
                T::deserialize(de::value::MapAccessDeserializer::new(map)).map(Object)
            }
        }

        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

/// Deserialize a `T` from a JSON object and describe how it failed, naming the field.
pub(crate) fn decode<'de, T: Deserialize<'de>, D: Deserializer<'de>>(deserializer: D) -> Result<T, FormatError> {
    serde_path_to_error::deserialize(deserializer)
        .map(|Object(value)| value)
        .map_err(|error| FormatError::from_serde(&error))
}

/// Record a problem at `path` unless `value` is finite and satisfies `rule`'s `holds`.
pub(crate) fn check(problems: &mut Vec<Problem>, path: &str, value: f64, rule: &str, holds: fn(f64) -> bool) {
    if !value.is_finite() {
        push(problems, path, format!("must be finite (got {})", show(value)));
    } else if !holds(value) {
        push(problems, path, format!("{rule} (got {})", show(value)));
    }
}

pub(crate) fn push(problems: &mut Vec<Problem>, path: &str, rule: impl Into<String>) {
    problems.push(Problem {
        path: path.to_owned(),
        rule: rule.into(),
    });
}

/// The problem for a field the data lacks.
pub(crate) fn required(problems: &mut Vec<Problem>, path: &str) {
    push(problems, path, "is required");
}
