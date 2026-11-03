//! The built-in `beats` through its public face.

use proptest::prelude::*;
use serde_json::json;

use bullterrier_control::builtin::beats::Params;
use bullterrier_control::formats::{Event, EventOptions, Grid};
use bullterrier_control::inputs::{Input, InputKind};
use bullterrier_control::registry::{Call, Kind, Output, Returns};
use bullterrier_control::{Error, Operation, Registry};

fn grid(beats: Vec<f64>, meter: u32, first_downbeat: u32) -> Result<Grid, Error> {
    Grid::new(beats, meter, first_downbeat).map_err(|source| Error::Format { source })
}

/// A beat every half second; the first downbeat is the second beat (index 1) and a bar is 4 beats.
fn steady() -> Result<Grid, Error> {
    grid(vec![0.0, 0.5, 1.0, 1.5, 2.0], 4, 1)
}

/// The first beat is half a second long, the second one second.
fn slowing() -> Result<Grid, Error> {
    grid(vec![0.0, 0.5, 1.5], 4, 0)
}

fn times(events: &[Event]) -> Vec<f64> {
    events.iter().map(Event::t).collect()
}

fn labels(events: &[Event]) -> Vec<&str> {
    events.iter().map(|event| event.label().unwrap_or("none")).collect()
}

/// The events of `grid` through the thing named `beats` in the registry: its input checked, its values read.
fn by_name(division: i64, grid: &Grid) -> Result<Vec<Event>, Error> {
    let call = Call {
        inputs: [("grid".to_owned(), Input::Grid(grid.clone()))].into(),
        values: json!({"division": division}),
        ..Call::default()
    };
    match Registry::builtin().get("beats")?.call(&call)? {
        Output::Events(events) => Ok(events),
        other => Err(Error::Call {
            message: format!("beats gave {other:?}"),
        }),
    }
}

/// The events of `grid` through the body alone, with the typed parameter.
fn by_body(division: i64, grid: &Grid) -> Result<Vec<Event>, Error> {
    match (Params { division }).run(grid.clone())? {
        Output::Events(events) => Ok(events),
        other => Err(Error::Call {
            message: format!("beats gave {other:?}"),
        }),
    }
}

/// Both ways of running it, which must agree.
fn made(division: i64, grid: &Grid) -> Result<Vec<Event>, Error> {
    let events = by_body(division, grid)?;
    assert_eq!(by_name(division, grid)?, events, "the door and the body agree");
    Ok(events)
}

#[test]
fn beats_gives_an_event_on_each_beat_labelled_downbeat_or_beat_each_at_full_strength() -> Result<(), Error> {
    let events = made(1, &steady()?)?;
    assert_eq!(times(&events), [0.0, 0.5, 1.0, 1.5, 2.0]);
    assert_eq!(labels(&events), ["beat", "downbeat", "beat", "beat", "beat"]);
    assert!(
        events
            .iter()
            .all(|event| event.strength().to_bits() == 1.0_f64.to_bits()
                && event.duration().is_none()
                && event.pitch().is_none())
    );
    Ok(())
}

#[test]
fn beats_with_a_division_adds_sub_events_between_the_beats() -> Result<(), Error> {
    let events = made(2, &steady()?)?;
    assert_eq!(times(&events), [0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]);
    assert_eq!(
        labels(&events),
        ["beat", "sub", "downbeat", "sub", "beat", "sub", "beat", "sub", "beat"]
    );
    Ok(())
}

#[test]
fn beats_puts_a_sub_halfway_in_beats_between_the_beats_it_lies_between() -> Result<(), Error> {
    // the first beat lasts 0.5 s and the second 1.0 s: the sub-beats are at 0.25 s and at 1.0 s
    let events = made(2, &slowing()?)?;
    assert_eq!(times(&events), [0.0, 0.25, 0.5, 1.0, 1.5]);
    assert_eq!(labels(&events), ["downbeat", "sub", "beat", "sub", "beat"]);
    Ok(())
}

#[test]
fn beats_with_a_division_that_does_not_halve_puts_the_subs_a_third_apart() -> Result<(), Error> {
    let events = made(3, &grid(vec![0.0, 0.6, 1.2], 2, 0)?)?;
    let expected = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2];
    assert!(
        times(&events)
            .iter()
            .zip(expected)
            .all(|(got, want)| (got - want).abs() <= 1e-12),
        "{:?}",
        times(&events)
    );
    assert_eq!(
        labels(&events),
        ["downbeat", "sub", "sub", "beat", "sub", "sub", "downbeat"]
    );
    Ok(())
}

