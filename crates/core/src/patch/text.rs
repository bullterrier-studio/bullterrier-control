//! A patch's TOML text decoded into the entries [`Patch::parse`](super::Patch::parse) reads, in file order.

use serde_json::{Map, Number, Value};
use toml::de::{DeTable, DeValue};

use super::PatchError;
use super::repr::cut;
use crate::formats::Problem;

/// A TOML text decoded: its top-level keys and values in file order, and a problem for each value TOML holds and a
/// patch cannot (each such value is `null` in `entries`, so it is reported once, here).
pub(super) struct Decoded {
    /// The keys and values of the top-level table, in the order the text first names each.
    pub(super) entries: Vec<(String, Value)>,
    /// A date or time, a number that is not finite, or a whole number beyond 64 bits, each at its path.
    pub(super) spoiled: Vec<Problem>,
}

/// `text` decoded.
///
/// # Errors
/// [`PatchError::Toml`] if `text` is not TOML, with the parser's words after the line and column it stopped at.
pub(super) fn decode(text: &str) -> Result<Decoded, PatchError> {
    let table = DeTable::parse(text).map_err(|error| {
        let detail = match error.span() {
            Some(span) => {
                let (line, column) = position(text, span.start);
                format!("line {line}, column {column}: {}", error.message())
            }
            None => error.message().to_owned(),
        };
        PatchError::Toml { detail }
    })?;
    let mut keys: Vec<_> = table.get_ref().iter().collect();
    // the table is sorted by key; the file's order is where each key is first named. A table's key holds the place of
    // its own `[name]` header, which can come after a header below it (`[name.grid]`) has named it: the earliest key
    // within it is that place.
    keys.sort_by_cached_key(|(key, value)| key.span().start.min(first_named(value.get_ref())));
    let mut spoiled = Vec::new();
    let entries = keys
        .into_iter()
        .map(|(key, value)| {
            let key = key.get_ref().to_string();
            let value = json(&key, value.get_ref(), &mut spoiled);
            (key, value)
        })
        .collect();
    Ok(Decoded { entries, spoiled })
}

/// The earliest place (a byte offset) of a key within `value`, a table at any depth (`[a.b.c]` names `a` and `b`
/// before `[a.b]` and `[a]` do), or `usize::MAX` if it holds none. An array's key is named before its items.
fn first_named(value: &DeValue<'_>) -> usize {
    match value {
        DeValue::Table(table) => table
            .iter()
            .map(|(key, value)| key.span().start.min(first_named(value.get_ref())))
            .min()
            .unwrap_or(usize::MAX),
        _ => usize::MAX,
    }
}

/// The line and column (each from 1, the column in characters) of the byte `offset` of `text`.
fn position(text: &str, offset: usize) -> (usize, usize) {
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |newline| newline + 1);
    (line, before[start..].chars().count() + 1)
}

/// `value`, written at `path`, as JSON; a value JSON cannot hold is `null` and a problem in `spoiled`.
fn json(path: &str, value: &DeValue<'_>, spoiled: &mut Vec<Problem>) -> Value {
    let rule = match value {
        DeValue::String(text) => return Value::String(text.to_string()),
        DeValue::Boolean(switch) => return Value::Bool(*switch),
        DeValue::Integer(whole) => match i64::from_str_radix(whole.as_str(), whole.radix()) {
            Ok(whole) => return Value::from(whole),
            Err(_) => format!(
                "must be a whole number within 64 bits (got {})",
                cut(&whole.to_string())
            ),
        },
        DeValue::Float(decimal) => match decimal.as_str().parse::<f64>().ok().and_then(Number::from_f64) {
            Some(number) => return Value::Number(number),
            None => format!("must be finite (got {decimal})"),
        },
        DeValue::Datetime(moment) => format!("is a date or time ({moment}), which no input or parameter takes"),
        DeValue::Array(items) => {
            return Value::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| json(&format!("{path}[{index}]"), item.get_ref(), spoiled))
                    .collect(),
            );
        }
        DeValue::Table(table) => {
            return Value::Object(
                table
                    .iter()
                    .map(|(key, value)| {
                        let key = key.get_ref().to_string();
                        let value = json(&format!("{path}.{key}"), value.get_ref(), spoiled);
                        (key, value)
                    })
                    .collect::<Map<_, _>>(),
            );
        }
    };
    spoiled.push(Problem {
        path: path.to_owned(),
        rule,
    });
    Value::Null
}
