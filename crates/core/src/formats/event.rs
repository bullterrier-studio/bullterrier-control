//! [`Event`]: a moment, or a span when it has a duration.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use super::error::{FormatError, Problem};
use super::field::{Number, check, decode, push, required};

/// Something at `t` seconds: an onset, a beat, a note (with `duration` and `pitch`) or a section start.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Event {
    t: f64,
    strength: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pitch: Option<f64>,
}

/// The optional parts of an [`Event`]; end a literal with `..Default::default()`.
#[derive(Debug, Clone, PartialEq)]
pub struct EventOptions {
    /// How strong the event is, in `[0, 1]`. Default 1.
    pub strength: f64,
    /// The length of a span in seconds, `> 0`. Default `None`: a moment.
    pub duration: Option<f64>,
    /// A name for it (`"A"`, `"drop"`, `"kick"`), not empty. Default `None`.
    pub label: Option<String>,
    /// A MIDI note number in `[0, 127]`, fractions allowed. Default `None`.
    pub pitch: Option<f64>,
}

impl Default for EventOptions {
    fn default() -> Self {
        Self {
            strength: 1.0,
            duration: None,
            label: None,
            pitch: None,
        }
    }
}

impl Event {
    /// The event at `t` seconds with `options`.
    ///
    /// # Errors
    /// A [`FormatError`] listing every rule broken: `t` is below 0, `strength` outside `[0, 1]`, `duration` not above 0,
    /// `label` empty, `pitch` outside `[0, 127]`, or any number not finite.
    pub fn new(t: f64, options: EventOptions) -> Result<Self, FormatError> {
        let mut problems = Vec::new();
        Self::build("", Some(t), options, &mut problems).ok_or_else(|| FormatError::new(problems))
    }

    /// The one place the rules are written. Every rule `options` breaks is recorded in `problems` under `prefix`
    /// (empty, or where this event sits in a document: `events.kick[3]`), after what the caller already collected;
    /// `t` is `None` when the data lacked it. The event, if none is broken.
    fn build(prefix: &str, t: Option<f64>, options: EventOptions, problems: &mut Vec<Problem>) -> Option<Self> {
        let before = problems.len();
        match t {
            Some(t) => check(problems, prefix, "t", t, "must be >= 0", |t| t >= 0.0),
            None => required(problems, prefix, "t"),
        }
        check(
            problems,
            prefix,
            "strength",
            options.strength,
            "must be in [0, 1]",
            |x| (0.0..=1.0).contains(&x),
        );
        if let Some(duration) = options.duration {
            check(problems, prefix, "duration", duration, "must be > 0", |x| x > 0.0);
        }
        if options.label.as_deref() == Some("") {
            push(problems, prefix, "label", "must not be empty");
        }
        if let Some(pitch) = options.pitch {
            check(problems, prefix, "pitch", pitch, "must be in [0, 127]", |x| {
                (0.0..=127.0).contains(&x)
            });
        }
        match t {
            Some(t) if problems.len() == before => Some(Self {
                t,
                strength: options.strength,
                duration: options.duration,
                label: options.label,
                pitch: options.pitch,
            }),
            _ => None,
        }
    }

    /// Read an event from any serde format (JSON, a Python object) and check it with [`Event::new`]'s rules. A field
    /// absent or `null` is unset (`t` is then required, `strength` is 1).
    ///
    /// # Errors
    /// A [`FormatError`]: the one problem that stopped the reading (a wrong type for a field, an unknown or repeated
    /// field, something that is not an object, a whole number beyond 2^53), or, when the data was read, every rule it
    /// breaks.
    pub fn from_deserializer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, FormatError> {
        let raw: RawEvent = decode(deserializer)?;
        let mut problems = Vec::new();
        raw.read("", &mut problems).ok_or_else(|| FormatError::new(problems))
    }

    /// When it happens, in seconds from the start of the material.
    #[must_use]
    pub const fn t(&self) -> f64 {
        self.t
    }

    /// How strong it is, in `[0, 1]`.
    #[must_use]
    pub const fn strength(&self) -> f64 {
        self.strength
    }

    /// The length of the span in seconds, or `None` for a moment.
    #[must_use]
    pub const fn duration(&self) -> Option<f64> {
        self.duration
    }

    /// Its name, if it has one.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Its MIDI note number, if it has one.
    #[must_use]
    pub const fn pitch(&self) -> Option<f64> {
        self.pitch
    }
}