/// `(id, meter, first_downbeat, labels of as many beats as the row has words)`.
#[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
const BARS: [(&str, u32, u32, &str); 5] = [
    (
        "a bar of 4 from the second beat",
        4,
        1,
        "beat downbeat beat beat beat downbeat beat beat beat",
    ),
    (
        "a bar of 3 from the third beat",
        3,
        2,
        "beat beat downbeat beat beat downbeat beat",
    ),
    (
        "a bar of 1: every beat is a downbeat",
        1,
        0,
        "downbeat downbeat downbeat downbeat",
    ),
    (
        "a bar of 2 from the first beat",
        2,
        0,
        "downbeat beat downbeat beat downbeat",
    ),
    (
        "the downbeat on the last beat there is",
        4,
        3,
        "beat beat beat downbeat",
    ),
];

#[test]
fn beats_labels_every_meter_th_beat_from_the_first_downbeat_a_downbeat_and_no_other_beat() -> Result<(), Error> {
    for (id, meter, first_downbeat, expected) in BARS {
        let words: Vec<&str> = expected.split(' ').collect();
        let beats = (0_u8..).take(words.len()).map(|index| f64::from(index) * 0.5).collect();
        assert_eq!(labels(&made(1, &grid(beats, meter, first_downbeat)?)?), words, "{id}");
    }
    Ok(())
}

#[test]
fn beats_has_the_count_of_beats_times_division_and_the_subs_between() -> Result<(), Error> {
    let events = made(64, &steady()?)?;
    assert_eq!(events.len(), 4 * 64 + 1);
    assert_eq!(labels(&events).iter().filter(|label| **label == "sub").count(), 4 * 63);
    assert_eq!(labels(&events).iter().filter(|label| **label == "downbeat").count(), 1);
    Ok(())
}

#[test]
fn a_division_beyond_its_range_is_refused_naming_it() -> Result<(), Error> {
    for (id, division) in [("0", 0), ("65", 65)] {
        let paths: Vec<String> = match by_name(division, &steady()?) {
            Err(Error::Params { shaper, problems, .. }) if shaper == "beats" => {
                problems.iter().map(|problem| problem.path.clone()).collect()
            }
            other => vec![format!("not beats' parameter error: {other:?}")],
        };
        assert_eq!(paths, ["division"], "{id}");
    }
    Ok(())
}

#[test]
fn beats_needs_its_grid_and_takes_it_as_a_grid() -> Result<(), Error> {
    let thing = Registry::builtin().get("beats")?;
    let missing = thing.call(&Call::default());
    assert!(
        matches!(&missing, Err(Error::Call { message }) if message.contains("beats") && message.contains("'grid'")),
        "{missing:?}"
    );
    assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Events));
    assert_eq!(thing.inputs(), [(InputKind::Grid, true)]);
    assert!(
        thing.summary().starts_with("Events at the grid's beats"),
        "{}",
        thing.summary()
    );
    Ok(())
}

proptest! {
    /// Beat times in 64ths of a second, which floats hold exactly.
    #[test]
    fn beats_lies_on_the_grid_and_its_downbeats_are_the_grids(
        first in 0_u32..=64,
        gaps in prop::collection::vec(1_u32..=64, 1..12),
        meter in 1_u32..=8,
        division in 1_i64..=8,
        pick in 0_u32..8,
    ) {
        let mut steps = vec![first];
        for gap in &gaps {
            steps.push(steps[steps.len() - 1] + gap);
        }
        let first_downbeat = pick % meter.min(u32::try_from(steps.len()).unwrap_or(1));
        let beats: Vec<f64> = steps.iter().map(|step| f64::from(*step) / 64.0).collect();
        let g = grid(beats.clone(), meter, first_downbeat).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let events = made(division, &g).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let step = usize::try_from(division).unwrap_or(1);
        prop_assert_eq!(events.len(), (steps.len() - 1) * step + 1);
        // the whole beats are the grid's own times
        prop_assert_eq!(times(&events).into_iter().step_by(step).collect::<Vec<_>>(), beats.clone());
        // strictly later each
        prop_assert!(events.windows(2).all(|pair| pair[0].t() < pair[1].t()));
        let on = |wanted: &[&str]| -> Vec<f64> {
            events.iter().filter(|event| wanted.contains(&event.label().unwrap_or(""))).map(Event::t).collect()
        };
        prop_assert_eq!(on(&["downbeat"]), g.downbeats());
        prop_assert_eq!(on(&["beat", "downbeat"]), beats);
        prop_assert!(events.iter().enumerate().all(|(index, event)| index % step == 0 || event.label() == Some("sub")));
    }
}

#[test]
fn events_are_made_with_the_defaults_of_an_event_but_a_label() -> Result<(), Error> {
    let expected = Event::new(
        0.5,
        EventOptions {
            label: Some("downbeat".into()),
            ..EventOptions::default()
        },
    )
    .map_err(|source| Error::Format { source })?;
    assert_eq!(made(1, &steady()?)?[1], expected);
    Ok(())
}
