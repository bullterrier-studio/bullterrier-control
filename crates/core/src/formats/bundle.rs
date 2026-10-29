//! [`Bundle`]: the one document that carries Series, Events and a Grid.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::marker::PhantomData;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::ser::Formatter;

use super::error::{FormatError, Problem};
use super::event::{Event, RawEvent};
use super::field::{Number, Object, Whole, check, decode, push, required, show};
use super::grid::{Grid, RawGrid};
use super::series::{RawSeries, Series};

/// What a bundle file states its `format` as.
const FORMAT: &str = "bullterrier-control/bundle";

/// The one version of the format there is.
const VERSION: i128 = 1;

/// A series or an event list is named by a string of this shape (`bass`, `stem.drums`, `kick`).
const NAME_PATTERN: &str = "^[a-z0-9][a-z0-9_.-]*$";

/// Everything one producer measured of one piece of material: named Series, named Event lists, a Grid.
///
/// Built by [`Bundle::new`] or read by [`Bundle::from_json`], both held to the rules of `docs/formats.md`. A
/// [`FormatError`] from either lists every rule broken, in the order of the document (`duration`, then `grid`, then
/// each series by name, then each event list by name); the rule that ties an item to the rest of the document (an event
/// sorted by `t` and not after the `duration`, a series not starting after it) follows the other rules of that item.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    source: Option<String>,
    duration: Option<f64>,
    grid: Option<Grid>,
    series: BTreeMap<String, Series>,
    events: BTreeMap<String, Vec<Event>>,
}

/// The parts of a [`Bundle`], all optional; end a literal with `..Default::default()`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BundleOptions {
    /// What produced the data (tool, version, input name): free text.
    pub source: Option<String>,
    /// The length of the material in seconds, `> 0`. Every event is at or before it, and every series starts at or
    /// before it.
    pub duration: Option<f64>,
    /// The beats.
    pub grid: Option<Grid>,
    /// The series by name; a name matches `^[a-z0-9][a-z0-9_.-]*$`.
    pub series: BTreeMap<String, Series>,
    /// The events by name (same pattern), each list sorted by `t`, non-decreasing.
    pub events: BTreeMap<String, Vec<Event>>,
}

impl Bundle {
    /// The bundle of `options`.
    ///
    /// # Errors
    /// A [`FormatError`] listing every rule broken: `duration` not above 0 or not finite, a name outside
    /// `^[a-z0-9][a-z0-9_.-]*$`, a series starting after the duration, events not sorted by `t`, and events after the
    /// duration.
    pub fn new(options: BundleOptions) -> Result<Self, FormatError> {
        let mut problems = Vec::new();
        let limit = check_duration(&mut problems, options.duration);
        for (name, series) in &options.series {
            check_name(&mut problems, "series", name);
            check_start(&mut problems, name, series.start(), limit);
        }
        for (name, events) in &options.events {
            check_name(&mut problems, "events", name);
            let mut before = None;
            for (index, event) in events.iter().enumerate() {
                check_event(&mut problems, name, index, Some(event.t()), before, limit);
                before = Some(event.t());
            }
        }
        if problems.is_empty() {
            Ok(Self {
                source: options.source,
                duration: options.duration,
                grid: options.grid,
                series: options.series,
                events: options.events,
            })
        } else {
            Err(FormatError::new(problems))
        }
    }

    /// Read a bundle from JSON text and check it with [`Bundle::new`]'s rules, and these: `format` is
    /// `"bullterrier-control/bundle"` and `version` the integer `1`, the top level is one object, the text ends with
    /// it, and no key is written twice in one object. `NaN` and the infinities are not JSON and are refused.
    ///
    /// # Errors
    /// A [`FormatError`]: the one problem that stopped the reading (not JSON, a wrong type for a field, an unknown or
    /// repeated field or name, a whole number beyond 2^53 and within 64 bits), or, when the document was read, every
    /// rule it breaks.
    pub fn from_json(text: &str) -> Result<Self, FormatError> {
        let mut reader = serde_json::Deserializer::from_str(text);
        let bundle = Self::from_deserializer(&mut reader)?;
        reader.end().map_err(|error| FormatError::at("", error.to_string()))?;
        Ok(bundle)
    }

    /// Read a bundle from any serde format and check it as [`Bundle::from_json`] does.
    ///
    /// # Errors
    /// As [`Bundle::from_json`].
    pub fn from_deserializer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, FormatError> {
        let raw: RawBundle = decode(deserializer)?;
        let mut problems = Vec::new();
        raw.read(&mut problems).ok_or_else(|| FormatError::new(problems))
    }

    /// What produced the data, if said.
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// The length of the material in seconds, if said.
    #[must_use]
    pub const fn duration(&self) -> Option<f64> {
        self.duration
    }

    /// The beats, if there are any.
    #[must_use]
    pub const fn grid(&self) -> Option<&Grid> {
        self.grid.as_ref()
    }

    /// The series by name.
    #[must_use]
    pub const fn series(&self) -> &BTreeMap<String, Series> {
        &self.series
    }

    /// The events by name.
    #[must_use]
    pub const fn events(&self) -> &BTreeMap<String, Vec<Event>> {
        &self.events
    }

