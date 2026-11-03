//! The built-in `steps` through its public face.

#![allow(
    clippy::float_cmp,
    reason = "a step reads one of the values exactly, never one near it"
)]

use proptest::prelude::*;
use serde_json::json;

use bullterrier_control::builtin::steps::*;
use bullterrier_control::cadence::Per;
use bullterrier_control::formats::Grid;
use bullterrier_control::formats::Problem;
use bullterrier_control::inputs::Input;
use bullterrier_control::registry::{Call, Kind, Output};
use bullterrier_control::{Error, Signal, Source};
use bullterrier_control::{Registry, builtin::steps, lib};

fn grid(beats: &[f64], meter: u32) -> Result<Grid, Error> {
    Grid::new(beats.to_vec(), meter, 0).map_err(|source| Error::Format { source })
}

/// Beats of 0.5, 0.6 and 0.7 s.
fn slowing() -> Result<Grid, Error> {
    grid(&[0.0, 0.5, 1.1, 1.8], 4)
}

/// Beats of 0.5 s from 1.0 s on: time 0 is beat -2.
fn late() -> Result<Grid, Error> {
    grid(&[1.0, 1.5, 2.0], 1)
}

/// (row, the values, the parameters, the grid, the times read, the values they read)
type Row<'a> = (
    &'a str,
    &'a [f64],
    steps::Params,
    Option<&'a Grid>,
    &'a [f64],
    &'a [f64],
);

fn per_second(rate: f64) -> steps::Params {
    steps::Params { rate, per: Per::Second }
}

fn per_beat(rate: f64) -> steps::Params {
    steps::Params { rate, per: Per::Beat }
}

// each expected value worked out by hand: index floor(rate * t) mod len, per beat of the grid's beat lengths
#[test]
fn each_step_holds_its_value_and_the_list_goes_round() -> Result<(), Error> {
    let (slowing, late) = (slowing()?, late()?);
    let rows: [Row<'_>; 8] = [
        (
            "one a second, a step starting exactly at its time",
            &[1.0, 2.0, 3.0],
            per_second(1.0),
            None,
            &[0.0, 0.5, 1.0_f64.next_down(), 1.0, 1.5, 2.5, 3.0, 3.5, 4.5],
            &[1.0, 1.0, 1.0, 2.0, 2.0, 3.0, 1.0, 1.0, 2.0],
        ),
        (
            "four a second: a step is 0.25 s",
            &[0.5, -1.0, 2.0, 7.0],
            per_second(4.0),
            None,
            &[0.1, 0.3, 0.6, 0.8, 1.1],
            &[0.5, -1.0, 2.0, 7.0, 0.5],
        ),
        (
            "slow: a step is 1000 s",
            &[1.0, 2.0, 3.0],
            per_second(0.001),
            None,
            &[999.0, 1500.0, 2500.0, 3000.0],
            &[1.0, 2.0, 3.0, 1.0],
        ),
        (
            "one value is constant",
            &[0.75],
            per_second(3.0),
            None,
            &[0.0, 0.2, 1.7, 1e6, 1e300],
            &[0.75; 5],
        ),
        (
            "repeated values are steps of their own",
            &[5.0, 5.0, 6.0],
            per_second(1.0),
            None,
            &[0.5, 1.5, 2.5, 3.5],
            &[5.0, 5.0, 6.0, 5.0],
        ),
        (
            "a grid is ignored per second",
            &[1.0, 2.0, 3.0],
            per_second(1.0),
            Some(&slowing),
            &[0.5, 1.5, 2.5],
            &[1.0, 2.0, 3.0],
        ),
        (
            "a step a beat, whatever the beat's length: beats at 0.5, 1.1, 1.8 s, then every 0.7 s",
            &[1.0, 2.0, 3.0],
            per_beat(1.0),
            Some(&slowing),
            &[0.25, 0.5, 0.8, 1.1, 1.45, 1.79, 1.8, 2.6, 3.3],
            &[1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 1.0, 2.0, 3.0],
        ),
        (
            "before a first beat at 1 s the list runs backwards: 0 s is beat -2, 0.75 s beat -0.5",
            &[10.0, 20.0, 30.0],
            per_beat(1.0),
            Some(&late),
            &[0.0, 0.5, 0.75, 1.0, 1.5],
            &[20.0, 30.0, 30.0, 10.0, 20.0],
        ),
    ];
    for (row, values, params, grid, times, expected) in rows {
        assert_eq!(lib::steps(values, params, grid)?.sample(times)?, expected, "{row}");
    }
    Ok(())
}

#[test]
fn two_steps_a_beat_split_each_beat_in_halves() -> Result<(), Error> {
    // beat 0 is 0 to 0.5 s, beat 1 0.5 to 1.1 s: halves start at 0, 0.25, 0.5, 0.8 and 1.1 s
    let signal = lib::steps(&[1.0, 2.0, 3.0, 4.0], per_beat(2.0), Some(&slowing()?))?;
    assert_eq!(signal.sample(&[0.1, 0.3, 0.6, 0.9, 1.2])?, [1.0, 2.0, 3.0, 4.0, 1.0]);
    Ok(())
}

#[test]
fn far_times_pick_the_step_exact_integers_say_or_are_refused() -> Result<(), Error> {
    let signal = lib::steps(&[1.0, 2.0, 3.0], per_second(1.0), None)?;
    // in exact integers: 10^12 mod 3 = 1; int(1e300) mod 3 = 0; int(1.7976931348623157e308) mod 3 = 2
    assert_eq!(
        signal.sample(&[1e12, 1e12 + 0.5, 1e300, f64::MAX])?,
        [2.0, 2.0, 1.0, 3.0]
    );
    // at 1000 a second, f64::MAX seconds is beyond the largest float: refused, naming the time
    for per in [Per::Second, Per::Beat] {
        let fast = lib::steps(&[1.0, 2.0], steps::Params { rate: 1000.0, per }, Some(&slowing()?))?;
        let refused = fast.sample(&[1.797e308]);
        assert!(matches!(&refused, Err(Error::Signal { .. })), "{per:?}: {refused:?}");
    }
    Ok(())
}

