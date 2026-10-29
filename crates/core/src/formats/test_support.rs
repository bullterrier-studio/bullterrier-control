//! Test helpers shared by the formats' tests.

use serde::de::value::{Error, MapDeserializer};
use serde::de::{Deserializer, IntoDeserializer, Visitor};
use serde::forward_to_deserialize_any;

use super::error::{FormatError, Problem};

/// The problems wanted: each a path and a short part of its rule.
pub(crate) type Expected = &'static [(&'static str, &'static str)];

/// Assert that `result` is accepted when `expected` is empty, else refused with exactly those paths, in that order,
/// each rule containing the given part.
pub(crate) fn check_against<T: std::fmt::Debug>(id: &str, result: Result<T, FormatError>, expected: Expected) {
    match result {
        Ok(value) => assert!(expected.is_empty(), "{id}: accepted {value:?}, wanted {expected:?}"),
        Err(error) => {
            assert!(!expected.is_empty(), "{id}: refused with {error}");
            check_problems(id, &error.problems, expected);
        }
    }
}

/// Assert that `problems` have exactly the paths of `expected`, in that order, each rule containing the given part.
pub(crate) fn check_problems(id: &str, problems: &[Problem], expected: Expected) {
    let paths: Vec<&str> = problems.iter().map(|problem| problem.path.as_str()).collect();
    let wanted: Vec<&str> = expected.iter().map(|(path, _)| *path).collect();
    assert_eq!(paths, wanted, "{id}: {problems:?}");
    for (problem, (_, part)) in problems.iter().zip(expected) {
        assert!(
            problem.rule.contains(part),
            "{id}: {}: {:?} lacks {part:?}",
            problem.path,
            problem.rule
        );
    }
}

/// A number as a non-JSON format delivers it (JSON text cannot hold NaN or an infinity, a Python float can).
struct Decimal(f64);

impl<'de> Deserializer<'de> for Decimal {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_f64(self.0)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit unit_struct
        newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any
    }
}

impl IntoDeserializer<'_, Error> for Decimal {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

/// An object whose fields are all `f64`s, delivered the way a Python float is: NaN and infinities arrive as numbers.
pub(crate) fn object_of_floats<'de>(fields: &[(&'static str, f64)]) -> impl Deserializer<'de, Error = Error> {
    MapDeserializer::new(fields.iter().copied().map(|(name, value)| (name, Decimal(value))))
}