    /// The JSON text of the bundle: one object on one line, ending in a newline. Fields come in the order of
    /// `docs/formats.md`, names sorted, and an optional field that is absent is left out. Separators are `", "` and
    /// `": "`. With `ascii_only` every character beyond ASCII is written as `\uXXXX` (a surrogate pair above the
    /// Basic Multilingual Plane), for an output not known to be UTF-8; the text means the same either way.
    #[must_use]
    pub fn to_json(&self, ascii_only: bool) -> String {
        let written = Written {
            format: FORMAT,
            version: 1,
            source: self.source.as_deref(),
            duration: self.duration,
            grid: self.grid.as_ref(),
            series: &self.series,
            events: &self.events,
        };
        let mut out = Vec::new();
        let mut serializer = serde_json::Serializer::with_formatter(&mut out, Spacing { ascii_only });
        // writing to memory fails for nothing here: every key is text and every number was checked finite
        let _ = written.serialize(&mut serializer);
        out.push(b'\n');
        String::from_utf8_lossy(&out).into_owned()
    }
}

/// The JSON Schema of a bundle file, for tools in other languages, generated from the structs the file is read into:
/// it states the shapes, types, required fields, ranges and the name pattern, and, as the format asks, that nothing
/// else is allowed in an object. What a schema cannot state is only checked by [`Bundle::from_json`]: sorted events,
/// the `duration` bounds, strictly increasing beats, `first_downbeat` below `meter` and the number of beats, `scaling`
/// and the range of a `fraction` series, repeated keys, and an integer written as `1.0`.
#[must_use]
pub fn bundle_schema() -> serde_json::Value {
    serde_json::Value::from(schemars::schema_for!(RawBundle))
}

/// The rules of the name of a series or an event list.
fn check_name(problems: &mut Vec<Problem>, section: &str, name: &str) {
    let mut characters = name.chars();
    let is_start = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let is_rest = |c: char| is_start(c) || matches!(c, '_' | '.' | '-');
    if !(characters.next().is_some_and(is_start) && characters.all(is_rest)) {
        let rule = format!("name must match {NAME_PATTERN} (got {})", serde_json::Value::from(name));
        push(problems, section, name, rule);
    }
}

/// The rule of `duration`, `> 0`; the duration other rules measure against: it, if it is a number that holds.
fn check_duration(problems: &mut Vec<Problem>, duration: Option<f64>) -> Option<f64> {
    let duration = duration?;
    let before = problems.len();
    check(problems, "", "duration", duration, "must be > 0", |x| x > 0.0);
    (problems.len() == before).then_some(duration)
}

/// The rule that a series starts at or before the `duration`, if there is one.
fn check_start(problems: &mut Vec<Problem>, name: &str, start: f64, duration: Option<f64>) {
    if let Some(duration) = duration.filter(|duration| start > *duration) {
        let rule = format!("{} is after the duration of {}", show(start), show(duration));
        push(problems, &format!("series.{name}"), "start", rule);
    }
}

/// The rules that tie event `index` of the list `name`, which is at `t` (if it said so, as a number), to the rest of
/// the document: it is not before the event before it (at `before`), and not after the `duration`.
fn check_event(
    problems: &mut Vec<Problem>,
    name: &str,
    index: usize,
    t: Option<f64>,
    before: Option<f64>,
    duration: Option<f64>,
) {
    let Some(t) = t.filter(|t| t.is_finite()) else {
        return;
    };
    let at = format!("events.{name}[{index}]");
    if let Some(before) = before.filter(|before| t < *before) {
        let rule = format!(
            "t = {} comes before t = {} of the event before it: sort by t",
            show(t),
            show(before)
        );
        push(problems, "", &at, rule);
    }
    if let Some(duration) = duration.filter(|duration| t > *duration) {
        let rule = format!("{} is after the duration of {}", show(t), show(duration));
        push(problems, &at, "t", rule);
    }
}

/// The document as a file spells it, before any rule is checked: the one description of its shape that the schema is
/// generated from (the attributes state the rules a schema can).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "Bundle",
    description = "Everything one producer measured of one piece of material: named Series, named Event lists, a Grid."
)]
struct RawBundle {
    #[schemars(schema_with = "format_schema")]
    format: Option<String>,
    #[schemars(schema_with = "version_schema")]
    version: Option<Whole>,
    source: Option<String>,
    #[schemars(extend("exclusiveMinimum" = 0))]
    duration: Option<Number>,
    // each struct is read as an `Object`: a JSON array is no grid, series or event
    grid: Option<Object<RawGrid>>,
    #[serde(default)]
    series: Names<Object<RawSeries>>,
    #[serde(default)]
    events: Names<Vec<Object<RawEvent>>>,
}

fn format_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "const": FORMAT})
}

fn version_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "integer", "const": 1})
}

impl RawBundle {
    /// Check every rule of the document, recording each one broken in `problems`; the bundle if none is.
    fn read(self, problems: &mut Vec<Problem>) -> Option<Bundle> {
        match self.format.as_deref() {
            None => required(problems, "", "format"),
            Some(FORMAT) => {}
            Some(other) => {
                let rule = format!("must be {FORMAT:?} (got {})", serde_json::Value::from(other));
                push(problems, "", "format", rule);
            }
        }
        match self.version {
            None => required(problems, "", "version"),
            Some(Whole(VERSION)) => {}
            Some(Whole(other)) => push(problems, "", "version", format!("must be {VERSION} (got {other})")),
        }
        let duration = self.duration.map(|number| number.0);
        let limit = check_duration(problems, duration);
        let grid = self.grid.map(|Object(grid)| grid.read("grid", problems));
        let mut series = BTreeMap::new();
        for (name, Object(raw)) in self.series.0 {
            check_name(problems, "series", &name);
            let start = raw.start();
            let found = raw.read(&format!("series.{name}"), problems);
            check_start(problems, &name, start, limit);
            if let Some(found) = found {
                series.insert(name, found);
            }
        }
        let mut events = BTreeMap::new();
        for (name, list) in self.events.0 {
            check_name(problems, "events", &name);
            let mut found = Vec::with_capacity(list.len());
            let mut before = None;
            for (index, Object(raw)) in list.into_iter().enumerate() {
                let t = raw.t();
                if let Some(event) = raw.read(&format!("events.{name}[{index}]"), problems) {
                    found.push(event);
                }
                check_event(problems, &name, index, t, before, limit);
                // the next event is compared with the last time that could be read, even past an event without one
                before = t.filter(|t| t.is_finite()).or(before);
            }
            events.insert(name, found);
        }
        problems.is_empty().then(|| Bundle {
            source: self.source,
            duration,
            grid: grid.flatten(),
            series,
            events,
        })
    }
}

