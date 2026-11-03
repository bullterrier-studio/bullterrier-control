//! The built-in `every` through its public face.

use serde_json::json;

use crate::common::problems::check_problems;
use bullterrier_control::Params as _;
use bullterrier_control::builtin::every::Params;
use bullterrier_control::formats::{Event, EventOptions};
use bullterrier_control::inputs::{Input, InputKind};
use bullterrier_control::registry::{Call, Kind, Output, Returns};
use bullterrier_control::{Error, Operation, Registry};

fn event(t: f64, options: EventOptions) -> Result<Event, Error> {
    Event::new(t, options).map_err(|source| Error::Format { source })
}

/// Seven events a quarter second apart, each telling its index by its label.
fn seven() -> Result<Vec<Event>, Error> {
    (0..7_u8)
        .map(|i| {
            event(
                f64::from(i) * 0.25,
                EventOptions {
                    label: Some(i.to_string()),
                    ..EventOptions::default()
                },
            )
        })
        .collect()
}

fn indices(events: &[Event]) -> Vec<u8> {
    events
        .iter()
        .map(|kept| kept.label().and_then(|label| label.parse().ok()).unwrap_or(u8::MAX))
        .collect()
}

/// `events` through the thing named `every` in the registry: its input checked, its values read.
fn by_name(values: serde_json::Value, events: &[Event]) -> Result<Vec<Event>, Error> {
    let call = Call {
        inputs: [("events".to_owned(), Input::from_events(events.to_vec()))].into(),
        values,
        ..Call::default()
    };
    match Registry::builtin().get("every")?.call(&call)? {
        Output::Events(kept) => Ok(kept),
        other => Err(Error::Call {
            message: format!("every gave {other:?}"),
        }),
    }
}

/// `events` through the body alone, with the typed parameters.
fn by_body(n: i64, offset: i64, events: &[Event]) -> Result<Vec<Event>, Error> {
    match (Params { n, offset }).run(events.to_vec())? {
        Output::Events(kept) => Ok(kept),
        other => Err(Error::Call {
            message: format!("every gave {other:?}"),
        }),
    }
}

/// `(id, n, offset, the indices kept of seven events)`: the rows of `tests/builtin/test_every.py`.
#[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
const KEEPS: [(&str, i64, i64, &[u8]); 8] = [
    (
        "the defaults keep every second event, from the first",
        2,
        0,
        &[0, 2, 4, 6],
    ),
    ("every third from the second: 1, 4 (7 is past the end)", 3, 1, &[1, 4]),
    ("every third from the first", 3, 0, &[0, 3, 6]),
    ("n = 1 keeps every event", 1, 0, &[0, 1, 2, 3, 4, 5, 6]),
    ("an offset starts at that event", 2, 1, &[1, 3, 5]),
    ("the last offset below n keeps one", 4, 3, &[3]),
    ("n above the count keeps only the first", 10, 0, &[0]),
    ("an offset past the last event keeps none", 10, 9, &[]),
];

#[test]
fn every_keeps_the_events_at_offset_plus_whole_multiples_of_n() -> Result<(), Error> {
    for (id, n, offset, expected) in KEEPS {
        assert_eq!(indices(&by_body(n, offset, &seven()?)?), expected, "{id}: the body");
        assert_eq!(
            indices(&by_name(json!({"n": n, "offset": offset}), &seven()?)?),
            expected,
            "{id}: the door"
        );
    }
    Ok(())
}

#[test]
fn every_counts_in_time_order_whatever_the_order_given() -> Result<(), Error> {
    let mut backwards = seven()?;
    backwards.reverse();
    assert_eq!(indices(&by_name(json!({"n": 3, "offset": 1}), &backwards)?), [1, 4]);
    Ok(())
}

#[test]
fn every_counts_events_at_the_same_time_one_by_one_in_the_order_given() -> Result<(), Error> {
    let named: Vec<Event> = "abcd"
        .chars()
        .map(|name| {
            event(
                1.0,
                EventOptions {
                    label: Some(name.to_string()),
                    ..EventOptions::default()
                },
            )
        })
        .collect::<Result<_, _>>()?;
    let reversed: Vec<Event> = named.iter().rev().cloned().collect();
    assert_eq!(by_body(2, 0, &named)?, [named[0].clone(), named[2].clone()]);
    assert_eq!(
        by_name(json!({}), &reversed)?,
        [reversed[0].clone(), reversed[2].clone()]
    );
    Ok(())
}

