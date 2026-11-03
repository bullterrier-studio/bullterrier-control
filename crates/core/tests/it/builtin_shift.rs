//! The built-in `shift` through its public face.

use serde_json::json;

use bullterrier_control::builtin::shift::Params;
use bullterrier_control::formats::{Event, EventOptions};
use bullterrier_control::inputs::{Input, InputKind};
use bullterrier_control::registry::{Call, Kind, Output, Returns};
use bullterrier_control::{Error, Operation, Registry};

const LARGEST: f64 = f64::MAX;

fn event(t: f64, options: EventOptions) -> Result<Event, Error> {
    Event::new(t, options).map_err(|source| Error::Format { source })
}

fn moment(t: f64) -> Result<Event, Error> {
    event(
        t,
        EventOptions {
            strength: 0.5,
            ..EventOptions::default()
        },
    )
}

fn span(t: f64, duration: f64) -> Result<Event, Error> {
    event(
        t,
        EventOptions {
            duration: Some(duration),
            ..EventOptions::default()
        },
    )
}

fn times(events: &[Event]) -> Vec<f64> {
    events.iter().map(Event::t).collect()
}

/// `events` through the thing named `shift` in the registry: its input checked, its values read.
fn by_name(offset: f64, events: &[Event]) -> Result<Vec<Event>, Error> {
    let call = Call {
        inputs: [("events".to_owned(), Input::from_events(events.to_vec()))].into(),
        values: json!({"offset": offset}),
        ..Call::default()
    };
    match Registry::builtin().get("shift")?.call(&call)? {
        Output::Events(moved) => Ok(moved),
        other => Err(Error::Call {
            message: format!("shift gave {other:?}"),
        }),
    }
}

/// `events` through the body alone, with the typed parameter.
fn by_body(offset: f64, events: &[Event]) -> Result<Vec<Event>, Error> {
    match (Params { offset }).run(events.to_vec())? {
        Output::Events(moved) => Ok(moved),
        other => Err(Error::Call {
            message: format!("shift gave {other:?}"),
        }),
    }
}

/// Both ways of running it, which must agree.
fn shifted(offset: f64, events: &[Event]) -> Result<Vec<Event>, Error> {
    let moved = by_body(offset, events)?;
    assert_eq!(by_name(offset, events)?, moved, "the door and the body agree");
    Ok(moved)
}

fn close(left: f64, right: f64) -> bool {
    left.to_bits() == right.to_bits() || (left - right).abs() <= 1e-12
}

/// `(id, offset, times given, times moved)`: most are multiples of 1/8, which floats hold exactly.
#[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
const MOMENTS: [(&str, f64, &[f64], &[f64]); 5] = [
    (
        "a positive offset moves every moment later",
        0.25,
        &[0.0, 0.5, 1.0],
        &[0.25, 0.75, 1.25],
    ),
    (
        "0.1 would land before 0 and is dropped; 0.25 lands at 0",
        -0.25,
        &[0.1, 0.25, 1.0],
        &[0.0, 0.75],
    ),
    (
        "a moment landing exactly at 0 is kept",
        -0.25,
        &[0.125, 0.25, 0.375],
        &[0.0, 0.125],
    ),
    ("the largest lead", -3600.0, &[3599.0, 3600.0, 3601.0], &[0.0, 1.0]),
    ("the largest lag", 3600.0, &[0.0, 1.0], &[3600.0, 3601.0]),
];

#[test]
fn shift_moves_each_moment_by_the_offset_and_drops_those_that_land_before_zero() -> Result<(), Error> {
    for (id, offset, given, expected) in MOMENTS {
        let events: Vec<Event> = given.iter().map(|t| moment(*t)).collect::<Result<_, _>>()?;
        assert_eq!(times(&shifted(offset, &events)?), expected, "{id}");
    }
    Ok(())
}

