//! A patch's TOML text: decoded into the entries [`Patch::parse`](super::Patch::parse) reads, in file order, and
//! written from a patch ([`Patch::to_toml`](super::Patch::to_toml)). The one place that knows TOML's syntax.

use std::fmt::Write as _;

use serde_json::{Map, Number, Value};
use toml::de::{DeTable, DeValue};

use super::repr::cut;
use super::{FORMAT, Given, Patch, PatchError, VERSION};
use crate::formats::Problem;
use crate::inputs::Input;

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

/// `patch` as TOML text that decodes and reads back as it, with the registry it holds the things of: the header,
/// then each node in order (`use`, `signal`, its inputs in the order its thing declares them, its parameters in the
/// order its table declares them), a blank line before each. A number is written as what it is (a decimal as a
/// decimal, a whole number as a whole one), each decimal in the shortest form that reads back as the same bits.
pub(super) fn write(patch: &Patch) -> String {
    let mut text = format!("format = {}\nversion = {VERSION}\n", quoted(FORMAT));
    for (name, node) in &patch.nodes {
        let _ = write!(text, "\n[{}]\nuse = {}\n", key(name), quoted(node.uses()));
        if let Some(reference) = &node.signal {
            let _ = writeln!(text, "signal = {}", quoted(&reference.to_string()));
        }
        for (kind, given) in &node.inputs {
            let value = match given {
                Given::Data(input) => data(input),
                Given::Reference(reference) => quoted(&reference.to_string()),
            };
            let _ = writeln!(text, "{} = {value}", key(kind.name()));
        }
        for param in node.thing.table().params() {
            if let Some(value) = node.values.get(&param.name) {
                let _ = writeln!(text, "{} = {}", key(&param.name), json_value(value));
            }
        }
    }
    text
}

/// `input` as a TOML value, in the shape a bundle writes it (`docs/formats.md`); a field that is not set is left out.
fn data(input: &Input) -> String {
    match input {
        Input::Points(points) => list(points.iter().map(|point| {
            table(&[
                ("t", decimal(point.t)),
                ("value", decimal(point.value)),
                ("ease", quoted(&point.ease.to_string())),
            ])
        })),
        Input::Events(events) => list(events.iter().map(|event| {
            let mut fields = vec![("t", decimal(event.t())), ("strength", decimal(event.strength()))];
            fields.extend(event.duration().map(|duration| ("duration", decimal(duration))));
            fields.extend(event.label().map(|label| ("label", quoted(label))));
            fields.extend(event.pitch().map(|pitch| ("pitch", decimal(pitch))));
            table(&fields)
        })),
        Input::Grid(grid) => table(&[
            ("beats", list(grid.beats().iter().copied().map(decimal))),
            ("meter", grid.meter().to_string()),
            ("first_downbeat", grid.first_downbeat().to_string()),
        ]),
        Input::Values(values) => list(values.iter().copied().map(decimal)),
        Input::Series(series) => {
            let mut fields = vec![
                ("start", decimal(series.start())),
                ("rate", decimal(series.rate())),
                ("values", list(series.values().iter().copied().map(decimal))),
                ("unit", quoted(series.unit().as_str())),
            ];
            fields.extend(series.scaling().map(|scaling| ("scaling", quoted(scaling))));
            table(&fields)
        }
    }
}

/// `value` as a TOML value: a parameter's (a number, a switch or text), or anything JSON holds but `null`, which no
/// patch holds (reading refuses it, a table never gives it) and TOML cannot write: it is written as `null`, which is
/// not TOML, so that the text is refused when it is read rather than read as something else.
fn json_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(switch) => switch.to_string(),
        // a whole number's text is exact; a decimal's is written as `decimal` writes it
        Value::Number(number) => match number.as_f64() {
            Some(float) if number.is_f64() => decimal(float),
            _ => number.to_string(),
        },
        Value::String(text) => quoted(text),
        Value::Array(items) => list(items.iter().map(json_value)),
        Value::Object(fields) => {
            let fields: Vec<(&str, String)> = fields
                .iter()
                .map(|(name, value)| (name.as_str(), json_value(value)))
                .collect();
            table(&fields)
        }
    }
}

/// `number` (finite) as a TOML decimal: the shortest text that reads back as the same bits, always with a point or
/// an exponent (`1.0`, `-0.0`, `1e-320`, `1e308`), never as a whole number.
fn decimal(number: f64) -> String {
    format!("{number:?}")
}

/// `items` as a TOML array on one line.
fn list(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(", "))
}

/// `fields` as a TOML inline table on one line.
fn table(fields: &[(&str, String)]) -> String {
    if fields.is_empty() {
        return "{}".to_owned();
    }
    let fields: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{} = {value}", key(name)))
        .collect();
    format!("{{ {} }}", fields.join(", "))
}

/// `name` as a TOML key: bare when it may be (ASCII letters, digits, `_` and `-`), else quoted.
fn key(name: &str) -> String {
    let bare = !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-');
    if bare { name.to_owned() } else { quoted(name) }
}

/// `text` as a TOML basic string: in double quotes, with the quote, the backslash and every control character TOML
/// does not take as it is (all below U+0020 and U+007F) escaped; any other character as it is.
fn quoted(text: &str) -> String {
    let mut written = String::with_capacity(text.len() + 2);
    written.push('"');
    for character in text.chars() {
        match character {
            '"' => written.push_str("\\\""),
            '\\' => written.push_str("\\\\"),
            '\n' => written.push_str("\\n"),
            '\t' => written.push_str("\\t"),
            '\r' => written.push_str("\\r"),
            '\u{8}' => written.push_str("\\b"),
            '\u{c}' => written.push_str("\\f"),
            '\u{0}'..='\u{1f}' | '\u{7f}' => {
                let _ = write!(written, "\\u{:04X}", u32::from(character));
            }
            _ => written.push(character),
        }
    }
    written.push('"');
    written
}
