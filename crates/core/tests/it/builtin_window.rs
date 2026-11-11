//! The built-in `window` through its public face.

use proptest::prelude::*;
use serde_json::json;

use bullterrier_control::builtin::{thin, window::Params};
use bullterrier_control::formats::{Event, EventOptions};
use bullterrier_control::inputs::{Input, InputKind};
use bullterrier_control::registry::{Call, Kind, Output, Returns};
use bullterrier_control::{Error, Operation, Registry};

fn event(t: f64, options: EventOptions) -> Result<Event, Error> {
    Event::new(t, options).map_err(|source| Error::Format { source })
}

fn moments(times: &[f64]) -> Result<Vec<Event>, Error> {
    times.iter().map(|t| event(*t, EventOptions::default())).collect()
}

fn times(events: &[Event]) -> Vec<f64> {
    events.iter().map(Event::t).collect()
}

fn call(name: &str, values: serde_json::Value, events: &[Event]) -> Result<Vec<Event>, Error> {
    let call = Call {
        inputs: [("events".to_owned(), Input::from_events(events.to_vec()))].into(),
        values,
        ..Call::default()
    };
    match Registry::builtin().get(name)?.call(&call)? {
        Output::Events(kept) => Ok(kept),
        other => Err(Error::Call {
            message: format!("{name} gave {other:?}"),
        }),
    }
}

/// `events` through the thing named `window` in the registry: its input checked, its values read.
fn by_name(values: serde_json::Value, events: &[Event]) -> Result<Vec<Event>, Error> {
    call("window", values, events)
}

/// `events` through the body alone, with the typed parameters.
fn by_body(start: f64, end: f64, count: i64, events: &[Event]) -> Result<Vec<Event>, Error> {
    match (Params {
        start,
        end,
        first: count,
    })
    .run(events.to_vec())?
    {
        Output::Events(kept) => Ok(kept),
        other => Err(Error::Call {
            message: format!("window gave {other:?}"),
        }),
    }
}

/// `(id, start, end, count, times given, times kept)`: the rows of `tests/builtin/test_window.py`.
#[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
const KEEPS: [(&str, f64, f64, i64, &[f64], &[f64]); 13] = [
    ("the defaults keep all", 0.0, 0.0, 0, &[0.0, 1.0, 2.0], &[0.0, 1.0, 2.0]),
    (
        "an event exactly at start is kept",
        2.0,
        0.0,
        0,
        &[1.0, 2.0, 3.0],
        &[2.0, 3.0],
    ),
    (
        "an event a hair before start is not",
        2.0,
        0.0,
        0,
        &[1.9999, 2.0],
        &[2.0],
    ),
    (
        "an event exactly at end is dropped",
        0.0,
        2.0,
        0,
        &[1.0, 2.0, 3.0],
        &[1.0],
    ),
    (
        "an event a hair before end is kept",
        0.0,
        2.0,
        0,
        &[1.9999, 2.0],
        &[1.9999],
    ),
    (
        "start and end make a range: [1, 3)",
        1.0,
        3.0,
        0,
        &[0.5, 1.0, 2.0, 3.0, 4.0],
        &[1.0, 2.0],
    ),
    ("start equal to end is empty", 2.0, 2.0, 0, &[1.0, 2.0, 3.0], &[]),
    (
        "first keeps the first of the range, not of the input",
        2.0,
        0.0,
        2,
        &[1.0, 2.0, 3.0, 4.0],
        &[2.0, 3.0],
    ),
    (
        "first larger than what remains keeps what remains",
        2.0,
        0.0,
        10,
        &[1.0, 2.0, 3.0],
        &[2.0, 3.0],
    ),
    (
        "equal times count one by one",
        0.0,
        0.0,
        2,
        &[1.0, 1.0, 1.0],
        &[1.0, 1.0],
    ),
    (
        "decimals: 0.1 + 0.2 is at 0.3 within rounding",
        0.1 + 0.2,
        0.0,
        0,
        &[0.3],
        &[0.3],
    ),
    (
        "decimals: an event at 0.3 is at the end 0.1 + 0.2 within rounding, so the half-open range drops it",
        0.0,
        0.1 + 0.2,
        0,
        &[0.2, 0.3],
        &[0.2],
    ),
    ("no events keep none", 1.0, 2.0, 3, &[], &[]),
];

#[test]
fn window_keeps_the_events_in_the_range_then_the_first_count() -> Result<(), Error> {
    for (id, start, end, count, given, expected) in KEEPS {
        assert_eq!(
            times(&by_body(start, end, count, &moments(given)?)?),
            expected,
            "{id}: the body"
        );
        assert_eq!(
            times(&by_name(
                json!({"start": start, "end": end, "first": count}),
                &moments(given)?
            )?),
            expected,
            "{id}: the door"
        );
    }
    Ok(())
}

#[test]
fn given_no_values_window_keeps_every_event() -> Result<(), Error> {
    assert_eq!(
        times(&by_name(json!({}), &moments(&[0.0, 5.0, 100_000.0])?)?),
        [0.0, 5.0, 100_000.0]
    );
    Ok(())
}