/// The items of an object by name, in name order. A name written twice in one object is refused, where a map would
/// keep the last silently.
struct Names<T>(BTreeMap<String, T>);

impl<T> Default for Names<T> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Names<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NamesVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for NamesVisitor<T> {
            type Value = Names<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map from names")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Names<T>, A::Error> {
                let mut found = BTreeMap::new();
                while let Some(name) = map.next_key::<String>()? {
                    if found.contains_key(&name) {
                        // worded as serde words a repeated field, so the problem is named by the key
                        return Err(de::Error::custom(format!("duplicate field `{name}`")));
                    }
                    found.insert(name, map.next_value()?);
                }
                Ok(Names(found))
            }
        }

        deserializer.deserialize_map(NamesVisitor(PhantomData))
    }
}

impl<T: JsonSchema> JsonSchema for Names<T> {
    fn schema_name() -> Cow<'static, str> {
        Cow::Owned(format!("Names<{}>", T::schema_name()))
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let item = generator.subschema_for::<T>();
        json_schema!({
            "type": "object",
            "patternProperties": {NAME_PATTERN: item},
            "additionalProperties": false,
        })
    }
}

/// The document as it is written: the fields of [`RawBundle`], in the order of `docs/formats.md`.
#[derive(Serialize)]
struct Written<'a> {
    format: &'static str,
    version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grid: Option<&'a Grid>,
    series: &'a BTreeMap<String, Series>,
    events: &'a BTreeMap<String, Vec<Event>>,
}

/// How `json.dumps` of Python spaces JSON (`", "` and `": "`), and, with `ascii_only`, escapes text as it does with
/// `ensure_ascii=True`: whatever is not a printable ASCII character becomes `\uXXXX`, in UTF-16 units.
struct Spacing {
    ascii_only: bool,
}

impl Formatter for Spacing {
    fn begin_array_value<W: ?Sized + io::Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()> {
        if first { Ok(()) } else { writer.write_all(b", ") }
    }

    fn begin_object_key<W: ?Sized + io::Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()> {
        if first { Ok(()) } else { writer.write_all(b", ") }
    }

