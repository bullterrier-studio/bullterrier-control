//! What the formats share: how a number is read from JSON and shown in a message, how a struct is read from a JSON
//! object and a failure of that named by its path, and how a field is checked against a rule and a broken rule
//! recorded under a path prefix (so a format nested in a document reports `events.kick[3].t`, not `t`).

use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserialize, Deserializer, Visitor};

use super::error::{FormatError, Problem};

/// The largest whole number a JSON number can carry into an `f64` without rounding (2^53): beyond it, two different
/// integers in a file would read as one value. Only integers `serde_json` reads as 64-bit are held to it: one beyond
/// `i64`/`u64` arrives as an already rounded `f64`, like a decimal, and is accepted (`arbitrary_precision` would
/// close that gap, at a cost in every number read).
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

    // NaN and the infinities (a Python float can be either) are accepted here and refused by `check`, the one place
    // that does, so they are listed beside the other broken rules instead of stopping the reading.
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Number, E> {
        Ok(Number(value))
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

impl JsonSchema for Number {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("number")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type": "number"})
    }
}

/// A JSON integer read strictly: a whole number written without a fraction or exponent (`4`, never `4.0`, `true` or
/// `"4"`). Any 64-bit integer is held exactly; its sign and size are rules for the field that reads it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Whole(pub(crate) i128);

impl<'de> Deserialize<'de> for Whole {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(WholeVisitor)
    }
}

struct WholeVisitor;

impl Visitor<'_> for WholeVisitor {
    type Value = Whole;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a whole number")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Whole, E> {
        Ok(Whole(value.into()))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Whole, E> {
        Ok(Whole(value.into()))
    }

    // a decimal (`4.0`, `1e2`) or an integer too large for 64 bits, which `serde_json` has already rounded
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Whole, E> {
        Err(E::custom(format!("must be a whole number (got {})", show(value))))
    }
}

impl JsonSchema for Whole {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("whole")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type": "integer"})
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

/// The path of `field` under `prefix` (`events.kick[3]` + `t`), or `field` alone when there is no prefix.
fn path(prefix: &str, field: impl fmt::Display) -> String {
    if prefix.is_empty() {
        field.to_string()
    } else {
        format!("{prefix}.{field}")
    }
}

/// Record that `field` (under `prefix`) breaks `rule`.
pub(crate) fn push(problems: &mut Vec<Problem>, prefix: &str, field: impl fmt::Display, rule: impl Into<String>) {
    problems.push(Problem {
        path: path(prefix, field),
        rule: rule.into(),
    });
}

/// Record a problem unless `value` is finite; whether it was.
pub(crate) fn finite(problems: &mut Vec<Problem>, prefix: &str, field: impl fmt::Display, value: f64) -> bool {
    let is_finite = value.is_finite();
    if !is_finite {
        push(problems, prefix, field, format!("must be finite (got {})", show(value)));
    }
    is_finite
}

/// Record a problem unless `value` is finite and satisfies `holds`, which `rule` states (`"must be >= 0"`).
pub(crate) fn check(
    problems: &mut Vec<Problem>,
    prefix: &str,
    field: impl fmt::Display + Copy,
    value: f64,
    rule: &str,
    holds: fn(f64) -> bool,
) {
    // the field is only turned into text when a problem is recorded
    if finite(problems, prefix, field, value) && !holds(value) {
        push(problems, prefix, field, format!("{rule} (got {})", show(value)));
    }
}

/// The problem for a field the data lacks.
pub(crate) fn required(problems: &mut Vec<Problem>, prefix: &str, field: impl fmt::Display) {
    push(problems, prefix, field, "is required");
}