/// `(id, offset, moment, the time it lands at)`: a landing at 0 is decided to within a nanosecond and no looser.
const LANDINGS: [(&str, f64, f64, Option<f64>); 3] = [
    (
        "a moment 1e-16 before 0 (0.1 + 0.7 - 0.8) is at 0",
        -0.8,
        0.799_999_999_999_999_9,
        Some(0.0),
    ),
    ("a decimal moment landing at 0", -0.3, 0.3, Some(0.0)),
    ("a moment a millionth after 0 is kept", -0.499_999, 0.5, Some(1e-6)),
];

#[test]
fn shift_keeps_a_moment_landing_at_zero_within_rounding() -> Result<(), Error> {
    for (id, offset, given, expected) in LANDINGS {
        let moved = times(&shifted(offset, &[moment(given)?])?);
        assert_eq!(moved.len(), usize::from(expected.is_some()), "{id}");
        assert!(
            moved
                .iter()
                .zip(expected)
                .all(|(got, want)| close(*got, want) && *got >= 0.0),
            "{id}: {moved:?}"
        );
    }
    Ok(())
}

#[test]
fn shift_drops_a_moment_a_millionth_before_zero() -> Result<(), Error> {
    assert_eq!(shifted(-0.500_001, &[moment(0.5)?])?, []);
    Ok(())
}

#[test]
fn shift_by_the_default_changes_nothing() -> Result<(), Error> {
    let note = event(
        0.5,
        EventOptions {
            strength: 0.3,
            duration: Some(0.25),
            label: Some("lead".into()),
            pitch: Some(64.5),
        },
    )?;
    let given = [moment(0.1)?, note, span(0.0, 2.0)?];
    // unchanged, in time order
    let expected = [given[2].clone(), given[0].clone(), given[1].clone()];
    assert_eq!(by_body(0.0, &expected)?, expected);
    assert_eq!(by_name(0.0, &given)?, expected);
    Ok(())
}

/// `(id, offset, start, duration, the (start, duration) it ends up with)`.
#[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
const SPANS: [(&str, f64, f64, f64, (f64, f64)); 9] = [
    (
        "a span cut at 0 keeps the rest: 1.125 - 0.375",
        -0.375,
        0.125,
        1.0,
        (0.0, 0.75),
    ),
    ("starts at -0.25, ends at 0.25: 0.25 left", -0.5, 0.25, 0.5, (0.0, 0.25)),
    ("starts exactly at 0: whole, not cut", -0.5, 0.5, 1.0, (0.0, 1.0)),
    ("moved later: the duration is unchanged", 0.25, 0.5, 1.0, (0.75, 1.0)),
    ("a span of the largest length", -3600.0, 0.0, LARGEST, (0.0, LARGEST)),
    (
        "the largest time and span",
        3600.0,
        LARGEST,
        LARGEST,
        (LARGEST, LARGEST),
    ),
    (
        "decimals: a span ending 1e-6 after 0 is kept",
        -0.3,
        0.1,
        0.200_001,
        (0.0, 1e-6),
    ),
    (
        "decimals: starts at -0.2, ends at 0.5, 0.5 left",
        -0.3,
        0.1,
        0.7,
        (0.0, 0.5),
    ),
    (
        "decimals: starts at -0.4, ends at 1.0, 1.0 left",
        -0.7,
        0.3,
        1.4,
        (0.0, 1.0),
    ),
];

#[test]
fn shift_cuts_a_span_that_starts_before_zero_at_zero_and_keeps_the_rest() -> Result<(), Error> {
    for (id, offset, t, duration, (start, rest)) in SPANS {
        let moved = shifted(offset, &[span(t, duration)?])?;
        let got: Vec<(f64, Option<f64>)> = moved.iter().map(|moved| (moved.t(), moved.duration())).collect();
        assert!(
            matches!(got[..], [(s, Some(d))] if close(s, start) && close(d, rest)),
            "{id}: {got:?} is not ({start}, {rest})"
        );
    }
    Ok(())
}