/// The fields as a file spells them, before any rule is checked; an absent field and a `null` one are both `None`.
/// A document that holds events (a bundle) reads them as this and calls [`RawEvent::read`] with each one's place in it,
/// so every problem of every event is kept, under a full path.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawEvent {
    t: Option<Number>,
    strength: Option<Number>,
    duration: Option<Number>,
    label: Option<String>,
    pitch: Option<Number>,
}

impl RawEvent {
    /// Check the rules of [`Event::new`], recording each broken one in `problems` under `prefix` (`events.kick[3]`);
    /// the event if none is broken.
    pub(crate) fn read(self, prefix: &str, problems: &mut Vec<Problem>) -> Option<Event> {
        let defaults = EventOptions::default();
        let options = EventOptions {
            strength: self.strength.map_or(defaults.strength, |number| number.0),
            duration: self.duration.map(|number| number.0),
            label: self.label,
            pitch: self.pitch.map(|number| number.0),
        };
        Event::build(prefix, self.t.map(|number| number.0), options, problems)
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_deserializer(deserializer).map_err(D::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "the getters give back exactly what was given")]
mod tests {
    use super::super::test_support::{Expected, check_against, check_problems, object_of_floats};
    use super::*;
    use proptest::prelude::*;
    use serde_json::{Value, json};

