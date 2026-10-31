//! Reading a list item by item, so that a broken item is one problem and the items after it are still read, and
//! placing a format's problems under the input's path.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{DeserializeSeed, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_path_to_error::Track;

use crate::formats::{FormatError, Problems};

/// A list item read on its own: what stopped its reading is its problem, and the list goes on to the next item. (A
/// reader over a value in memory can; one streaming JSON text could not find the next item.)
struct Item<T>(Result<T, FormatError>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Item<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(
            serde_path_to_error::deserialize(deserializer).map_err(|error| FormatError::from_serde(&error)),
        ))
    }
}

/// `data` read as a list of `T`, each item (read, or the problem that stopped its reading) handed to `each` as soon
/// as it is read, so that nothing of a list is held but what `each` keeps; the problem that stopped the reading when
/// `data` is no list.
pub(super) fn items<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    data: D,
    each: impl FnMut(Result<T, FormatError>),
) -> Result<(), FormatError> {
    let mut track = Track::new();
    Each(each, PhantomData)
        .deserialize(serde_path_to_error::Deserializer::new(data, &mut track))
        .map_err(|error| FormatError::from_serde(&serde_path_to_error::Error::new(track.path(), error)))
}

/// The reading of a list that hands each item to the function it holds.
struct Each<T, F>(F, PhantomData<fn() -> T>);

impl<'de, T: Deserialize<'de>, F: FnMut(Result<T, FormatError>)> DeserializeSeed<'de> for Each<T, F> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, T: Deserialize<'de>, F: FnMut(Result<T, FormatError>)> Visitor<'de> for Each<T, F> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a sequence")
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut items: A) -> Result<(), A::Error> {
        while let Some(Item(item)) = items.next_element()? {
            (self.0)(item);
        }
        Ok(())
    }
}

/// Record the problems of `error`, kept or counted, each with its path (a field's, `t` or `beats[1]`, or empty for
/// the whole) under `prefix` (`events[2]`, `grid`).
pub(super) fn absorb_under(problems: &mut Problems, prefix: &str, mut error: FormatError) {
    for problem in &mut error.problems {
        problem.path = if problem.path.is_empty() {
            prefix.to_owned()
        } else {
            format!("{prefix}.{}", problem.path)
        };
    }
    problems.absorb(&error);
}