/// 0.0 and -0.0 are one time: the door keeps the order given, `a` then `b`, and so does the operation.
fn tied() -> Result<[Event; 2], Error> {
    let named = |t: f64, name: &str| {
        event(
            t,
            EventOptions {
                label: Some(name.into()),
                ..EventOptions::default()
            },
        )
    };
    Ok([named(0.0, "a")?, named(-0.0, "b")?])
}

#[test]
fn every_counts_events_at_zero_and_minus_zero_as_one_time_in_the_order_given() -> Result<(), Error> {
    let [a, b] = tied()?;
    assert_eq!(by_name(json!({}), &[a.clone(), b.clone()])?, std::slice::from_ref(&a));
    assert_eq!(by_name(json!({}), &[b.clone(), a])?, std::slice::from_ref(&b));
    Ok(())
}

#[test]
fn every_keeps_every_field_of_the_events_it_keeps() -> Result<(), Error> {
    let options = EventOptions {
        strength: 0.7,
        duration: Some(0.25),
        label: Some("lead".into()),
        pitch: Some(64.5),
    };
    let note = event(0.5, options)?;
    let events = [note.clone(), event(0.75, EventOptions::default())?];
    assert_eq!(by_body(2, 0, &events)?, std::slice::from_ref(&note));
    assert_eq!(by_name(json!({"n": 2}), &events)?, [note]);
    Ok(())
}

#[test]
fn every_of_no_events_is_no_events() -> Result<(), Error> {
    assert_eq!(by_body(2, 0, &[])?, []);
    assert_eq!(by_name(json!({}), &[])?, []);
    Ok(())
}

#[test]
fn n_and_offset_are_accepted_up_to_1000_and_999_and_refused_beyond() -> Result<(), Error> {
    assert_eq!(by_name(json!({"n": 1000, "offset": 999}), &seven()?)?, []);
    assert_eq!(indices(&by_name(json!({"n": 1000}), &seven()?)?), [0]);
    for (id, values, named) in [
        ("n above 1000", json!({"n": 1001}), "n"),
        ("n below 1", json!({"n": 0}), "n"),
        ("offset above 999", json!({"n": 1000, "offset": 1000}), "offset"),
        ("offset below 0", json!({"offset": -1}), "offset"),
        ("an offset not below n", json!({"n": 3, "offset": 3}), "offset"),
    ] {
        let paths: Vec<String> = match by_name(values, &seven()?) {
            Err(Error::Params { shaper, problems, .. }) if shaper == "every" => {
                problems.iter().map(|problem| problem.path.clone()).collect()
            }
            other => vec![format!("not every's parameter error: {other:?}")],
        };
        assert_eq!(paths, [named], "{id}");
    }
    Ok(())
}

#[test]
fn it_is_an_operation_giving_events_from_events() -> Result<(), Error> {
    let thing = Registry::builtin().get("every")?;
    assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Events));
    assert_eq!(thing.inputs(), [(InputKind::Events, true)]);
    assert!(
        thing.summary().starts_with("The events at positions"),
        "{}",
        thing.summary()
    );
    Ok(())
}

fn check(id: &str, given: &serde_json::Value, expected: &'static [(&'static str, &'static str)]) {
    match Params::parse(given) {
        Ok(params) => assert!(expected.is_empty(), "{id}: accepted {params:?}, wanted {expected:?}"),
        Err(error) => check_problems(id, &error.problems, expected),
    }
}

#[test]
fn the_offset_must_be_below_n() {
    for (id, given, expected) in [
        ("the defaults", json!({}), &[][..]),
        ("one below n", json!({"n": 3, "offset": 2}), &[]),
        ("the first event of many", json!({"n": 7, "offset": 0}), &[]),
        (
            "equal to n",
            json!({"n": 3, "offset": 3}),
            &[("offset", "must be below n = 3 (got 3)")],
        ),
        (
            "past n",
            json!({"n": 2, "offset": 5}),
            &[("offset", "must be below n = 2 (got 5)")],
        ),
    ] {
        check(id, &given, expected);
    }
}

#[test]
fn a_broken_field_rule_hides_the_rule_between_fields() {
    check(
        "offset past its own range",
        &json!({"n": 3, "offset": 1000}),
        &[("offset", "<= 999")],
    );
}
