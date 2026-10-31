//! The inputs' promises, checked through their public doors: `InputKind::check` over JSON (and, for NaN, a reader
//! that delivers floats as Python does), the typed `Input::from_*`, `Keyframe::from_deserializer` and `matching`.
//! Every expected path and rule is worked out by hand from the rules; none is computed by the code under test.

use std::error::Error as StdError;
use std::fmt::Debug;

use proptest::prelude::*;
use serde_json::{Value, json};

use super::{Input, InputKind, Keyframe, matching};
use crate::Error;
use crate::easing::Ease;
use crate::formats::{
    Event, EventOptions, Grid, Series, SeriesOptions, Unit, check_problems, list_of_floats, object_of_floats,
};

type Outcome = Result<(), Box<dyn StdError>>;

/// The problems wanted: each a path and a short part of its rule.
type Expected = &'static [(&'static str, &'static str)];

/// A case: its name, the data given, and what it is checked into or the problems wanted.
type Row<T> = (&'static str, Value, Result<T, Expected>);

/// Assert that `result` is `Ok(wanted)`, or, when `wanted` is `Err(expected)`, an `Error::Input` with exactly
/// those paths in that order, each rule containing its part.
fn verdict<T: PartialEq + Debug>(id: &str, result: Result<T, Error>, wanted: Result<T, Expected>) {
    match (result, wanted) {
        (Ok(got), Ok(want)) => assert_eq!(got, want, "{id}"),
        (Err(Error::Input { problems, .. }), Err(expected)) => check_problems(id, &problems, expected),
        // a mismatch of kind (accepted where refused, another error): the two never print alike
        (got, want) => assert_eq!(format!("{got:?}"), format!("{want:?}"), "{id}"),
    }
}

/// An event at `t` of `strength`, nothing else set.
fn event(t: f64, strength: f64) -> Result<Event, Box<dyn StdError>> {
    Ok(Event::new(
        t,
        EventOptions {
            strength,
            ..Default::default()
        },
    )?)
}

/// An event at `t` carrying `label`.
fn labelled(t: f64, label: &str) -> Result<Event, Box<dyn StdError>> {
    Ok(Event::new(
        t,
        EventOptions {
            label: Some(label.to_owned()),
            ..Default::default()
        },
    )?)
}

#[test]
fn a_keyframe_is_read_with_its_rules_and_every_broken_one_named() {
    let bezier = |x1, y1, x2, y2| Ease::Bezier { x1, y1, x2, y2 };
    #[rustfmt::skip]
    let rows: Vec<Row<Keyframe>> = vec![
        ("the ease is linear by default", json!({"t": 2.5, "value": -0.75}), Ok(Keyframe::at(2.5, -0.75))),
        ("a null ease is linear", json!({"t": 1, "value": 0, "ease": null}), Ok(Keyframe::at(1.0, 0.0))),
        ("time zero", json!({"t": 0, "value": 1}), Ok(Keyframe::at(0.0, 1.0))),
        ("a whole number for a decimal", json!({"t": 1, "value": 3}), Ok(Keyframe::at(1.0, 3.0))),
        ("a large negative value", json!({"t": 1, "value": -1e300}), Ok(Keyframe::at(1.0, -1e300))),
        ("the largest exact whole number", json!({"t": 9_007_199_254_740_992_u64, "value": 0}),
            Ok(Keyframe::at(9_007_199_254_740_992.0, 0.0))),
        ("the ease linear", json!({"t": 1, "value": 0, "ease": "linear"}), Ok(Keyframe::at(1.0, 0.0))),
        ("the ease hold", json!({"t": 1, "value": 0, "ease": "hold"}), Ok(Keyframe::at(1.0, 0.0).ease(Ease::Hold))),
        ("the ease in", json!({"t": 1, "value": 0, "ease": "in"}), Ok(Keyframe::at(1.0, 0.0).ease(Ease::In))),
        ("the ease out", json!({"t": 1, "value": 0, "ease": "out"}), Ok(Keyframe::at(1.0, 0.0).ease(Ease::Out))),
        ("the ease in-out", json!({"t": 1, "value": 0, "ease": "in-out"}),
            Ok(Keyframe::at(1.0, 0.0).ease(Ease::InOut))),
        ("the ease cubic-in", json!({"t": 1, "value": 0, "ease": "cubic-in"}),
            Ok(Keyframe::at(1.0, 0.0).ease(Ease::CubicIn))),
        ("the ease cubic-out", json!({"t": 1, "value": 0, "ease": "cubic-out"}),
            Ok(Keyframe::at(1.0, 0.0).ease(Ease::CubicOut))),
        ("the ease cubic-in-out", json!({"t": 1, "value": 0, "ease": "cubic-in-out"}),
            Ok(Keyframe::at(1.0, 0.0).ease(Ease::CubicInOut))),
        ("the ease overshoot", json!({"t": 1, "value": 0, "ease": "overshoot"}),
            Ok(Keyframe::at(1.0, 0.0).ease(Ease::Overshoot))),
        ("a bezier, its y outside 0..1", json!({"t": 4, "value": 1e300, "ease": "cubic-bezier(0.1, -2, 0.9, 3)"}),
            Ok(Keyframe::at(4.0, 1e300).ease(bezier(0.1, -2.0, 0.9, 3.0)))),
        ("negative time", json!({"t": -0.01, "value": 1}), Err(&[("t", "must be >= 0 (got -0.01)")])),
        ("time missing", json!({"value": 1}), Err(&[("t", "is required")])),
        ("time null", json!({"t": null, "value": 1}), Err(&[("t", "is required")])),
        ("time a bool", json!({"t": true, "value": 1}), Err(&[("t", "invalid type: boolean `true`")])),
        ("time a string", json!({"t": "1", "value": 1}), Err(&[("t", "invalid type: string \"1\"")])),
        ("time a whole number beyond 2^53", json!({"t": 9_007_199_254_740_993_u64, "value": 1}),
            Err(&[("t", "beyond 2^53")])),
        ("value missing", json!({"t": 1}), Err(&[("value", "is required")])),
        ("value a string", json!({"t": 1, "value": "high"}), Err(&[("value", "invalid type: string \"high\"")])),
        ("value a bool", json!({"t": 1, "value": false}), Err(&[("value", "invalid type: boolean `false`")])),
        ("unknown ease, the valid ones listed", json!({"t": 1, "value": 1, "ease": "bounce"}),
            Err(&[("ease", "unknown ease 'bounce': use linear, hold, in, out, in-out")])),
        ("ease not text", json!({"t": 1, "value": 1, "ease": 2}), Err(&[("ease", "invalid type: integer `2`")])),
        ("a bezier with x outside 0..1", json!({"t": 1, "value": 1, "ease": "cubic-bezier(1.5, 0, 0.5, 1)"}),
            Err(&[("ease", "x1 and x2 must be in 0..1 (got 1.5 and 0.5)")])),
        ("unknown field", json!({"t": 1, "value": 1, "colour": "red"}), Err(&[("colour", "unknown field")])),
        ("not an object", json!([1, 1]), Err(&[("", "invalid type: sequence")])),
        ("every rule broken, listed in field order", json!({"t": -1, "ease": "wobble"}),
            Err(&[("t", "must be >= 0 (got -1.0)"), ("value", "is required"), ("ease", "unknown ease 'wobble'")])),
    ];
    for (id, given, wanted) in rows {
        verdict(id, Keyframe::from_deserializer(&given), wanted);
    }
}

#[test]
fn a_keyframe_read_as_python_gives_it_refuses_nan_and_the_infinities() {
    // the fields of a keyframe, each delivered as a float
    type Floats = [(&'static str, f64); 2];
    #[rustfmt::skip]
    let rows: [(&str, Floats, Expected); 3] = [
        ("time NaN", [("t", f64::NAN), ("value", 1.0)], &[("t", "must be finite (got NaN)")]),
        ("time infinite", [("t", f64::INFINITY), ("value", 1.0)], &[("t", "must be finite (got Infinity)")]),
        ("value -infinity", [("t", 1.0), ("value", f64::NEG_INFINITY)], &[("value", "must be finite (got -Infinity)")]),
    ];
    for (id, fields, expected) in rows {
        verdict(
            id,
            Keyframe::from_deserializer(object_of_floats(&fields)),
            Err(expected),
        );
    }
}

#[test]
fn points_are_keyframes_whose_times_strictly_increase() {
    #[rustfmt::skip]
    let rows: Vec<Row<Vec<Keyframe>>> = vec![
        ("mappings become keyframes, the ease linear unless given",
            json!([{"t": 0.5, "value": 2.0, "ease": "in"}, {"t": 1.5, "value": -1.0}, {"t": 4, "value": 0.25, "ease": "out"}]),
            Ok(vec![Keyframe::at(0.5, 2.0).ease(Ease::In), Keyframe::at(1.5, -1.0), Keyframe::at(4.0, 0.25).ease(Ease::Out)])),
        ("a single point is enough", json!([{"t": 0, "value": 9.0}]), Ok(vec![Keyframe::at(0.0, 9.0)])),
        ("times a billionth apart still increase", json!([{"t": 1.0, "value": 0}, {"t": 1.000_000_001, "value": 0}]),
            Ok(vec![Keyframe::at(1.0, 0.0), Keyframe::at(1.000_000_001, 0.0)])),
        ("from zero", json!([{"t": 0, "value": 0}, {"t": 7, "value": 1}, {"t": 7.5, "value": 2}]),
            Ok(vec![Keyframe::at(0.0, 0.0), Keyframe::at(7.0, 1.0), Keyframe::at(7.5, 2.0)])),
        ("an empty list", json!([]), Err(&[("points", "needs at least one keyframe")])),
        ("a number", json!(5), Err(&[("points", "invalid type: integer `5`")])),
        ("a string", json!("0:1"), Err(&[("points", "invalid type: string \"0:1\"")])),
        ("nothing", json!(null), Err(&[("points", "invalid type: null")])),
        ("a mapping, not a list of them", json!({"t": 0, "value": 1}), Err(&[("points", "invalid type: map")])),
        ("an item that is not a mapping", json!([{"t": 0, "value": 1}, 3]),
            Err(&[("points[1]", "invalid type: integer `3`")])),
        ("an item that is a list", json!([[0, 1]]), Err(&[("points[0]", "invalid type: sequence")])),
        ("unsorted", json!([{"t": 0, "value": 1}, {"t": 5, "value": 1}, {"t": 2, "value": 1}]),
            Err(&[("points[2].t", "2.0 is not after the previous t = 5.0")])),
        ("a duplicate time", json!([{"t": 0, "value": 1}, {"t": 1, "value": 1}, {"t": 1, "value": 2}]),
            Err(&[("points[2].t", "1.0 is not after the previous t = 1.0")])),
        ("starting with a repeat", json!([{"t": 3, "value": 1}, {"t": 3, "value": 1}]),
            Err(&[("points[1].t", "3.0 is not after the previous t = 3.0")])),
        ("every pair out of order is listed",
            json!([{"t": 0, "value": 0}, {"t": 5, "value": 0}, {"t": 2, "value": 0}, {"t": 1, "value": 0}]),
            Err(&[("points[2].t", "2.0 is not after the previous t = 5.0"), ("points[3].t", "1.0 is not after the previous t = 2.0")])),
        ("a bad ease at index 1", json!([{"t": 0, "value": 1}, {"t": 1, "value": 1, "ease": "wobble"}]),
            Err(&[("points[1].ease", "unknown ease 'wobble'")])),
        ("an unknown field at index 1", json!([{"t": 0, "value": 1}, {"t": 1, "value": 1, "x": 1}]),
            Err(&[("points[1].x", "unknown field")])),
        ("a negative time at index 0", json!([{"t": -1, "value": 1}, {"t": 1, "value": 1}]),
            Err(&[("points[0].t", "must be >= 0 (got -1.0)")])),
        ("every bad item is listed", json!([{"t": 0, "value": "a"}, {"t": 1, "value": 1, "ease": "wobble"}]),
            Err(&[("points[0].value", "invalid type: string \"a\""), ("points[1].ease", "unknown ease 'wobble'")])),
        ("the order of t comes with t, before the ease",
            json!([{"t": 1, "value": 0}, {"t": 0.5, "value": 1, "ease": "wobble"}]),
            Err(&[("points[1].t", "0.5 is not after the previous t = 1.0"), ("points[1].ease", "unknown ease")])),
        ("an item that could not be read is compared with neither neighbour",
            json!([{"t": 5, "value": 0}, {"t": "x", "value": 0}, {"t": 1, "value": 0}]),
            Err(&[("points[1].t", "invalid type: string \"x\"")])),
        ("an item without t is compared with neither neighbour",
            json!([{"t": 5, "value": 0}, {"value": 0}, {"t": 1, "value": 0}]),
            Err(&[("points[1].t", "is required")])),
    ];
    for (id, given, wanted) in rows {
        verdict(id, InputKind::Points.check(&given), wanted.map(Input::Points));
    }
}

#[test]
fn typed_points_are_held_to_the_same_rules() {
    #[rustfmt::skip]
    let rows: Vec<(&str, Vec<Keyframe>, Expected)> = vec![
        ("increasing, eased", vec![Keyframe::at(0.0, 1.0), Keyframe::at(2.0, 0.0).ease(Ease::Out)], &[]),
        ("none", vec![], &[("points", "needs at least one keyframe")]),
        ("a NaN time", vec![Keyframe::at(f64::NAN, 0.0)], &[("points[0].t", "must be finite (got NaN)")]),
        ("an infinite value", vec![Keyframe::at(0.0, f64::INFINITY)], &[("points[0].value", "must be finite (got Infinity)")]),
        ("a negative time", vec![Keyframe::at(-0.5, 0.0)], &[("points[0].t", "must be >= 0 (got -0.5)")]),
        ("an equal time", vec![Keyframe::at(1.0, 0.0), Keyframe::at(1.0, 2.0)],
            &[("points[1].t", "1.0 is not after the previous t = 1.0")]),
        ("-0 and 0 are the same time", vec![Keyframe::at(-0.0, 0.0), Keyframe::at(0.0, 1.0)],
            &[("points[1].t", "0.0 is not after the previous t = -0.0")]),
        ("a NaN time is compared with neither neighbour",
            vec![Keyframe::at(5.0, 0.0), Keyframe::at(f64::NAN, 0.0), Keyframe::at(1.0, 0.0)],
            &[("points[1].t", "must be finite (got NaN)")]),
    ];
    for (id, given, expected) in rows {
        let wanted = if expected.is_empty() {
            Ok(Input::Points(given.clone()))
        } else {
            Err(expected)
        };
        verdict(id, Input::from_points(given), wanted);
    }
}

#[test]
fn events_arrive_sorted_by_time_and_equal_times_keep_their_order() -> Outcome {
    let (kick, snare, hat) = (event(0.5, 0.3)?, event(1.0, 0.8)?, event(2.0, 0.45)?);
    let (first_of_two, second_of_two) = (event(1.0, 0.2)?, event(1.0, 0.9)?);
    let note = Event::new(
        1.25,
        EventOptions {
            strength: 0.7,
            duration: Some(0.5),
            label: Some("snare".into()),
            pitch: Some(38.0),
        },
    )?;
    let kick_json = json!({"t": 0.5, "strength": 0.3});
    let snare_json = json!({"t": 1.0, "strength": 0.8});
    let hat_json = json!({"t": 2.0, "strength": 0.45});
    let (first_json, second_json) = (json!({"t": 1.0, "strength": 0.2}), json!({"t": 1.0, "strength": 0.9}));
    #[rustfmt::skip]
    let rows: Vec<(&str, Value, Vec<Event>)> = vec![
        ("in order", json!([kick_json, snare_json, hat_json]), vec![kick.clone(), snare.clone(), hat.clone()]),
        ("every field read",
            json!([kick_json, {"t": 1.25, "strength": 0.7, "duration": 0.5, "label": "snare", "pitch": 38}]),
            vec![kick.clone(), note]),
        ("defaults filled in", json!([{"t": 3}]), vec![Event::new(3.0, EventOptions::default())?]),
        ("none at all", json!([]), vec![]),
        ("unsorted", json!([hat_json, kick_json, snare_json]), vec![kick.clone(), snare, hat]),
        ("equal times keep their order, weak first", json!([first_json, second_json]),
            vec![first_of_two.clone(), second_of_two.clone()]),
        ("equal times keep their order, strong first", json!([second_json, first_json]),
            vec![second_of_two.clone(), first_of_two.clone()]),
        ("equal times beside an earlier one", json!([first_json, kick_json, second_json]),
            vec![kick, first_of_two, second_of_two]),
    ];
    for (id, given, wanted) in rows {
        verdict(id, InputKind::Events.check(&given), Ok(Input::Events(wanted)));
    }
    // -0 and 0 are the same time: a tie, kept in the order given
    let (zero, negative_zero) = (event(0.0, 0.2)?, event(-0.0, 0.9)?);
    assert_eq!(
        Input::from_events(vec![zero.clone(), negative_zero.clone()]),
        Input::Events(vec![zero, negative_zero])
    );
    Ok(())
}

#[test]
fn bad_events_are_refused_naming_each_path() {
    let kick = json!({"t": 0.5, "strength": 0.3});
    #[rustfmt::skip]
    let rows: Vec<(&str, Value, Expected)> = vec![
        ("a string", json!("0.5, 1.0"), &[("events", "invalid type: string")]),
        ("a mapping, not a list of them", json!({"t": 0.5}), &[("events", "invalid type: map")]),
        ("nothing", json!(null), &[("events", "invalid type: null")]),
        ("an item that is not a mapping", json!([kick, 3]), &[("events[1]", "invalid type: integer `3`")]),
        ("a strength above 1 at index 1", json!([kick, {"t": 1, "strength": 1.5}]),
            &[("events[1].strength", "must be in [0, 1] (got 1.5)")]),
        ("a negative strength", json!([{"t": 1, "strength": -0.1}]), &[("events[0].strength", "must be in [0, 1] (got -0.1)")]),
        ("an unknown field at index 0", json!([{"t": 1, "colour": "red"}, kick]), &[("events[0].colour", "unknown field")]),
        ("a bool time", json!([{"t": true}]), &[("events[0].t", "invalid type: boolean `true`")]),
        ("a missing time", json!([kick, {"strength": 0.5}]), &[("events[1].t", "is required")]),
        ("a negative time at index 1", json!([kick, {"t": -0.25}]), &[("events[1].t", "must be >= 0 (got -0.25)")]),
        ("a zero duration", json!([{"t": 1, "duration": 0}]), &[("events[0].duration", "must be > 0 (got 0.0)")]),
        ("an empty label", json!([kick, {"t": 1, "label": ""}]), &[("events[1].label", "must not be empty")]),
        ("a pitch above 127", json!([{"t": 1, "pitch": 128}]), &[("events[0].pitch", "must be in [0, 127] (got 128.0)")]),
        ("every broken item is listed",
            json!([{"t": -1, "strength": 0.5}, kick, {"t": 1, "strength": 2}, {"t": 2, "duration": 0}]),
            &[("events[0].t", ">= 0"), ("events[2].strength", "[0, 1]"), ("events[3].duration", "> 0")]),
    ];
    for (id, given, expected) in rows {
        verdict(id, InputKind::Events.check(&given), Err(expected));
    }
}

#[test]
fn a_grid_is_read_as_the_format_reads_it_with_problems_under_grid() -> Outcome {
    let steady = || json!({"beats": [0.0, 0.5, 1.0, 1.5], "meter": 4, "first_downbeat": 0});
    let with = |field: &str, value: Value| {
        let mut grid = steady();
        grid[field] = value;
        grid
    };
    verdict(
        "a mapping becomes a grid",
        InputKind::Grid.check(json!({"beats": [0.0, 0.5, 1.1], "meter": 3, "first_downbeat": 1})),
        Ok(Input::Grid(Grid::new(vec![0.0, 0.5, 1.1], 3, 1)?)),
    );
    let mut meter_missing = steady();
    meter_missing.as_object_mut().map(|grid| grid.remove("meter"));
    #[rustfmt::skip]
    let rows: Vec<(&str, Value, Expected)> = vec![
        ("a number", json!(5), &[("grid", "invalid type: integer `5`")]),
        ("a string", json!("0.5, 1.0"), &[("grid", "invalid type: string")]),
        ("nothing", json!(null), &[("grid", "invalid type: null")]),
        ("a list of beats, not a grid", json!([0.0, 0.5, 1.0]), &[("grid", "invalid type: sequence")]),
        ("meter missing", meter_missing, &[("grid.meter", "is required")]),
        ("a single beat", with("beats", json!([0.0])), &[("grid.beats", "needs at least 2 beats")]),
        ("beats not increasing", with("beats", json!([0.0, 1.0, 0.5])),
            &[("grid.beats[2]", "0.5 does not come after beats[1] = 1.0")]),
        ("a negative beat", with("beats", json!([-0.5, 0.5, 1.0])), &[("grid.beats[0]", "must be >= 0 (got -0.5)")]),
        ("a bool beat", with("beats", json!([0.0, true])), &[("grid.beats[1]", "invalid type: boolean")]),
        ("a downbeat outside the bar", with("first_downbeat", json!(4)), &[("grid.first_downbeat", "inside a bar")]),
        ("an unknown field", with("tempo", json!(120)), &[("grid.tempo", "unknown field")]),
    ];
    for (id, given, expected) in rows {
        verdict(id, InputKind::Grid.check(&given), Err(expected));
    }
    Ok(())
}

#[test]
fn a_series_is_read_as_the_format_reads_it_with_problems_under_series() -> Outcome {
    let wanted = Series::new(
        4.0,
        vec![0.0, 1.0],
        Unit::Ratio,
        SeriesOptions {
            start: 0.5,
            ..Default::default()
        },
    )?;
    verdict(
        "a mapping becomes a series",
        InputKind::Series.check(json!({"start": 0.5, "rate": 4, "values": [0.0, 1.0], "unit": "ratio"})),
        Ok(Input::Series(wanted)),
    );
    #[rustfmt::skip]
    let rows: Vec<(&str, Value, Expected)> = vec![
        ("a number", json!(5), &[("series", "invalid type: integer `5`")]),
        ("a string", json!("0.1, 0.2"), &[("series", "invalid type: string")]),
        ("nothing", json!(null), &[("series", "invalid type: null")]),
        ("a list of values, not a series", json!([0.1, 0.2]), &[("series", "invalid type: sequence")]),
        ("rate missing", json!({"values": [0.1], "unit": "ratio"}), &[("series.rate", "is required")]),
        ("rate zero", json!({"rate": 0, "values": [0.1], "unit": "ratio"}), &[("series.rate", "must be > 0 (got 0.0)")]),
        ("no values", json!({"rate": 1, "values": [], "unit": "ratio"}), &[("series.values", "needs at least one value")]),
        ("a bool value", json!({"rate": 1, "values": [0.1, true], "unit": "ratio"}),
            &[("series.values[1]", "invalid type: boolean")]),
        ("an unknown unit", json!({"rate": 1, "values": [0.1], "unit": "pixels"}), &[("series.unit", "pixels")]),
        ("a fraction without scaling", json!({"rate": 1, "values": [0.1], "unit": "fraction"}),
            &[("series.scaling", "is required when unit is \"fraction\"")]),
        ("a fraction above 1", json!({"rate": 1, "values": [0.1, 1.5], "unit": "fraction", "scaling": "x"}),
            &[("series.values[1]", "must be in [0, 1]")]),
        ("a negative start", json!({"start": -1, "rate": 1, "values": [0.1], "unit": "ratio"}),
            &[("series.start", "must be >= 0 (got -1.0)")]),
        ("an unknown field", json!({"rate": 1, "values": [0.1], "unit": "ratio", "gain": 2}), &[("series.gain", "unknown field")]),
    ];
    for (id, given, expected) in rows {
        verdict(id, InputKind::Series.check(&given), Err(expected));
    }
    Ok(())
}

#[test]
fn values_are_a_non_empty_list_of_finite_numbers() {
    #[rustfmt::skip]
    let rows: Vec<Row<Vec<f64>>> = vec![
        ("decimals", json!([0.25, -1.5, 3.0]), Ok(vec![0.25, -1.5, 3.0])),
        ("whole numbers become decimals", json!([1, 2, -3]), Ok(vec![1.0, 2.0, -3.0])),
        ("a single value", json!([0.5]), Ok(vec![0.5])),
        ("large and small", json!([1e300, -1e-300]), Ok(vec![1e300, -1e-300])),
        ("the largest exact whole number", json!([9_007_199_254_740_992_u64]), Ok(vec![9_007_199_254_740_992.0])),
        ("an empty list", json!([]), Err(&[("values", "needs at least one value")])),
        ("a number", json!(5), Err(&[("values", "invalid type: integer `5`")])),
        ("a string", json!("1, 2, 3"), Err(&[("values", "invalid type: string")])),
        ("nothing", json!(null), Err(&[("values", "invalid type: null")])),
        ("a mapping", json!({"a": 1.0}), Err(&[("values", "invalid type: map")])),
        ("a bool item", json!([0.5, true]), Err(&[("values[1]", "invalid type: boolean `true`")])),
        ("a string item", json!([0.5, "high"]), Err(&[("values[1]", "invalid type: string \"high\"")])),
        ("a null item", json!([0.5, null]), Err(&[("values[1]", "invalid type: null")])),
        ("an item that is a list", json!([[1.0], 2.0]), Err(&[("values[0]", "invalid type: sequence")])),
        ("a whole number beyond 2^53", json!([1, 9_007_199_254_740_993_u64]), Err(&[("values[1]", "beyond 2^53")])),
        ("the largest 64-bit integer", json!([u64::MAX]), Err(&[("values[0]", "beyond 2^53")])),
        ("every bad item is listed", json!([true, 1.0, "x"]),
            Err(&[("values[0]", "invalid type: boolean"), ("values[2]", "invalid type: string")])),
    ];
    for (id, given, wanted) in rows {
        verdict(id, InputKind::Values.check(&given), wanted.map(Input::Values));
    }
}

#[test]
fn values_read_as_python_gives_them_refuse_nan_and_the_infinities() {
    verdict(
        "a NaN item",
        InputKind::Values.check(list_of_floats(&[0.0, 1.0, f64::NAN])),
        Err(&[("values[2]", "must be finite (got NaN)")]),
    );
    verdict(
        "an infinite item",
        InputKind::Values.check(list_of_floats(&[f64::NEG_INFINITY, 1.0])),
        Err(&[("values[0]", "must be finite (got -Infinity)")]),
    );
    verdict(
        "finite floats",
        InputKind::Values.check(list_of_floats(&[0.5, -2.0])),
        Ok(Input::Values(vec![0.5, -2.0])),
    );
}

#[test]
fn typed_values_are_held_to_the_same_rules() {
    #[rustfmt::skip]
    let rows: Vec<(&str, Vec<f64>, Expected)> = vec![
        ("finite", vec![0.5, -3.0], &[]),
        ("none", vec![], &[("values", "needs at least one value")]),
        ("an infinity among them", vec![1.0, f64::INFINITY, -2.0], &[("values[1]", "must be finite (got Infinity)")]),
        ("every NaN listed", vec![f64::NAN, f64::NAN], &[("values[0]", "finite (got NaN)"), ("values[1]", "finite (got NaN)")]),
    ];
    for (id, given, expected) in rows {
        let wanted = if expected.is_empty() {
            Ok(Input::Values(given.clone()))
        } else {
            Err(expected)
        };
        verdict(id, Input::from_values(given), wanted);
    }
}

#[test]
fn a_refusal_keeps_the_first_100_problems_and_counts_the_rest() {
    let repeated = |item: Value, count: usize| Value::Array(vec![item; count]);
    // (id, kind, data, problems found in all): each bad event and each bool value is one problem; each keyframe at
    // t = -1 is one (below 0), and each after the first a second (not after the one before it)
    #[rustfmt::skip]
    let rows = [
        ("100 bad events: all kept", InputKind::Events, repeated(json!({"t": -1}), 100), 100),
        ("101 bad events: one counted", InputKind::Events, repeated(json!({"t": -1}), 101), 101),
        ("250 bad events", InputKind::Events, repeated(json!({"t": -1}), 250), 250),
        ("250 values of the wrong type", InputKind::Values, repeated(json!(true), 250), 250),
        ("250 keyframes at -1 s", InputKind::Points, repeated(json!({"t": -1, "value": 0}), 250), 1 + 2 * 249),
    ];
    for (id, kind, given, found) in rows {
        let result = kind.check(&given);
        let Err(error @ Error::Input { problems, omitted }) = &result else {
            assert!(result.is_err(), "{id}: accepted");
            assert_eq!(format!("{result:?}"), "an input error", "{id}");
            return;
        };
        assert_eq!((problems.len(), *omitted), (100, found - 100), "{id}");
        let first = &problems[0].path;
        assert!(first.ends_with("[0]") || first.ends_with("[0].t"), "{id}: {first}");
        let text = error.to_string();
        if found == 100 {
            assert!(!text.contains("more"), "{id}: {text}");
        } else {
            assert!(
                text.ends_with(&format!("; ... and {} more", found - 100)),
                "{id}: {text}"
            );
        }
    }
}

#[test]
fn every_kind_has_a_name_that_parses_back_to_it() {
    #[rustfmt::skip]
    let kinds = [
        ("points", InputKind::Points), ("events", InputKind::Events), ("grid", InputKind::Grid),
        ("values", InputKind::Values), ("series", InputKind::Series),
    ];
    assert_eq!(InputKind::NAMES, kinds.map(|(name, _)| name));
    for (name, kind) in kinds {
        assert_eq!(name.parse::<InputKind>().ok(), Some(kind), "{name}");
        assert_eq!((kind.name(), kind.to_string().as_str()), (name, name));
    }
}

#[test]
fn a_name_that_is_no_kind_is_refused_naming_the_kinds() {
    for name in ["keyframes", "", "Points", " points", "event"] {
        let result = name.parse::<InputKind>();
        let Err(Error::Input { problems, .. }) = &result else {
            assert_eq!(format!("{result:?}"), "an input error", "{name:?}");
            return;
        };
        let rules: Vec<&str> = problems.iter().map(|problem| problem.rule.as_str()).collect();
        let wanted = format!("unknown input '{name}': use points, events, grid, values or series");
        assert_eq!(rules, [wanted.as_str()], "{name:?}");
    }
}

#[test]
fn each_doc_says_what_its_input_is() {
    #[rustfmt::skip]
    let rows = [
        (InputKind::Points, "Keyframes: a list of {t, value, ease}"),
        (InputKind::Events, "Events: a list of {t, strength, duration, label, pitch}"),
        (InputKind::Grid, "A beat grid: {beats, meter, first_downbeat}"),
        (InputKind::Values, "Values: a non-empty list of numbers"),
        (InputKind::Series, "A series: {start, rate, values, unit, scaling}"),
    ];
    for (kind, start) in rows {
        assert!(kind.doc().starts_with(start), "{kind}: {}", kind.doc());
    }
    let points = InputKind::Points.doc();
    for ease in [
        "linear",
        "hold",
        "in",
        "out",
        "in-out",
        "cubic-in",
        "cubic-out",
        "cubic-in-out",
        "overshoot",
        "cubic-bezier(x1, y1, x2, y2)",
    ] {
        assert!(points.contains(ease), "{ease}: {points}");
    }
}

#[test]
fn matching_keeps_the_events_with_the_label_in_their_order() -> Outcome {
    let strong_kick = Event::new(
        1.1,
        EventOptions {
            label: Some("kick".into()),
            strength: 0.4,
            ..Default::default()
        },
    )?;
    let events = [
        labelled(0.5, "kick")?,
        labelled(0.7, "snare")?,
        event(0.9, 1.0)?,
        strong_kick.clone(),
    ];
    let times = |label: &str| matching(&events, label).iter().map(Event::t).collect::<Vec<_>>();
    assert_eq!(
        times(""),
        [0.5, 0.7, 0.9, 1.1],
        "no label: every event, the unlabelled one too"
    );
    assert_eq!(times("kick"), [0.5, 1.1], "a label: only the events carrying it");
    assert_eq!(times("snare"), [0.7]);
    assert_eq!(times("hat"), Vec::<f64>::new(), "a label nothing has: none");
    assert_eq!(matching(&events, "kick")[1], strong_kick, "every other field is kept");
    Ok(())
}

proptest! {
    /// Events come out sorted by time, the same events as went in, and those at one time in the order given: the
    /// order a bucket per time gives (each time's events, in input order, the times ascending).
    #[test]
    fn events_are_sorted_stably(slots in prop::collection::vec(0_u8..4, 0..80)) {
        let given: Vec<Value> = slots
            .iter()
            .enumerate()
            .map(|(index, &slot)| json!({"t": f64::from(slot) * 0.5, "label": format!("e{index}")}))
            .collect();
        let mut wanted = Vec::new();
        for slot in 0_u8..4 {
            wanted.extend(slots.iter().enumerate().filter(|&(_, &s)| s == slot).map(|(index, _)| format!("e{index}")));
        }
        let Ok(Input::Events(from_data)) = InputKind::Events.check(Value::Array(given)) else {
            return Err(TestCaseError::fail("events refused"));
        };
        let labels = |events: &[Event]| events.iter().map(|event| event.label().unwrap_or("").to_owned()).collect::<Vec<_>>();
        prop_assert_eq!(labels(&from_data), wanted.clone());
        let mut reversed = from_data.clone();
        reversed.reverse();
        let Input::Events(typed) = Input::from_events(reversed) else {
            return Err(TestCaseError::fail("not events"));
        };
        // reversed, each time's events come out in reversed order
        let mut wanted_reversed = Vec::new();
        for slot in 0_u8..4 {
            let mut bucket: Vec<String> =
                slots.iter().enumerate().filter(|&(_, &s)| s == slot).map(|(index, _)| format!("e{index}")).collect();
            bucket.reverse();
            wanted_reversed.extend(bucket);
        }
        prop_assert_eq!(labels(&typed), wanted_reversed);
    }

    /// Points are accepted exactly when every `t` is above the one before it; otherwise each `t` that is not is named.
    #[test]
    fn points_name_each_time_not_after_the_one_before(slots in prop::collection::vec(0_u8..6, 1..30)) {
        let points: Vec<Keyframe> = slots.iter().map(|&slot| Keyframe::at(f64::from(slot), 0.0)).collect();
        let wanted: Vec<String> = (1..slots.len())
            .filter(|&index| slots[index] <= slots[index - 1])
            .map(|index| format!("points[{index}].t"))
            .collect();
        match Input::from_points(points.clone()) {
            Ok(input) => {
                prop_assert!(wanted.is_empty(), "accepted {:?}", slots);
                prop_assert_eq!(input, Input::Points(points));
            }
            Err(Error::Input { problems, .. }) => {
                let paths: Vec<String> = problems.into_iter().map(|problem| problem.path).collect();
                prop_assert_eq!(paths, wanted);
            }
            Err(other) => return Err(TestCaseError::fail(format!("{other:?}"))),
        }
    }

    /// Finite values come back as given, from typed values and from JSON alike.
    #[test]
    fn finite_values_come_back_as_given(
        values in prop::collection::vec(prop::num::f64::NORMAL | prop::num::f64::SUBNORMAL | prop::num::f64::ZERO, 1..20)
    ) {
        prop_assert_eq!(Input::from_values(values.clone()).ok(), Some(Input::Values(values.clone())));
        let given = Value::Array(values.iter().map(|&value| json!(value)).collect());
        prop_assert_eq!(InputKind::Values.check(&given).ok(), Some(Input::Values(values)));
    }
}
