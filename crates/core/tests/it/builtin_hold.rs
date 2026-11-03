//! The built-in `hold` through its public face.

use proptest::prelude::*;
use serde_json::json;

use bullterrier_control::Registry;
use bullterrier_control::builtin::hold::*;
use bullterrier_control::formats::Event;
use bullterrier_control::formats::EventOptions;
use bullterrier_control::inputs::Input;
use bullterrier_control::registry::{Call, Output};
use bullterrier_control::{Error, Shaper, Signal};

/// The input at time t is t + 2, so a held value names the time it was read at (2 at time 0, 3.5 at 1.5 s).
fn clock() -> Signal {
    let time = Signal::from_fn(|times, out| {
        out.copy_from_slice(times);
        Ok::<(), std::fmt::Error>(())
    });
    time + 2.0
}

fn events(times: &[f64]) -> Result<Vec<Event>, Error> {
    times
        .iter()
        .map(|&t| Event::new(t, EventOptions::default()).map_err(|source| Error::Format { source }))
        .collect()
}

#[test]
fn the_value_is_the_input_at_the_latest_event_at_or_before_the_read() -> Result<(), Error> {
    let held = clock().hold(&events(&[1.0, 2.5])?)?;
    // (time, expected): the input at 0 (2) before the first event, at 1 (3) from it, at 2.5 (4.5) from the second
    let rows = [
        (0.0, 2.0),
        (1.0_f64.next_down(), 2.0), // the instant before the first event
        (1.0, 3.0),                 // at an event: already its value
        (2.0, 3.0),
        (2.5_f64.next_down(), 3.0),
        (2.5, 4.5),
        (9.0, 4.5), // after the last event it holds for good
    ];
    let times: Vec<f64> = rows.iter().map(|(time, _)| *time).collect();
    let expected: Vec<f64> = rows.iter().map(|(_, value)| *value).collect();
    assert_eq!(held.sample(&times)?, expected);
    Ok(())
}

#[test]
fn with_no_events_or_one_at_0_every_read_is_the_input_at_0() -> Result<(), Error> {
    for given in [events(&[])?, events(&[0.0])?] {
        assert_eq!(
            clock().hold(&given)?.sample(&[0.0, 1.0, 50.0])?,
            [2.0, 2.0, 2.0],
            "{given:?}"
        );
    }
    Ok(())
}

#[test]
fn the_order_and_the_repeats_of_the_events_change_nothing_whichever_door_takes_them() -> Result<(), Error> {
    let reads = [0.9, 1.0, 3.9, 4.0, 7.0];
    // 2 before 1 s, 3 from 1 s, 6 from 4 s
    let expected = [2.0, 3.0, 3.0, 6.0, 6.0];
    let shuffled = events(&[4.0, 1.0, 1.0])?;
    // the body given them as they are, unsorted
    assert_eq!(Params.shape(&clock(), shuffled.clone())?.sample(&reads)?, expected);
    let thing = Registry::builtin().get("hold")?;
    let call = Call {
        signals: vec![clock()],
        inputs: [("events".to_owned(), Input::from_events(shuffled))].into(),
        values: json!({}),
    };
    let Output::Signal(held) = thing.call(&call)? else {
        return Err(Error::Call {
            message: "not a signal".into(),
        });
    };
    assert_eq!(held.sample(&reads)?, expected);
    assert_eq!(held.recipe().map(|recipe| recipe.name.as_str()), Some("hold"));
    Ok(())
}

#[test]
fn without_its_events_the_registry_refuses_the_call_naming_them() -> Result<(), Error> {
    let refused = Registry::builtin().get("hold")?.call(&Call {
        signals: vec![clock()],
        ..Call::default()
    });
    assert!(
        matches!(&refused, Err(Error::Call { message }) if message.contains("events")),
        "{refused:?}"
    );
    Ok(())
}

proptest! {
    #[test]
    fn it_agrees_with_a_plain_loop_over_the_events(
        at in prop::collection::vec(0.0_f64..=10.0, 0..8),
        reads in prop::collection::vec(0.0_f64..=10.0, 1..12),
    ) {
        // reads at, and a hair before, every event as well: the inclusive edge is where an off-by-one hides
        let reads: Vec<f64> = reads.into_iter().chain(at.iter().copied()).chain(at.iter().map(|t| t.next_down())).map(|t| t.max(0.0)).collect();
        let expected: Vec<f64> = reads
            .iter()
            .map(|read| 2.0 + at.iter().copied().filter(|time| time <= read).fold(0.0, f64::max))
            .collect();
        let held = events(&at)
            .and_then(|given| clock().hold(&given))
            .and_then(|held| held.sample(&reads))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(held, expected);
    }
}
