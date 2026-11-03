//! The built-in `latch` through its public face.

use serde_json::json;

use bullterrier_control::Error;
use bullterrier_control::formats::Event;
use bullterrier_control::formats::EventOptions;
use bullterrier_control::inputs::Input;
use bullterrier_control::registry::{Call, Output};
use bullterrier_control::{Registry, builtin::latch, lib};

fn event(t: f64, label: Option<&str>) -> Result<Event, Error> {
    let options = EventOptions {
        label: label.map(str::to_owned),
        ..EventOptions::default()
    };
    Event::new(t, options).map_err(|source| Error::Format { source })
}

/// 0.2 before and 0.7 after: distinct, so a swapped pair or a wrong level fails.
fn levels(label: &str) -> latch::Params {
    latch::Params {
        label: label.to_owned(),
        before: 0.2,
        after: 0.7,
    }
}

#[test]
fn it_is_before_until_the_event_and_after_from_exactly_its_time_on() -> Result<(), Error> {
    let latched = lib::latch(&[event(12.0, None)?], levels(""))?;
    let times = [0.0, 11.0, 12.0_f64.next_down(), 12.0, 12.0_f64.next_up(), 30.0, 1e6];
    assert_eq!(latched.sample(&times)?, [0.2, 0.2, 0.2, 0.7, 0.7, 0.7, 0.7]);
    Ok(())
}

#[test]
fn the_label_chooses_the_events_that_count_and_the_earliest_of_them_switches() -> Result<(), Error> {
    // "kick" at 2.5 s, unlabelled at 5 s, "drop" at 12 s, given late first: "every event" switches at the kick
    let events = [event(12.0, Some("drop"))?, event(5.0, None)?, event(2.5, Some("kick"))?];
    let reads = [2.0, 4.0, 8.0, 15.0];
    for (label, expected) in [
        ("drop", [0.2, 0.2, 0.2, 0.7]),
        ("kick", [0.2, 0.7, 0.7, 0.7]),
        ("", [0.2, 0.7, 0.7, 0.7]),
        ("hat", [0.2, 0.2, 0.2, 0.2]), // a label nothing has: never switches
    ] {
        assert_eq!(
            lib::latch(&events, levels(label))?.sample(&reads)?,
            expected,
            "{label:?}"
        );
    }
    Ok(())
}

#[test]
fn with_no_events_it_is_before_forever_and_with_one_at_0_after_from_the_start() -> Result<(), Error> {
    assert_eq!(
        lib::latch(&[], levels(""))?.sample(&[0.0, 12.0, 1e300])?,
        [0.2, 0.2, 0.2]
    );
    assert_eq!(
        lib::latch(&[event(0.0, None)?], levels(""))?.sample(&[0.0, 1.0])?,
        [0.7, 0.7]
    );
    Ok(())
}

#[test]
fn through_the_registry_it_switches_and_refuses_a_level_beyond_its_range_naming_it() -> Result<(), Error> {
    let thing = Registry::builtin().get("latch")?;
    let call = |values| -> Result<Call, Error> {
        Ok(Call {
            inputs: [("events".to_owned(), Input::from_events(vec![event(1.0, None)?]))].into(),
            values,
            ..Call::default()
        })
    };
    let Output::Signal(latched) = thing.call(&call(json!({"before": 1.0, "after": 0.0}))?)? else {
        return Err(Error::Call {
            message: "not a signal".into(),
        });
    };
    assert_eq!(latched.sample(&[0.5, 1.0])?, [1.0, 0.0]); // an inverted latch
    assert_eq!(latched.recipe().map(|recipe| recipe.name.as_str()), Some("latch"));
    let refused = thing.call(&call(json!({"after": 1.5}))?);
    assert!(
        matches!(&refused, Err(Error::Params { shaper, problems, .. })
            if shaper == "latch" && problems.len() == 1 && problems[0].path == "after"),
        "{refused:?}"
    );
    Ok(())
}
