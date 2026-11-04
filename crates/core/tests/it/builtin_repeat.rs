//! The built-in `repeat` through its public face.

use proptest::prelude::*;
use serde_json::json;

use bullterrier_control::builtin::repeat::{self, Mode};
use bullterrier_control::cadence::Per;
use bullterrier_control::formats::Grid;
use bullterrier_control::registry::{Call, Output};
use bullterrier_control::{Error, FromValues, Interpolation, Registry, Signal};

/// Reads as its time from 0 to 60 s (a value a second, straight between), then holds 60.
fn clock() -> Result<Signal, Error> {
    let seconds: Vec<f64> = (0..=60).map(f64::from).collect();
    Signal::from_values(&seconds, 1.0, FromValues::default(), Interpolation::Linear)
}

#[test]
fn through_the_registry_it_repeats_and_a_length_outside_its_range_is_refused_naming_it() -> Result<(), Error> {
    let thing = Registry::builtin().get("repeat")?;
    let input = clock()?;
    let call = |length: f64| Call {
        signals: vec![input.clone()],
        values: json!({ "length": length, "mode": "ping-pong" }),
        ..Call::default()
    };
    let Output::Signal(swung) = thing.call(&call(2.0))? else {
        return Err(Error::Call {
            message: "not a signal".into(),
        });
    };
    // forward to 2 s, back to 0 at 4 s, forward again
    assert_eq!(swung.sample(&[1.0, 3.0, 4.0, 5.0])?, [1.0, 1.0, 0.0, 1.0]);
    assert_eq!(
        swung
            .recipe()
            .map(|recipe| (recipe.name.as_str(), recipe.values.clone())),
        Some(("repeat", json!({ "length": 2.0, "mode": "ping-pong", "per": "second" })))
    );
    for length in [0.0, 3601.0] {
        let refused = thing.call(&call(length));
        assert!(
            matches!(&refused, Err(Error::Params { shaper, problems, .. })
                if shaper == "repeat" && problems.len() == 1 && problems[0].path == "length"),
            "{length}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn per_beat_without_a_grid_is_refused_naming_per() -> Result<(), Error> {
    let refused = clock()?.repeat(
        repeat::Params {
            per: Per::Beat,
            ..Default::default()
        },
        None,
    );
    assert!(
        matches!(&refused, Err(Error::Params { problems, .. }) if problems.iter().any(|problem| problem.path == "per")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_grid_sets_the_length_in_beats_and_follows_the_tempo() -> Result<(), Error> {
    // beats of 0.5, 0.6 and 0.7 s: two beats are 0.5 + 0.6 = 1.1 s here, so a read at 1.1 s is back at time 0
    let grid = Grid::new(vec![0.0, 0.5, 1.1, 1.8], 4, 0).map_err(|error| Error::Call {
        message: error.to_string(),
    })?;
    let params = repeat::Params {
        length: 2.0,
        per: Per::Beat,
        ..Default::default()
    };
    let repeated = clock()?.repeat(params, Some(&grid))?.sample(&[0.25, 1.1, 1.45, 1.8])?;
    assert!(
        repeated
            .iter()
            .zip([0.25, 0.0, 0.25, 0.5])
            .all(|(got, want)| (got - want).abs() <= 1e-12),
        "{repeated:?}"
    );
    Ok(())
}

proptest! {
    #[test]
    fn a_wrap_never_reads_later_than_it_is_read_and_comes_back_every_length(
        length in 0.5_f64..=20.0,
        times in prop::collection::vec(0.0_f64..=500.0, 1..20),
    ) {
        let params = repeat::Params { length, ..Default::default() };
        let wrapped = clock().and_then(|input| input.repeat(params, None)).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let read = wrapped.sample(&times).map_err(|error| TestCaseError::fail(error.to_string()))?;
        for (time, value) in times.iter().zip(&read) {
            prop_assert!(*value <= time + 1e-9 && (0.0..length + 1e-9).contains(value), "{time}: {value}");
        }
        let later: Vec<f64> = times.iter().map(|time| time + length).collect();
        let again = wrapped.sample(&later).map_err(|error| TestCaseError::fail(error.to_string()))?;
        for (first, second) in read.iter().zip(&again) {
            prop_assert!((first - second).abs() <= 1e-6, "{first} {second}");
        }
    }

    #[test]
    fn a_ping_pong_stays_inside_its_length_and_meets_itself_at_the_turn(length in 0.5_f64..=20.0, offset in 0.0_f64..=1.0) {
        let params = repeat::Params { length, mode: Mode::PingPong, ..Default::default() };
        let swung = clock().and_then(|input| input.repeat(params, None)).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let distance = offset * length;
        let read = swung.sample(&[length - distance, length + distance]).map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert!((read[0] - read[1]).abs() <= 1e-9, "{read:?}");
    }
}
