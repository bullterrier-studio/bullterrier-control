//! Reading a list item by item, so that a broken item is one problem and the items after it are still read, and
//! placing a format's problems under the input's path.

use serde::{Deserialize, Deserializer};

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

/// `data` read as a list of `T`, each item read or refused on its own; the problem that stopped the reading when
/// `data` is no list.
pub(super) fn items<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    data: D,
) -> Result<Vec<Result<T, FormatError>>, FormatError> {
    serde_path_to_error::deserialize::<_, Vec<Item<T>>>(data)
        .map(|items| items.into_iter().map(|Item(item)| item).collect())
        .map_err(|error| FormatError::from_serde(&error))
}

/// Record the problems of `error`, kept or counted, each with its path (`t`, `beats[1]`, empty for the whole) under
/// `prefix` (`events[2]`, `grid`).
pub(super) fn absorb_under(problems: &mut Problems, prefix: &str, mut error: FormatError) {
    for problem in &mut error.problems {
        problem.path = match problem.path.as_str() {
            "" => prefix.to_owned(),
            path if path.starts_with('[') => format!("{prefix}{path}"),
            path => format!("{prefix}.{path}"),
        };
    }
    problems.absorb(&error);
}
