//! Values written as Python's `repr` writes them: the words of `help()` and of the errors are the Python package's.

use std::fmt::Write as _;

use serde_json::Value;

/// `text` as Python's `repr` writes a string: in single quotes, or double ones when it holds a single quote and no
/// double one; a backslash, the quote, a tab, a newline, a carriage return and the other control characters of ASCII
/// and Latin-1 escaped (`\x07`). (Python also escapes the few other characters Unicode counts as not printable, which
/// no name or default holds.)
pub(crate) fn quoted(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut written = String::with_capacity(text.len() + 2);
    written.push(quote);
    for character in text.chars() {
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
                // a control character of ASCII or Latin-1: at most 0x9f, two hex digits
                let _ = write!(written, "\\x{:02x}", u32::from(character));
            }
            _ => written.push(character),
        }
    }
    written.push(quote);
    written
}

/// A parameter's default as Python's `repr` writes the value it reads as: a decimal as a float (`0.1`, `1000.0`,
/// `1e+16`), a whole number as an int, text quoted ([`quoted`]), a switch `True` or `False`.
pub(crate) fn python(value: &Value) -> String {
    match value {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::String(text) => quoted(text),
        Value::Number(number) => match number.as_f64() {
            Some(decimal) if number.is_f64() => float(decimal),
            _ => number.to_string(),
        },
        // no default is a list or an object
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

/// `value` as Python's `repr` writes a float: the shortest digits that read back as `value` (Rust's `{:e}` and Python
/// both give them), in positional notation with at least one decimal (`1000.0`, `0.0001`) when the decimal exponent is
/// from -4 to 15, else in scientific notation with a signed exponent of at least two digits (`1e+16`, `1.5e-07`).
fn float(value: f64) -> String {
    let scientific = format!("{value:e}");
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return scientific; // `inf` or `NaN`, which a JSON value cannot hold
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return scientific;
    };
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(magnitude) => ("-", magnitude),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|character| *character != '.').collect();
    let (first, rest) = digits.split_at(1);
    if !(-4..16).contains(&exponent) {
        let point = if rest.is_empty() { "" } else { "." };
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        return format!(
            "{sign}{first}{point}{rest}e{exponent_sign}{:02}",
            exponent.unsigned_abs()
        );
    }
    let whole = usize::try_from(exponent + 1).unwrap_or(0); // digits before the point; 0 for a value below 1
    if whole >= digits.len() {
        format!("{sign}{digits}{}.0", "0".repeat(whole - digits.len()))
    } else if whole > 0 {
        let (before, after) = digits.split_at(whole);
        format!("{sign}{before}.{after}")
    } else {
        let zeros = usize::try_from(-exponent - 1).unwrap_or(0);
        format!("{sign}0.{}{digits}", "0".repeat(zeros))
    }
}
