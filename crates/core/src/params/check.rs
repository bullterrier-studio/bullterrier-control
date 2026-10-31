//! The rules of a parameter table: a declaration's (once, as the table is built) and a value's (on every parse).

use serde_json::Value;

use super::{Kind, Param, ParamUnit};
use crate::formats::{Problem, Problems, show};

/// Names a parameter must not have, with what to say instead: the same idea drifts into many names otherwise.
const DISCOURAGED: [(&str, &str); 5] = [
    ("strength", "depth or amount"),
    ("intensity", "depth or amount"),
    ("factor", "ratio, depth or amount"),
    ("val", "value"),
    ("k", "a name that says what it is"),
];

/// How many characters of an offending value a message shows.
const SHOWN: usize = 60;

/// Record every rule `params` break, each under its field's name; leave each valid default as it reads (a decimal
/// declared `1` is `1.0`).
pub(super) fn declarations(params: &mut [Param], problems: &mut Problems) {
    for index in 0..params.len() {
        let repeated = params[..index].iter().any(|earlier| earlier.name == params[index].name);
        let param = &mut params[index];
        let mut broken = |rule: String| {
            problems.push(Problem {
                path: param.name.clone(),
                rule,
            });
        };
        if repeated {
            broken("the name is declared more than once".to_owned());
        }
        if let Some((_, instead)) = DISCOURAGED.iter().find(|(name, _)| *name == param.name) {
            broken(format!("do not name a parameter {:?}; use {instead}", param.name));
        }
        if ParamUnit::ALL.iter().any(|unit| unit.as_str() == param.name) {
            broken(format!(
                "do not name a parameter after its unit ({:?}); say what it is",
                param.name
            ));
        }
        if param.doc.trim().is_empty() {
            broken("needs a description".to_owned());
        }
        let is_number = matches!(param.kind, Kind::Float | Kind::Whole);
        if is_number {
            if param.unit.is_none() {
                broken(format!("a number needs a unit, one of {}", ParamUnit::listed()));
            }
            match (param.min, param.max) {
                (Some(min), Some(max)) if min.is_finite() && max.is_finite() => {}
                (Some(_), Some(_)) => broken(format!(
                    "min and max must be finite numbers (got min {}, max {})",
                    end(param.min),
                    end(param.max)
                )),
                _ => broken(format!(
                    "a number needs both min and max (got min {}, max {})",
                    end(param.min),
                    end(param.max)
                )),
            }
        } else if param.unit.is_some() || param.min.is_some() || param.max.is_some() {
            broken("a field that is not a number takes no unit, min or max".to_owned());
        }
        if param.cyclic && !cyclic_declared(param) {
            broken(format!(
                "cyclic is for a decimal in cycles whose range is exactly one cycle (max = min + 1); got unit {}, \
                 min {}, max {}",
                param.unit.map_or("none", ParamUnit::as_str),
                end(param.min),
                end(param.max)
            ));
        }
        if param.default.is_null() {
            broken("needs a default".to_owned());
            continue;
        }
        match typed(&param.kind, &param.default) {
            Err(rule) => broken(format!("the default is not valid: {rule}")),
            Ok(default) => match (param.min, param.max) {
                (Some(min), Some(max))
                    if is_number && min.is_finite() && max.is_finite() && !in_range(&default, min, max) =>
                {
                    broken(format!(
                        "the default {} must lie in the range {} to {}",
                        shown(&param.default),
                        bound(&param.kind, min),
                        bound(&param.kind, max)
                    ));
                }
                _ => param.default = default,
            },
        }
    }
}

/// Whether a cyclic field is declared as one must be: a decimal in cycles, its range one cycle wide.
#[expect(
    clippy::float_cmp,
    reason = "one cycle exactly, as Python's `max == min + 1` checked"
)]
fn cyclic_declared(param: &Param) -> bool {
    matches!(param.kind, Kind::Float)
        && param.unit == Some(ParamUnit::Cycles)
        && matches!((param.min, param.max), (Some(min), Some(max)) if max == min + 1.0)
}

