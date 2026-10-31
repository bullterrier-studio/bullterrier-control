//! Values as a patch's messages show them: as Python's `repr` writes them (the words are the Python package's), cut
//! short past [`SHOWN`] characters so a hostile value (a million-character name) keeps a message small.

use std::fmt::Write as _;

use serde_json::Value;

/// How many characters of a value a message shows; a longer one ends in `...`.
const SHOWN: usize = 80;

/// `text` as Python's `repr` writes a string: in single quotes, or double ones when it holds a single quote and no
/// double one; a backslash, the quote and the control characters escaped.
pub(super) fn quoted(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut written = String::from(quote);
    for (index, character) in text.chars().enumerate() {
        if index == SHOWN {
            written.push_str("...");
            break;
        }
        match character {
            '\\' => written.push_str("\\\\"),
            '\t' => written.push_str("\\t"),
            '\n' => written.push_str("\\n"),
            '\r' => written.push_str("\\r"),
            _ if character == quote => {
                written.push('\\');
                written.push(character);
            }
            _ if character.is_control() => {
                let _ = write!(written, "\\x{:02x}", u32::from(character));
            }
            _ => written.push(character),
        }
    }
    written.push(quote);
    written
}

/// `value` as Python's `repr` writes what it decodes to (`None`, `True`, `3`, `'text'`, `[1, 2]`, `{'a': 1}`), cut
/// short past [`SHOWN`] characters.
pub(super) fn shown(value: &Value) -> String {
    cut(&python(value))
}

/// `text`, cut short past [`SHOWN`] characters (ending in `...`).
pub(super) fn cut(text: &str) -> String {
    match text.char_indices().nth(SHOWN) {
        Some((end, _)) => format!("{}...", &text[..end]),
        None => text.to_owned(),
    }
}

/// `names` joined by `, ` (`none` when there are none), [`cut`] short; only the characters shown are read, so a
/// message about a patch of a million nodes costs no more than one about a patch of ten.
pub(super) fn listed<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    let mut shown = String::new();
    let mut count = 0;
    for (index, name) in names.into_iter().enumerate() {
        let gap = if index == 0 { "" } else { ", " };
        for character in gap.chars().chain(name.chars()) {
            if count == SHOWN {
                shown.push_str("...");
                return shown;
            }
            shown.push(character);
            count += 1;
        }
    }
    if shown.is_empty() { "none".to_owned() } else { shown }
}

/// `value` as Python's `repr` writes it, in full but for its strings (each [`quoted`]).
fn python(value: &Value) -> String {
    match value {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => quoted(text),
        Value::Array(items) => format!("[{}]", items.iter().map(python).collect::<Vec<_>>().join(", ")),
        Value::Object(fields) => {
            let fields: Vec<String> = fields
                .iter()
                .map(|(key, value)| format!("{}: {}", quoted(key), python(value)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
    }
}