#[test]
fn window_keeps_every_field_of_what_it_keeps_and_works_in_time_order() -> Result<(), Error> {
    let loud = event(
        3.0,
        EventOptions {
            strength: 0.9,
            label: Some("a".into()),
            pitch: Some(40.0),
            ..EventOptions::default()
        },
    )?;
    let early = event(1.0, EventOptions::default())?;
    let mid = event(2.0, EventOptions::default())?;
    // given out of order: the first two from 2 s are 2 s and 3 s
    let kept = by_name(json!({"start": 2.0, "first": 2}), &[loud.clone(), early, mid.clone()])?;
    assert_eq!(kept, [mid, loud]);
    Ok(())
}

#[test]
fn the_first_four_kicks_after_the_reveal_thinned_are_the_times_worked_by_hand() -> Result<(), Error> {
    let kicks = moments(&[0.5, 1.0, 8.0, 8.1, 8.5, 8.6, 9.2, 10.0])?;
    // from 8 s the first four are 8.0, 8.1, 8.5, 8.6; 0.3 s apart leaves 8.0 and 8.5 (8.1 and 8.6 are 0.1 after)
    let first_four = by_name(json!({"start": 8.0, "first": 4}), &kicks)?;
    assert_eq!(times(&first_four), [8.0, 8.1, 8.5, 8.6]);
    let thinned = call("thin", json!({"gap": 0.3}), &first_four)?;
    assert_eq!(times(&thinned), [8.0, 8.5]);
    // the same through the body types
    assert!(matches!(
        thin::Params { gap: 0.3 }.run(first_four)?,
        Output::Events(kept) if times(&kept) == [8.0, 8.5]
    ));
    Ok(())
}

#[test]
fn a_window_closing_before_it_opens_is_refused_naming_end() -> Result<(), Error> {
    let paths: Vec<String> = match by_name(json!({"start": 5.0, "end": 2.0}), &moments(&[1.0])?) {
        Err(Error::Params { shaper, problems, .. }) if shaper == "window" => {
            problems.iter().map(|problem| problem.path.clone()).collect()
        }
        other => vec![format!("not window's parameter error: {other:?}")],
    };
    assert_eq!(paths, ["end"]);
    // known good: closing at the opening is allowed
    assert_eq!(
        times(&by_name(json!({"start": 5.0, "end": 5.0}), &moments(&[1.0])?)?),
        Vec::<f64>::new()
    );
    Ok(())
}

#[test]
fn values_beyond_their_range_are_refused_naming_the_field() -> Result<(), Error> {
    for (field, value) in [
        ("first", json!(-1)),
        ("start", json!(-0.5)),
        ("end", json!(-0.5)),
        ("first", json!(10001)),
    ] {
        let paths: Vec<String> = match by_name(json!({field: value}), &moments(&[0.5])?) {
            Err(Error::Params { shaper, problems, .. }) if shaper == "window" => {
                problems.iter().map(|problem| problem.path.clone()).collect()
            }
            other => vec![format!("not window's parameter error: {other:?}")],
        };
        assert_eq!(paths, [field], "{field}");
    }
    Ok(())
}

#[test]
fn it_is_an_operation_giving_events_from_events() -> Result<(), Error> {
    let thing = Registry::builtin().get("window")?;
    assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Events));
    assert_eq!(thing.inputs(), [(InputKind::Events, true)]);
    assert!(
        thing.summary().starts_with("The events from `start`"),
        "{}",
        thing.summary()
    );
    Ok(())
}

proptest! {
    /// Times, start and end are multiples of 1/64, which floats hold exactly: the range is decided exactly.
    #[test]
    fn window_agrees_with_a_brute_force_filter(
        steps in prop::collection::vec(0_u16..=256, 0..30),
        start_steps in 0_u16..=300,
        end_steps in 0_u16..=300,
        count in 0_usize..=40,
    ) {
        let start = f64::from(start_steps) / 64.0;
        // an end below start is an error, tested apart: here it is 0 (no end) or at least start
        let end = if end_steps < start_steps { 0.0 } else { f64::from(end_steps) / 64.0 };
        let events: Vec<Event> = steps
            .iter()
            .enumerate()
            .map(|(index, step)| event(f64::from(*step) / 64.0, EventOptions { label: Some(index.to_string()), ..EventOptions::default() }))
            .collect::<Result<_, _>>()
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let mut ordered = events.clone();
        ordered.sort_by(|a, b| a.t().total_cmp(&b.t()));
        let mut expected = Vec::new();
        for candidate in &ordered {
            let after_start = candidate.t() >= start;
            let before_end = end == 0.0 || candidate.t() < end;
            if after_start && before_end && (count == 0 || expected.len() < count) {
                expected.push(candidate.clone());
            }
        }
        let kept = by_name(json!({"start": start, "end": end, "first": count}), &events)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(kept, expected);
    }
}