/// `given` as the field `param` holds it (its type read strictly, a cyclic number wrapped, a number in its range), or
/// the rule it breaks. `param` is a declaration the table accepted.
pub(super) fn value(param: &Param, given: &Value) -> Result<Value, String> {
    let (Some(min), Some(max)) = (param.min, param.max) else {
        return typed(&param.kind, given);
    };
    // only a decimal is cyclic (the table checked): any JSON number is one
    let read = match given.as_f64() {
        Some(number) if param.cyclic => Value::from(wrapped(number, given.is_f64(), min, max)),
        _ => typed(&param.kind, given)?,
    };
    if in_range(&read, min, max) {
        Ok(read)
    } else if in_range(&read, min, f64::INFINITY) {
        Err(format!("must be <= {} (got {})", bound(&param.kind, max), shown(given)))
    } else {
        Err(format!("must be >= {} (got {})", bound(&param.kind, min), shown(given)))
    }
}

/// `given` as a value of `kind`, its type read strictly (a decimal as a JSON decimal), or the rule it breaks.
fn typed(kind: &Kind, given: &Value) -> Result<Value, String> {
    let read = match (kind, given) {
        (Kind::Float, Value::Number(number)) => number.as_f64().map(Value::from),
        (Kind::Whole, Value::Number(number)) if number.is_i64() => Some(given.clone()),
        (Kind::Whole, Value::Number(number)) if number.is_u64() => {
            return Err(format!("must be a whole number within 64 bits (got {})", shown(given)));
        }
        (Kind::Switch, Value::Bool(_)) | (Kind::Text, Value::String(_)) => Some(given.clone()),
        (Kind::Choice(names), Value::String(name)) if names.contains(name) => Some(given.clone()),
        _ => None,
    };
    read.ok_or_else(|| {
        let rule = match kind {
            Kind::Float => "must be a number".to_owned(),
            Kind::Whole => "must be a whole number".to_owned(),
            Kind::Switch => "must be true or false".to_owned(),
            Kind::Text => "must be text".to_owned(),
            Kind::Choice(names) => {
                let names: Vec<String> = names.iter().map(|name| format!("{name:?}")).collect();
                format!("must be one of {}", names.join(", "))
            }
        };
        format!("{rule} (got {})", shown(given))
    })
}

/// `number` given for a cyclic field, in its range `min..=max` (one cycle): kept if it is inside, either end
/// included; else the same position inside. An integer (not `decimal`) is a whole number of cycles from 0 however
/// large, so its position is `min`'s distance to the next whole number, with no arithmetic on the integer (its f64 is
/// rounded beyond 2^53, which only matters outside the range: a range one cycle wide lies well within 2^53).
fn wrapped(number: f64, decimal: bool, min: f64, max: f64) -> f64 {
    if min <= number && number <= max {
        number
    } else if decimal {
        min + (number - min).rem_euclid(1.0)
    } else {
        min + (-min).rem_euclid(1.0)
    }
}

/// Whether a number read for a field lies in `min..=max`; a whole number is compared exactly, however large.
fn in_range(value: &Value, min: f64, max: f64) -> bool {
    match value.as_i64() {
        Some(whole) => {
            // an integer is >= min exactly when it is >= ceil(min), a whole f64 that converts to i128 exactly (or
            // saturates beyond it, where no i64 lies)
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a whole f64 converts exactly or saturates"
            )]
            let (low, high) = (min.ceil() as i128, max.floor() as i128);
            (low..=high).contains(&i128::from(whole))
        }
        None => value.as_f64().is_some_and(|decimal| min <= decimal && decimal <= max),
    }
}

/// A bound of `kind`, for a message: an integer for a whole number, else a decimal (`0.0`).
fn bound(kind: &Kind, value: f64) -> String {
    match whole(kind, value) {
        Some(whole) => whole.to_string(),
        None => show(value),
    }
}

/// A bound of `kind` as JSON: an integer for a whole number, else a decimal.
pub(super) fn bound_json(kind: &Kind, value: f64) -> Value {
    whole(kind, value).map_or_else(|| Value::from(value), Value::from)
}

/// `value` as the integer it is, if `kind` is a whole number and it is one within 64 bits.
fn whole(kind: &Kind, value: f64) -> Option<i64> {
    // -2^63 and 2^63: the range of an i64, both exact in an f64
    const LOWEST: f64 = -9_223_372_036_854_775_808.0;
    let fits = matches!(kind, Kind::Whole) && value.fract() == 0.0 && (LOWEST..-LOWEST).contains(&value);
    #[expect(clippy::cast_possible_truncation, reason = "a whole number within i64")]
    fits.then_some(value as i64)
}

/// An end of a range as a message shows it, `none` if it is missing.
fn end(value: Option<f64>) -> String {
    value.map_or_else(|| "none".to_owned(), show)
}

/// `value` as JSON, for a message, cut short after 60 characters.
pub(super) fn shown(value: &Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(SHOWN) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}