/// `(id, offset, start, duration)` of a span that ends at or before 0.
const GONE: [(&str, f64, f64, f64); 6] = [
    ("ends exactly at 0", -0.75, 0.25, 0.5),
    ("ends before 0", -1.0, 0.25, 0.25),
    ("decimals: 0.1 + 0.2 - 0.3 is 3e-17, which is 0", -0.3, 0.1, 0.2),
    ("decimals: ends at 0 within rounding", -0.7, 0.3, 0.4),
    ("decimals: ends at 0 within rounding, below it", -1.3, 1.1, 0.2),
    ("decimals: ends 1e-6 before 0", -0.3, 0.1, 0.199_999),
];

#[test]
fn shift_drops_a_span_that_ends_at_or_before_zero() -> Result<(), Error> {
    for (id, offset, t, duration) in GONE {
        assert_eq!(shifted(offset, &[span(t, duration)?])?, [], "{id}");
    }
    Ok(())
}

#[test]
fn shift_keeps_the_strength_label_and_pitch_of_a_cut_span() -> Result<(), Error> {
    let options = EventOptions {
        strength: 0.6,
        duration: Some(0.5),
        label: Some("riser".into()),
        pitch: Some(60.0),
    };
    let cut = EventOptions {
        duration: Some(0.25),
        ..options.clone()
    };
    assert_eq!(shifted(-0.5, &[event(0.25, options)?])?, [event(0.0, cut)?]);
    Ok(())
}

#[test]
fn shift_keeps_events_at_the_same_time_in_the_order_given() -> Result<(), Error> {
    let named = |name: &str| {
        event(
            1.0,
            EventOptions {
                label: Some(name.into()),
                ..EventOptions::default()
            },
        )
    };
    let (first, second) = (named("a")?, named("b")?);
    let moved = shifted(0.5, &[second.clone(), first.clone()])?;
    assert_eq!(
        moved.iter().map(|moved| moved.label()).collect::<Vec<_>>(),
        [Some("b"), Some("a")]
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
fn shift_keeps_events_at_zero_and_minus_zero_in_the_order_given() -> Result<(), Error> {
    let [a, b] = tied()?;
    let labels = |moved: Vec<Event>| {
        moved
            .iter()
            .map(|moved| moved.label().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labels(by_name(0.0, &[a.clone(), b.clone()])?),
        [Some("a".to_owned()), Some("b".to_owned())]
    );
    assert_eq!(
        labels(by_name(0.0, &[b, a])?),
        [Some("b".to_owned()), Some("a".to_owned())]
    );
    Ok(())
}

#[test]
fn shift_works_in_time_order_whatever_the_order_given() -> Result<(), Error> {
    let given = [moment(1.0)?, moment(0.25)?, moment(0.5)?];
    assert_eq!(times(&by_name(0.25, &given)?), [0.5, 0.75, 1.25]);
    Ok(())
}

#[test]
fn shift_of_no_events_is_no_events() -> Result<(), Error> {
    assert_eq!(shifted(1.0, &[])?, []);
    Ok(())
}

#[test]
fn an_offset_beyond_its_range_is_refused_naming_it() -> Result<(), Error> {
    for (id, offset) in [("below -3600", -3600.5), ("above 3600", 3600.5)] {
        let paths: Vec<String> = match by_name(offset, &[moment(0.5)?]) {
            Err(Error::Params { shaper, problems, .. }) if shaper == "shift" => {
                problems.iter().map(|problem| problem.path.clone()).collect()
            }
            other => vec![format!("not shift's parameter error: {other:?}")],
        };
        assert_eq!(paths, ["offset"], "{id}");
    }
    Ok(())
}

#[test]
fn it_is_an_operation_giving_events_from_events() -> Result<(), Error> {
    let thing = Registry::builtin().get("shift")?;
    assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Events));
    assert_eq!(thing.inputs(), [(InputKind::Events, true)]);
    assert!(
        thing.summary().starts_with("The events with every time moved"),
        "{}",
        thing.summary()
    );
    Ok(())
}