    /// A case: its name, `t`, the options and the problems wanted.
    type Row = (&'static str, f64, EventOptions, Expected);

    fn with(f: impl FnOnce(&mut EventOptions)) -> EventOptions {
        let mut options = EventOptions::default();
        f(&mut options);
        options
    }

    #[test]
    fn each_rule_holds_on_one_side_of_its_boundary_and_breaks_on_the_other() {
        let label = |text: &str| with(|o| o.label = Some(text.to_owned()));
        #[rustfmt::skip]
        let rows: Vec<Row> = vec![
            ("t: zero is the start", 0.0, EventOptions::default(), &[]),
            ("t: negative zero is zero", -0.0, EventOptions::default(), &[]),
            ("t: a long way in", 86_400.5, EventOptions::default(), &[]),
            ("t: below zero", -0.1, EventOptions::default(), &[("t", ">= 0 (got -0.1)")]),
            ("t: NaN", f64::NAN, EventOptions::default(), &[("t", "finite (got NaN)")]),
            ("t: infinity", f64::INFINITY, EventOptions::default(), &[("t", "finite (got Infinity)")]),
            ("strength: zero", 0.5, with(|o| o.strength = 0.0), &[]),
            ("strength: one", 0.5, with(|o| o.strength = 1.0), &[]),
            ("strength: just above one", 0.5, with(|o| o.strength = 1.0001), &[("strength", "[0, 1] (got 1.0001)")]),
            ("strength: just below zero", 0.5, with(|o| o.strength = -0.0001), &[("strength", "[0, 1] (got -0.0001)")]),
            ("strength: NaN", 0.5, with(|o| o.strength = f64::NAN), &[("strength", "finite")]),
            ("duration: tiny but positive", 0.5, with(|o| o.duration = Some(1e-9)), &[]),
            ("duration: zero is not a span", 0.5, with(|o| o.duration = Some(0.0)), &[("duration", "> 0 (got 0.0)")]),
            ("duration: negative", 0.5, with(|o| o.duration = Some(-2.0)), &[("duration", "> 0 (got -2.0)")]),
            ("duration: infinite", 0.5, with(|o| o.duration = Some(f64::INFINITY)), &[("duration", "finite")]),
            ("label: one letter", 0.5, label("A"), &[]),
            ("label: text with a space and an accent", 0.5, label("drop é"), &[]),
            ("label: empty", 0.5, label(""), &[("label", "empty")]),
            ("pitch: lowest note", 0.5, with(|o| o.pitch = Some(0.0)), &[]),
            ("pitch: highest note", 0.5, with(|o| o.pitch = Some(127.0)), &[]),
            ("pitch: a fraction between notes", 0.5, with(|o| o.pitch = Some(60.5)), &[]),
            ("pitch: above the highest", 0.5, with(|o| o.pitch = Some(127.5)), &[("pitch", "[0, 127] (got 127.5)")]),
            ("pitch: below the lowest", 0.5, with(|o| o.pitch = Some(-0.5)), &[("pitch", "[0, 127] (got -0.5)")]),
            (
                "every field broken: all listed in field order",
                -1.0,
                EventOptions { strength: 2.0, duration: Some(0.0), label: Some(String::new()), pitch: Some(128.0) },
                &[("t", ">= 0"), ("strength", "[0, 1]"), ("duration", "> 0"), ("label", "empty"), ("pitch", "[0, 127]")],
            ),
            (
                "two broken, the ones between fine",
                f64::NAN,
                with(|o| o.pitch = Some(200.0)),
                &[("t", "finite"), ("pitch", "[0, 127] (got 200.0)")],
            ),
        ];
        for (id, t, options, expected) in rows {
            check_against(id, Event::new(t, options), expected);
        }
    }

    #[test]
    fn an_event_keeps_what_it_was_given_and_defaults_the_rest() -> Result<(), FormatError> {
        let moment = Event::new(0.5, EventOptions::default())?;
        assert_eq!(
            (
                moment.t(),
                moment.strength(),
                moment.duration(),
                moment.label(),
                moment.pitch()
            ),
            (0.5, 1.0, None, None, None)
        );
        let options = EventOptions {
            strength: 0.25,
            duration: Some(1.5),
            label: Some("A".to_owned()),
            pitch: Some(60.5),
        };
        let note = Event::new(2.0, options)?;
        assert_eq!(
            (note.t(), note.strength(), note.duration(), note.label(), note.pitch()),
            (2.0, 0.25, Some(1.5), Some("A"), Some(60.5))
        );
        Ok(())
    }

    fn read(text: &str) -> Result<Event, FormatError> {
        Event::from_deserializer(&mut serde_json::Deserializer::from_str(text))
    }

    #[test]
    fn json_is_read_strictly_and_every_broken_rule_is_listed_with_its_path() {
        #[rustfmt::skip]
        let rows: &[(&str, &str, Expected)] = &[
            ("only t", r#"{"t": 0.5}"#, &[]),
            ("a whole number for t", r#"{"t": 1}"#, &[]),
            ("an exponent", r#"{"t": 1e2}"#, &[]),
            ("null is absent", r#"{"t": 0.5, "strength": null, "duration": null, "label": null, "pitch": null}"#, &[]),
            ("everything", r#"{"t": 3, "strength": 0.5, "duration": 2, "label": "drop", "pitch": 60.5}"#, &[]),
            ("2^53 is exact", r#"{"t": 9007199254740992}"#, &[]),
            ("2^53 + 1 is not", r#"{"t": 9007199254740993}"#, &[("t", "2^53")]),
            ("negative 2^53 is exact, so it is only out of range", r#"{"t": 0, "pitch": -9007199254740992}"#, &[("pitch", "[0, 127] (got -9007199254740992.0)")]),
            ("negative beyond 2^53", r#"{"t": 0, "pitch": -9007199254740993}"#, &[("pitch", "2^53")]),
            ("a bool for a number", r#"{"t": true}"#, &[("t", "boolean")]),
            ("a string for a number", r#"{"t": "1"}"#, &[("t", "string")]),
            ("NaN is not JSON", r#"{"t": NaN}"#, &[("t", "expected value")]),
            ("a number out of range", r#"{"t": 1e999}"#, &[("t", "out of range")]),
            ("null for t is missing", r#"{"t": null}"#, &[("t", "is required")]),
            ("no t", r#"{"strength": 0.5}"#, &[("t", "is required")]),
            ("missing t and a broken rule", r#"{"strength": 2}"#, &[("t", "is required"), ("strength", "[0, 1] (got 2.0)")]),
            ("an unknown field", r#"{"t": 0.5, "colour": "red"}"#, &[("colour", "unknown field")]),
            ("a repeated field", r#"{"t": 0.5, "t": 0.6}"#, &[("t", "duplicate")]),
            ("an integer for a label", r#"{"t": 0.5, "label": 5}"#, &[("label", "string")]),
            ("the wrong type stops at its field", r#"{"t": true, "strength": 5}"#, &[("t", "boolean")]),
            ("rules broken in two fields", r#"{"t": -1, "strength": 2, "label": ""}"#, &[("t", ">= 0 (got -1.0)"), ("strength", "[0, 1]"), ("label", "empty")]),
            ("an array is not an event", "[0.5]", &[("", "an object")]),
            ("an array of the fields is not an event", "[0.5, null, null, null, null]", &[("", "an object")]),
            ("a bare number is not an event", "0.5", &[("", "an object")]),
        ];
        for (id, text, expected) in rows {
            check_against(id, read(text), expected);
        }
    }

    #[test]
    fn a_format_that_delivers_nan_still_lists_the_other_broken_rules() {
        let fields = [("t", f64::NAN), ("strength", 2.0), ("pitch", f64::INFINITY)];
        check_against(
            "NaN beside a range error and an infinity",
            Event::from_deserializer(object_of_floats(&fields)),
            &[
                ("t", "finite (got NaN)"),
                ("strength", "[0, 1] (got 2.0)"),
                ("pitch", "finite (got Infinity)"),
            ],
        );
    }

    #[test]
    fn an_event_inside_a_document_reports_every_problem_under_its_own_path() -> Result<(), serde_json::Error> {
        let mut problems = Vec::new();
        let fine: RawEvent = serde_json::from_value(json!({"t": 1.5}))?;
        let broken: RawEvent = serde_json::from_value(json!({"t": -1, "strength": 2, "label": "ok"}))?;
        let also_broken: RawEvent = serde_json::from_value(json!({"strength": 0.5, "pitch": 128}))?;

        assert!(fine.read("events.kick[2]", &mut problems).is_some());
        assert!(problems.is_empty(), "{problems:?}");
        assert!(broken.read("events.kick[3]", &mut problems).is_none());
        assert!(also_broken.read("events.snare[0]", &mut problems).is_none());
        check_problems(
            "two items, the second added after the first",
            &problems,
            &[
                ("events.kick[3].t", ">= 0 (got -1.0)"),
                ("events.kick[3].strength", "[0, 1] (got 2.0)"),
                ("events.snare[0].t", "is required"),
                ("events.snare[0].pitch", "[0, 127] (got 128.0)"),
            ],
        );
        Ok(())
    }

    #[test]
    fn json_defaults_the_strength_and_keeps_the_fields_read() -> Result<(), FormatError> {
        let event = read(r#"{"t": 2, "duration": 0.5, "label": "kick"}"#)?;
        assert_eq!(
            (
                event.t(),
                event.strength(),
                event.duration(),
                event.label(),
                event.pitch()
            ),
            (2.0, 1.0, Some(0.5), Some("kick"), None)
        );
        Ok(())
    }

    #[test]
    fn serde_json_reads_and_writes_an_event_leaving_unset_fields_out() -> Result<(), serde_json::Error> {
        let event: Event = serde_json::from_str(r#"{"t": 0.5, "pitch": 60}"#)?;
        assert_eq!(
            serde_json::to_value(&event)?,
            json!({"t": 0.5, "strength": 1.0, "pitch": 60.0})
        );
        let refused = serde_json::from_str::<Event>(r#"{"t": -2}"#)
            .err()
            .map(|error| error.to_string());
        assert!(refused.is_some_and(|text| text.contains("t: must be >= 0 (got -2.0)")));
        Ok(())
    }

    #[test]
    fn a_format_error_reads_as_path_rule_pairs_and_crosses_threads() {
        fn assert_send_sync<T: Send + Sync + std::error::Error + 'static>() {}
        assert_send_sync::<FormatError>();
        let error = Event::new(
            -1.0,
            EventOptions {
                strength: 3.0,
                ..Default::default()
            },
        )
        .err();
        assert_eq!(
            error.map(|error| error.to_string()).as_deref(),
            Some("t: must be >= 0 (got -1.0); strength: must be in [0, 1] (got 3.0)")
        );
    }

    fn finite_t() -> impl Strategy<Value = f64> {
        0.0..1.0e9_f64
    }

    fn valid_event() -> impl Strategy<Value = Event> {
        (
            finite_t(),
            0.0..=1.0_f64,
            proptest::option::of(1.0e-6..1.0e6_f64),
            proptest::option::of(".{1,12}"),
            proptest::option::of(0.0..=127.0_f64),
        )
            .prop_map(|(t, strength, duration, label, pitch)| {
                Event::new(
                    t,
                    EventOptions {
                        strength,
                        duration,
                        label,
                        pitch,
                    },
                )
            })
            .prop_filter_map("generated in range", Result::ok)
    }

    proptest! {
        #[test]
        fn a_valid_event_survives_json_text_exactly(event in valid_event()) {
            let text = serde_json::to_string(&event).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(read(&text), Ok(event), "{}", text);
        }

        #[test]
        fn a_valid_event_survives_a_json_value(event in valid_event()) {
            let value: Value = serde_json::to_value(&event).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(Event::from_deserializer(value), Ok(event));
        }

        #[test]
        fn a_refused_event_always_says_which_field(
            t in any::<f64>(), strength in any::<f64>(), duration in proptest::option::of(any::<f64>()),
        ) {
            if let Err(error) = Event::new(t, EventOptions { strength, duration, ..Default::default() }) {
                prop_assert!(!error.problems.is_empty());
                for problem in &error.problems {
                    prop_assert!(["t", "strength", "duration"].contains(&problem.path.as_str()), "{:?}", problem);
                }
            }
        }
    }
}