#[test]
fn a_time_before_0_reads_as_0_and_a_value_keeps_its_sign_of_zero() -> Result<(), Error> {
    let signal = lib::steps(&[-0.0, 0.0], per_second(1.0), None)?;
    let read = signal.sample(&[-1.0, -0.0, 0.0, 5e-324, 1.0, 1.5])?;
    let signs: Vec<bool> = read.iter().map(|value| value.is_sign_negative()).collect();
    assert_eq!(read, [0.0; 6]);
    assert_eq!(signs, [true, true, true, true, false, false]);
    Ok(())
}

/// The problems of a refusal of inputs, or what came instead.
fn input_problems<T: std::fmt::Debug>(made: Result<T, Error>) -> Result<Vec<Problem>, String> {
    match made {
        Err(Error::Input { problems, .. }) => Ok(problems),
        other => Err(format!("{other:?}")),
    }
}

#[test]
fn no_values_are_refused_when_made_as_the_door_refuses_them_even_called_directly() {
    let direct = input_problems(Params::default().source(Inputs {
        values: vec![],
        grid: None,
    }));
    let door = input_problems(lib::steps(&[], per_second(1.0), None));
    let rule = Problem {
        path: "values".to_owned(),
        rule: "needs at least one value".to_owned(),
    };
    assert_eq!((direct, door), (Ok(vec![rule.clone()]), Ok(vec![rule])));
}

#[test]
fn per_beat_without_a_grid_is_refused_naming_per_when_made() {
    let refused = lib::steps(&[1.0, 2.0], per_beat(1.0), None).map(|_| ());
    assert!(
        matches!(&refused, Err(Error::Params { problems, .. }) if problems.len() == 1 && problems[0].path == "per"),
        "{refused:?}"
    );
}

#[test]
fn through_the_registry_it_steps_records_its_inputs_and_refuses_what_is_wrong() -> Result<(), Error> {
    let thing = Registry::builtin().get("steps")?;
    assert_eq!(thing.kind(), Kind::Source);
    let values = Input::from_values(vec![1.0, 2.0, 3.0])?;
    let call = |values: serde_json::Value, inputs: Vec<(&str, Input)>| Call {
        inputs: inputs
            .into_iter()
            .map(|(name, input)| (name.to_owned(), input))
            .collect(),
        values,
        ..Call::default()
    };
    let made = thing.call(&call(
        json!({"rate": 1, "per": "beat"}),
        vec![("values", values.clone()), ("grid", Input::Grid(slowing()?))],
    ))?;
    let Output::Signal(stepped) = made else {
        return Err(Error::Call {
            message: "not a signal".into(),
        });
    };
    assert_eq!(stepped.sample(&[0.25, 0.8, 1.45])?, [1.0, 2.0, 3.0]);
    let recorded: Vec<&str> = stepped
        .recipe()
        .map(|recipe| recipe.inputs.iter().map(|(name, _)| name.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(recorded, ["values", "grid"]);
    // the grid may be left out per second; values may not
    let Output::Signal(free) = thing.call(&call(json!({}), vec![("values", values.clone())]))? else {
        return Err(Error::Call {
            message: "not a signal".into(),
        });
    };
    assert_eq!(free.sample(&[0.1, 0.3, 0.6, 0.8])?, [1.0, 2.0, 3.0, 1.0]); // the defaults: 4 a second
    let without_values = thing.call(&call(json!({}), vec![("grid", Input::Grid(slowing()?))]));
    assert!(
        matches!(&without_values, Err(Error::Call { message }) if message.contains("values")),
        "{without_values:?}"
    );
    let too_fast = thing.call(&call(json!({"rate": 1000.5}), vec![("values", values)]));
    assert!(
        matches!(&too_fast, Err(Error::Params { shaper, problems, .. })
            if shaper == "steps" && problems.len() == 1 && problems[0].path == "rate"),
        "{too_fast:?}"
    );
    Ok(())
}

#[test]
fn lib_records_both_inputs_and_only_the_grid_it_was_given() -> Result<(), Error> {
    let names = |signal: &Signal| -> Vec<String> {
        signal
            .recipe()
            .map(|recipe| recipe.inputs.iter().map(|(name, _)| name.clone()).collect())
            .unwrap_or_default()
    };
    assert_eq!(
        names(&lib::steps(&[1.0], per_second(1.0), Some(&slowing()?))?),
        ["values", "grid"]
    );
    assert_eq!(names(&lib::steps(&[1.0], per_second(1.0), None)?), ["values"]);
    let refused = lib::steps(&[f64::NAN], per_second(1.0), None).map(|_| ());
    assert!(
        matches!(&refused, Err(Error::Input { problems, .. }) if problems[0].path.starts_with("values")),
        "{refused:?}"
    );
    Ok(())
}

proptest! {
    #[test]
    fn every_read_is_one_of_the_values(
        values in prop::collection::vec(-1e6..1e6_f64, 1..8),
        rate in 0.001..1000.0_f64,
        time in 0.0..1e15_f64,
    ) {
        let read = lib::steps(&values, per_second(rate), None)?.sample(&[time])?;
        prop_assert!(values.contains(&read[0]), "{read:?} of {values:?}");
    }
}