    fn begin_object_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }

    // serde_json escapes the quote, the backslash and the control characters before it calls this with the rest
    fn write_string_fragment<W: ?Sized + io::Write>(&mut self, writer: &mut W, fragment: &str) -> io::Result<()> {
        if !self.ascii_only {
            return writer.write_all(fragment.as_bytes());
        }
        let mut units = [0_u16; 2];
        for character in fragment.chars() {
            if character.is_ascii() && character != '\u{7f}' {
                let mut utf8 = [0_u8; 4];
                writer.write_all(character.encode_utf8(&mut utf8).as_bytes())?;
            } else {
                for unit in character.encode_utf16(&mut units) {
                    write!(writer, "\\u{unit:04x}")?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "the getters give back exactly what was given")]
mod tests {
    use super::super::test_support::{Expected, check_against};
    use super::super::{EventOptions, SeriesOptions, Unit};
    use super::*;
    use proptest::prelude::*;
    use serde_json::{Value, json};

    /// The two fields every bundle states, then `rest`.
    fn doc(rest: &str) -> String {
        let head = r#""format": "bullterrier-control/bundle", "version": 1"#;
        if rest.is_empty() {
            format!("{{{head}}}")
        } else {
            format!("{{{head}, {rest}}}")
        }
    }

    const BASS: &str = r#"{"rate": 10, "values": [0.1, 0.2], "unit": "ratio"}"#;

    #[test]
    fn a_document_is_read_when_it_follows_the_format() {
        #[rustfmt::skip]
        let rows: Vec<(&str, String, Expected)> = vec![
            ("the smallest bundle", doc(""), &[]),
            ("everything the format has", doc(r#""source": "librosa 0.11 on song.wav", "duration": 10,
                "grid": {"beats": [0, 1, 2], "meter": 2, "first_downbeat": 1},
                "series": {"bass": {"rate": 10, "values": [0.1, 0.2], "unit": "ratio"},
                           "stem.drums": {"start": 1.5, "rate": 5, "values": [0.5], "unit": "fraction", "scaling": "p5 to p98"}},
                "events": {"kick": [{"t": 0.5}, {"t": 0.5, "strength": 0.5}, {"t": 10, "duration": 2, "label": "x", "pitch": 60}], "0": []}"#), &[]),
            ("null is an optional left out", doc(r#""source": null, "duration": null, "grid": null"#), &[]),
            ("an empty source is a source", doc(r#""source": """#), &[]),
            ("text beyond ASCII", doc("\"source\": \"ünï 😀 \u{200b}\""), &[]),
            ("names at the edge of the pattern", doc(&format!(r#""series": {{"0": {BASS}, "a-b_c.d": {BASS}, "9z": {BASS}}}"#)), &[]),
            ("a name used for both a series and events", doc(&format!(r#""series": {{"bass": {BASS}}}, "events": {{"bass": [{{"t": 1}}]}}"#)), &[]),
            ("with no duration an event may be anywhere", doc(r#""events": {"kick": [{"t": 1e9}]}"#), &[]),
            ("an event and a series starting exactly at the duration",
                doc(r#""duration": 4, "series": {"bass": {"start": 4, "rate": 1, "values": [1], "unit": "ratio"}}, "events": {"kick": [{"t": 4}]}"#), &[]),
            ("a span and a grid may run past the duration",
                doc(r#""duration": 1, "grid": {"beats": [0, 5], "meter": 1, "first_downbeat": 0}, "events": {"kick": [{"t": 1, "duration": 9}]}"#), &[]),
            ("events at the same time are sorted", doc(r#""events": {"kick": [{"t": 2}, {"t": 2}]}"#), &[]),
            ("names outside the pattern: a capital", doc(&format!(r#""series": {{"Bass": {BASS}}}"#)), &[("series.Bass", "name must match")]),
            ("names outside the pattern: a leading dot", doc(r#""events": {".x": []}"#), &[("events..x", "name must match")]),
            ("names outside the pattern: a leading underscore", doc(r#""events": {"_x": []}"#), &[("events._x", "name must match")]),
            ("names outside the pattern: empty", doc(r#""events": {"": []}"#), &[("events.", "name must match")]),
            ("names outside the pattern: a space", doc(&format!(r#""series": {{"a b": {BASS}}}"#)), &[("series.a b", "name must match")]),
            ("names outside the pattern: a slash", doc(r#""events": {"a/b": []}"#), &[("events.a/b", "name must match")]),
            ("names outside the pattern: a newline at the end", doc(r#""events": {"a\n": []}"#), &[("events.a\n", "name must match")]),
            ("duration: zero", doc(r#""duration": 0"#), &[("duration", "> 0 (got 0.0)")]),
            ("duration: negative", doc(r#""duration": -2.5"#), &[("duration", "> 0 (got -2.5)")]),
            ("duration: tiny and positive", doc(r#""duration": 1e-300"#), &[]),
            ("duration: a string", doc(r#""duration": "10""#), &[("duration", "string")]),
            ("events not sorted", doc(r#""events": {"kick": [{"t": 1}, {"t": 0.5}]}"#),
                &[("events.kick[1]", "t = 0.5 comes before t = 1.0 of the event before it: sort by t")]),
            ("an event after the duration", doc(r#""duration": 2, "events": {"kick": [{"t": 1}, {"t": 2.5}]}"#),
                &[("events.kick[1].t", "2.5 is after the duration of 2.0")]),
            ("a series starting after the duration", doc(r#""duration": 2, "series": {"bass": {"start": 3, "rate": 1, "values": [1], "unit": "ratio"}}"#),
                &[("series.bass.start", "3.0 is after the duration of 2.0")]),
            ("a list that is not a list", doc(r#""events": {"kick": {"t": 1}}"#), &[("events.kick", "sequence")]),
            ("a map that is not a map", doc(r#""series": []"#), &[("series", "map")]),
            ("a null map", doc(r#""events": null"#), &[("events", "null")]),
            ("a broken event is named by its index", doc(r#""events": {"kick": [{"t": 1}, {"t": 2}, {"t": 3}, {"t": -1}]}"#),
                &[("events.kick[3].t", ">= 0 (got -1.0)"), ("events.kick[3]", "comes before")]),
            ("a broken series is named", doc(r#""series": {"bass": {"rate": 1, "values": [0, 2], "unit": "fraction", "scaling": "x"}}"#),
                &[("series.bass.values[1]", "[0, 1] when unit is \"fraction\" (got 2.0)")]),
            ("a broken grid is named", doc(r#""grid": {"beats": [0, 1, 1], "meter": 2, "first_downbeat": 0}"#),
                &[("grid.beats[2]", "does not come after beats[1] = 1.0")]),
            ("a decimal meter in the grid", doc(r#""grid": {"beats": [0, 1], "meter": 2.0, "first_downbeat": 0}"#), &[("grid.meter", "whole number")]),
            ("an unknown field in an event", doc(r#""events": {"kick": [{"t": 1, "volume": 3}]}"#), &[("events.kick[0].volume", "unknown field")]),
            ("an integer beyond 2^53 in a series", doc(r#""series": {"bass": {"rate": 1, "values": [9007199254740993], "unit": "ratio"}}"#),
                &[("series.bass.values[0]", "2^53")]),
            ("an unknown top-level field", doc(r#""extra": 1"#), &[("extra", "unknown field")]),
            ("format: another string", r#"{"format": "other", "version": 1}"#.to_owned(), &[("format", "\"bullterrier-control/bundle\" (got \"other\")")]),
            ("format: left out", r#"{"version": 1}"#.to_owned(), &[("format", "is required")]),
            ("format: a number", r#"{"format": 1, "version": 1}"#.to_owned(), &[("format", "integer")]),
            ("version: left out", r#"{"format": "bullterrier-control/bundle"}"#.to_owned(), &[("version", "is required")]),
            ("version: 2", r#"{"format": "bullterrier-control/bundle", "version": 2}"#.to_owned(), &[("version", "must be 1 (got 2)")]),
            ("version: 0", r#"{"format": "bullterrier-control/bundle", "version": 0}"#.to_owned(), &[("version", "must be 1 (got 0)")]),
            ("version: 1.0", r#"{"format": "bullterrier-control/bundle", "version": 1.0}"#.to_owned(), &[("version", "whole number (got 1.0)")]),
            ("version: \"1\"", r#"{"format": "bullterrier-control/bundle", "version": "1"}"#.to_owned(), &[("version", "string")]),
            ("version: true", r#"{"format": "bullterrier-control/bundle", "version": true}"#.to_owned(), &[("version", "boolean")]),
            ("not an object: an array", "[]".to_owned(), &[("", "an object")]),
            ("not an object: a number", "1".to_owned(), &[("", "an object")]),
            ("not an object: a string", r#""bundle""#.to_owned(), &[("", "an object")]),
            ("not an object: null", "null".to_owned(), &[("", "an object")]),
            ("no text", String::new(), &[("", "EOF")]),
            ("cut short", r#"{"format": "bullterrier-control/bundle", "ver"#.to_owned(), &[("", "EOF")]),
            ("text after the object", format!("{} []", doc("")), &[("", "trailing characters")]),
            ("NaN", doc(r#""duration": NaN"#), &[("duration", "expected value")]),
            ("Infinity", doc(r#""duration": Infinity"#), &[("duration", "expected value")]),
            ("-Infinity", doc(r#""duration": -Infinity"#), &[("duration", "invalid number")]),
            ("a lone surrogate in text", doc(r#""source": "\ud800""#), &[("source", "hex escape")]),
            ("a repeated top-level key", doc(r#""duration": 1, "duration": 2"#), &[("duration", "duplicate field `duration`")]),
            ("a repeated top-level key that holds a map", doc(r#""series": {}, "series": {}"#), &[("series", "duplicate field `series`")]),
            ("a repeated series name", doc(&format!(r#""series": {{"bass": {BASS}, "bass": {BASS}}}"#)), &[("series.bass", "duplicate field `bass`")]),
            ("a repeated events name", doc(r#""events": {"kick": [], "kick": []}"#), &[("events.kick", "duplicate field `kick`")]),
            ("a repeated field of a series", doc(r#""series": {"bass": {"rate": 1, "rate": 2, "values": [1], "unit": "ratio"}}"#),
                &[("series.bass.rate", "duplicate field `rate`")]),
            ("a repeated field of an event", doc(r#""events": {"kick": [{"t": 1}, {"t": 1, "t": 2}]}"#), &[("events.kick[1].t", "duplicate field `t`")]),
            ("a repeated field of the grid", doc(r#""grid": {"beats": [0, 1], "meter": 2, "meter": 3, "first_downbeat": 0}"#), &[("grid.meter", "duplicate field `meter`")]),
            ("a repeated name with a backtick in it is named in full", doc(r#""events": {"a`b": [], "a`b": []}"#), &[("events.a`b", "duplicate field `a`b`")]),
            ("a repeated name inside a series name map with two backticks", doc(r#""series": {"`x`": {"rate": 1, "values": [1], "unit": "ratio"}, "`x`": {}}"#), &[("series.`x`", "duplicate field")]),
            ("a whole number beyond 64 bits is not told it is not whole", r#"{"format": "bullterrier-control/bundle", "version": 18446744073709551616}"#.to_owned(), &[("version", "within 64 bits")]),
            ("a grid written as an array", doc(r#""grid": [[0, 1], 4, 0]"#), &[("grid", "an object")]),
            ("a series written as an array", doc(r#""series": {"bass": [0, 10, [0.1], "ratio", null]}"#), &[("series.bass", "an object")]),
            ("an event written as an array", doc(r#""events": {"kick": [[1, null, null, null, null]]}"#), &[("events.kick[0]", "an object")]),
            ("an event written as a short array", doc(r#""events": {"kick": [{"t": 1}, [1]]}"#), &[("events.kick[1]", "an object")]),
            ("a time left out does not hide the order of the others", doc(r#""events": {"kick": [{"t": 5}, {"strength": 1}, {"t": 1}]}"#),
                &[("events.kick[1].t", "is required"), ("events.kick[2]", "t = 1.0 comes before t = 5.0")]),
        ];
        for (id, text, expected) in rows {
            let result = Bundle::from_json(&text);
            if let Err(error) = &result {
                assert!(
                    !error.to_string().contains("Raw"),
                    "{id}: names an internal type: {error}"
                );
            }
            check_against(id, result, expected);
        }
    }

    #[test]
    fn every_rule_broken_in_a_document_is_listed_with_its_full_path_in_the_order_of_the_document() {
        let text = r#"{
            "format": "bullterrier-control/bundle", "version": 2, "duration": 5,
            "grid": {"beats": [0, 0], "meter": 4, "first_downbeat": 5},
            "series": {
                "bass": {"start": 6, "rate": 0, "values": [1], "unit": "ratio"},
                "Bad Name": {"rate": 1, "values": [1], "unit": "ratio"}
            },
            "events": {
                "kick": [{"t": 7}, {"t": 6, "strength": 2}, {"t": 1}, {"t": 0.5}],
                "snare": [{"t": -1}, {"t": 9}, {"t": 3}]
            }
        }"#;
        check_against(
            "version, grid, series by name, events by name",
            Bundle::from_json(text),
            &[
                ("version", "must be 1 (got 2)"),
                ("grid.beats[1]", "does not come after beats[0] = 0.0"),
                (
                    "grid.first_downbeat",
                    "5 must be a beat inside a bar of meter = 4 (2 beats)",
                ),
                ("series.Bad Name", "name must match"),
                ("series.bass.rate", "> 0 (got 0.0)"),
                ("series.bass.start", "6.0 is after the duration of 5.0"),
                ("events.kick[0].t", "7.0 is after the duration of 5.0"),
                ("events.kick[1].strength", "[0, 1] (got 2.0)"),
                ("events.kick[1]", "t = 6.0 comes before t = 7.0"),
                ("events.kick[1].t", "6.0 is after the duration of 5.0"),
                ("events.kick[2]", "t = 1.0 comes before t = 6.0"),
                ("events.kick[3]", "t = 0.5 comes before t = 1.0"),
                ("events.snare[0].t", ">= 0 (got -1.0)"),
                ("events.snare[1].t", "9.0 is after the duration of 5.0"),
                ("events.snare[2]", "t = 3.0 comes before t = 9.0"),
            ],
        );
    }

    fn series_at(start: f64) -> Result<Series, FormatError> {
        Series::new(
            1.0,
            vec![1.0],
            Unit::Ratio,
            SeriesOptions {
                start,
                ..Default::default()
            },
        )
    }

    fn event_at(t: f64) -> Result<Event, FormatError> {
        Event::new(t, EventOptions::default())
    }

    fn kick_at(times: &[f64]) -> Result<BTreeMap<String, Vec<Event>>, FormatError> {
        let events = times.iter().map(|&t| event_at(t)).collect::<Result<Vec<_>, _>>()?;
        Ok(BTreeMap::from([("kick".to_owned(), events)]))
    }

    fn bass_starting(start: f64) -> Result<BTreeMap<String, Series>, FormatError> {
        Ok(BTreeMap::from([("bass".to_owned(), series_at(start)?)]))
    }

    #[test]
    fn a_bundle_built_in_code_is_held_to_the_same_rules() -> Result<(), FormatError> {
        let named = |name: &str, start: f64| -> Result<BTreeMap<String, Series>, FormatError> {
            Ok(BTreeMap::from([(name.to_owned(), series_at(start)?)]))
        };
        #[rustfmt::skip]
        let rows: Vec<(&str, BundleOptions, Expected)> = vec![
            ("nothing", BundleOptions::default(), &[]),
            ("all inside the duration", BundleOptions { duration: Some(2.0), series: bass_starting(2.0)?, events: kick_at(&[0.0, 2.0])?, ..Default::default() }, &[]),
            ("a bad name", BundleOptions { series: named("Bass", 0.0)?, ..Default::default() }, &[("series.Bass", "name must match")]),
            ("duration zero", BundleOptions { duration: Some(0.0), ..Default::default() }, &[("duration", "> 0 (got 0.0)")]),
            ("duration NaN", BundleOptions { duration: Some(f64::NAN), series: bass_starting(5.0)?, ..Default::default() }, &[("duration", "finite")]),
            ("unsorted events", BundleOptions { events: kick_at(&[1.0, 0.5])?, ..Default::default() }, &[("events.kick[1]", "comes before")]),
            ("late events and a late series, all listed",
                BundleOptions { duration: Some(1.0), series: bass_starting(1.5)?, events: kick_at(&[0.5, 1.5, 2.0])?, ..Default::default() },
                &[("series.bass.start", "after the duration"), ("events.kick[1].t", "after"), ("events.kick[2].t", "after")]),
        ];
        for (id, options, expected) in rows {
            check_against(id, Bundle::new(options), expected);
        }
        Ok(())
    }

    #[test]
    fn a_bundle_keeps_what_it_was_given() -> Result<(), FormatError> {
        let grid = Grid::new(vec![0.0, 1.0], 1, 0)?;
        let bundle = Bundle::new(BundleOptions {
            source: Some("tool 1.0".to_owned()),
            duration: Some(3.0),
            grid: Some(grid.clone()),
            series: bass_starting(0.5)?,
            events: kick_at(&[1.0, 2.0])?,
        })?;
        assert_eq!(
            (bundle.source(), bundle.duration(), bundle.grid()),
            (Some("tool 1.0"), Some(3.0), Some(&grid))
        );
        assert_eq!(bundle.series().keys().collect::<Vec<_>>(), ["bass"]);
        assert_eq!(bundle.series().get("bass").map(Series::start), Some(0.5));
        assert_eq!(
            bundle
                .events()
                .get("kick")
                .map(|events| events.iter().map(Event::t).collect::<Vec<_>>()),
            Some(vec![1.0, 2.0])
        );
        let empty = Bundle::new(BundleOptions::default())?;
        assert_eq!((empty.source(), empty.duration(), empty.grid()), (None, None, None));
        assert!(empty.series().is_empty() && empty.events().is_empty());
        Ok(())
    }

    #[test]
    fn json_is_written_in_the_order_of_the_format_leaving_out_what_is_absent() -> Result<(), FormatError> {
        let empty = Bundle::new(BundleOptions::default())?;
        assert_eq!(
            empty.to_json(false),
            "{\"format\": \"bullterrier-control/bundle\", \"version\": 1, \"series\": {}, \"events\": {}}\n"
        );
        let bundle = Bundle::new(BundleOptions {
            duration: Some(4.0),
            grid: Some(Grid::new(vec![0.0, 0.5], 2, 1)?),
            series: BTreeMap::from([("b".to_owned(), series_at(0.0)?), ("a".to_owned(), series_at(1.5)?)]),
            events: BTreeMap::from([(
                "kick".to_owned(),
                vec![
                    event_at(1.0)?,
                    Event::new(
                        2.0,
                        EventOptions {
                            strength: 0.5,
                            duration: Some(1.0),
                            label: Some("x".to_owned()),
                            pitch: Some(60.0),
                        },
                    )?,
                ],
            )]),
            ..Default::default()
        })?;
        // worked by hand from the format: top-level fields in table order, names sorted, optional fields only when set
        assert_eq!(
            bundle.to_json(false),
            concat!(
                r#"{"format": "bullterrier-control/bundle", "version": 1, "duration": 4.0, "#,
                r#""grid": {"beats": [0.0, 0.5], "meter": 2, "first_downbeat": 1}, "#,
                r#""series": {"a": {"start": 1.5, "rate": 1.0, "values": [1.0], "unit": "ratio"}, "#,
                r#""b": {"start": 0.0, "rate": 1.0, "values": [1.0], "unit": "ratio"}}, "#,
                r#""events": {"kick": [{"t": 1.0, "strength": 1.0}, "#,
                r#"{"t": 2.0, "strength": 0.5, "duration": 1.0, "label": "x", "pitch": 60.0}]}}"#,
                "\n"
            )
        );
        Ok(())
    }

    #[test]
    fn text_is_escaped_as_python_json_dumps_does_with_and_without_ascii_only() -> Result<(), FormatError> {
        let bundle = Bundle::new(BundleOptions {
            source: Some("é😀\"\\\n\t\u{7f}\u{1} \u{2028}".to_owned()),
            ..Default::default()
        })?;
        // json.dumps(..., ensure_ascii=True) and json.dumps(..., ensure_ascii=False) of the same text
        let head = "{\"format\": \"bullterrier-control/bundle\", \"version\": 1, \"source\": ";
        let tail = ", \"series\": {}, \"events\": {}}\n";
        assert_eq!(
            bundle.to_json(true),
            format!("{head}{}{tail}", r#""\u00e9\ud83d\ude00\"\\\n\t\u007f\u0001 \u2028""#)
        );
        assert_eq!(
            bundle.to_json(false),
            format!("{head}{}{tail}", "\"é😀\\\"\\\\\\n\\t\u{7f}\\u0001 \u{2028}\"")
        );
        Ok(())
    }

    #[test]
    fn floats_are_written_so_that_they_read_back_the_same() -> Result<(), FormatError> {
        let values = vec![
            0.1 + 0.2,
            1.0e-7,
            123_456_789.123_456_79,
            1.0e22,
            -0.0,
            f64::MIN_POSITIVE,
            f64::MAX,
        ];
        let series = Series::new(1.0 / 3.0, values, Unit::Ratio, SeriesOptions::default())?;
        let bundle = Bundle::new(BundleOptions {
            series: BTreeMap::from([("s".to_owned(), series)]),
            ..Default::default()
        })?;
        assert_eq!(Bundle::from_json(&bundle.to_json(false)), Ok(bundle));
        Ok(())
    }

    #[test]
    fn the_schema_states_the_shapes_ranges_and_names_a_schema_can() {
        let schema = bundle_schema();
        let at = |pointer: &str| schema.pointer(pointer).cloned().unwrap_or(Value::Null);
        let required = |pointer: &str| {
            let mut names: Vec<String> = at(pointer)
                .as_array()
                .map(|names| {
                    names
                        .iter()
                        .filter_map(|name| name.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            names
        };
        let keys = |pointer: &str| {
            let mut names: Vec<String> = at(pointer)
                .as_object()
                .map(|map| map.keys().cloned().collect())
                .unwrap_or_default();
            names.sort();
            names
        };
        // the bundle
        assert_eq!(at("/type"), json!("object"));
        assert_eq!(at("/additionalProperties"), json!(false));
        assert_eq!(required("/required"), ["format", "version"]);
        assert_eq!(
            keys("/properties"),
            ["duration", "events", "format", "grid", "series", "source", "version"]
        );
        assert_eq!(at("/properties/format/const"), json!("bullterrier-control/bundle"));
        assert_eq!(at("/properties/version/const"), json!(1));
        assert_eq!(at("/properties/version/type"), json!("integer"));
        assert_eq!(at("/properties/duration/exclusiveMinimum"), json!(0));
        // the name maps: only names of the pattern, nothing else
        for (field, shape) in [
            ("series", json!({"$ref": "#/$defs/Series"})),
            ("events", json!({"type": "array", "items": {"$ref": "#/$defs/Event"}})),
        ] {
            assert_eq!(at(&format!("/properties/{field}/type")), json!("object"), "{field}");
            assert_eq!(
                at(&format!("/properties/{field}/additionalProperties")),
                json!(false),
                "{field}"
            );
            assert_eq!(
                keys(&format!("/properties/{field}/patternProperties")),
                ["^[a-z0-9][a-z0-9_.-]*$"],
                "{field}"
            );
            assert_eq!(
                at(&format!("/properties/{field}/patternProperties/^[a-z0-9][a-z0-9_.-]*$")),
                shape,
                "{field}"
            );
        }
        // an event
        assert_eq!(at("/$defs/Event/additionalProperties"), json!(false));
        assert_eq!(required("/$defs/Event/required"), ["t"]);
        assert_eq!(
            keys("/$defs/Event/properties"),
            ["duration", "label", "pitch", "strength", "t"]
        );
        assert_eq!(at("/$defs/Event/properties/t/minimum"), json!(0));
        assert_eq!(at("/$defs/Event/properties/strength/minimum"), json!(0));
        assert_eq!(at("/$defs/Event/properties/strength/maximum"), json!(1));
        assert_eq!(at("/$defs/Event/properties/duration/exclusiveMinimum"), json!(0));
        assert_eq!(at("/$defs/Event/properties/label/minLength"), json!(1));
        assert_eq!(at("/$defs/Event/properties/pitch/minimum"), json!(0));
        assert_eq!(at("/$defs/Event/properties/pitch/maximum"), json!(127));
        // a series
        assert_eq!(at("/$defs/Series/additionalProperties"), json!(false));
        assert_eq!(required("/$defs/Series/required"), ["rate", "unit", "values"]);
        assert_eq!(
            keys("/$defs/Series/properties"),
            ["rate", "scaling", "start", "unit", "values"]
        );
        assert_eq!(at("/$defs/Series/properties/rate/exclusiveMinimum"), json!(0));
        assert_eq!(at("/$defs/Series/properties/start/minimum"), json!(0));
        assert_eq!(at("/$defs/Series/properties/values/minItems"), json!(1));
        assert_eq!(at("/$defs/Series/properties/values/items/type"), json!("number"));
        assert_eq!(at("/$defs/Series/properties/unit/$ref"), json!("#/$defs/Unit"));
        assert_eq!(
            at("/$defs/Unit/enum"),
            json!(["fraction", "decibels", "hertz", "midi", "ratio", "seconds", "beats"])
        );
        // a grid
        assert_eq!(at("/$defs/Grid/additionalProperties"), json!(false));
        assert_eq!(required("/$defs/Grid/required"), ["beats", "first_downbeat", "meter"]);
        assert_eq!(at("/$defs/Grid/properties/beats/minItems"), json!(2));
        assert_eq!(at("/$defs/Grid/properties/beats/items/minimum"), json!(0));
        assert_eq!(at("/$defs/Grid/properties/meter/type"), json!("integer"));
        assert_eq!(at("/$defs/Grid/properties/meter/minimum"), json!(1));
        assert_eq!(at("/$defs/Grid/properties/first_downbeat/type"), json!("integer"));
        assert_eq!(at("/$defs/Grid/properties/first_downbeat/minimum"), json!(0));
    }

    fn arbitrary_bundles() -> impl Strategy<Value = Bundle> {
        let name = "[a-z0-9][a-z0-9_.-]{0,6}";
        let text = "(?s).{0,12}";
        let series = (
            0.0..=1.0_f64,
            1.0e-3..1.0e6_f64,
            prop::collection::vec(-1.0e12..1.0e12_f64, 1..8),
            text,
        )
            .prop_map(|(start, rate, values, scaling)| {
                Series::new(
                    rate,
                    values,
                    Unit::Decibels,
                    SeriesOptions {
                        start,
                        scaling: Some(scaling),
                    },
                )
                .ok()
            });
        let event = (
            0.0..=1.0_f64,
            0.0..=1.0_f64,
            proptest::option::of(1.0e-3..100.0_f64),
            proptest::option::of(".{1,6}"),
            proptest::option::of(0.0..=127.0_f64),
        );
        let events = prop::collection::vec(event, 0..6).prop_map(|mut found| {
            found.sort_by(|a, b| a.0.total_cmp(&b.0));
            found
                .into_iter()
                .filter_map(|(t, strength, duration, label, pitch)| {
                    Event::new(
                        t,
                        EventOptions {
                            strength,
                            duration,
                            label,
                            pitch,
                        },
                    )
                    .ok()
                })
                .collect::<Vec<_>>()
        });
        let grid = (0.0..100.0_f64, prop::collection::vec(0.01..10.0_f64, 1..10), 1_u32..5).prop_map(
            |(start, gaps, meter)| {
                let beats = gaps
                    .iter()
                    .scan(start, |at, gap| {
                        *at += gap;
                        Some(*at)
                    })
                    .fold(vec![start], |mut all, at| {
                        all.push(at);
                        all
                    });
                Grid::new(beats, meter, 0).ok()
            },
        );
        (
            proptest::option::of(text),
            proptest::option::of(1.0..1000.0_f64),
            proptest::option::of(grid),
            prop::collection::btree_map(name, series, 0..4),
            prop::collection::btree_map(name, events, 0..4),
        )
            .prop_filter_map("generated in range", |(source, duration, grid, series, events)| {
                Bundle::new(BundleOptions {
                    source,
                    duration,
                    grid: grid.flatten(),
                    series: series
                        .into_iter()
                        .filter_map(|(name, series)| Some((name, series?)))
                        .collect(),
                    events,
                })
                .ok()
            })
    }

    proptest! {
        #[test]
        fn a_valid_bundle_survives_its_json_text_exactly_with_or_without_ascii_only(bundle in arbitrary_bundles()) {
            for ascii_only in [false, true] {
                let text = bundle.to_json(ascii_only);
                prop_assert!(text.ends_with('\n') && text.matches('\n').count() == 1, "{}", text);
                prop_assert!(!ascii_only || text.is_ascii(), "{}", text);
                prop_assert_eq!(Bundle::from_json(&text), Ok(bundle.clone()), "{}", text);
            }
        }

        #[test]
        fn the_two_spellings_of_a_bundle_are_the_same_json(bundle in arbitrary_bundles()) {
            let plain: Result<Value, _> = serde_json::from_str(&bundle.to_json(false));
            let ascii: Result<Value, _> = serde_json::from_str(&bundle.to_json(true));
            prop_assert!(plain.is_ok());
            prop_assert_eq!(plain.map_err(|e| e.to_string()), ascii.map_err(|e| e.to_string()));
        }
    }
}
