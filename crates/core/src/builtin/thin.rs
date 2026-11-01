//! The events, from the earliest on, that are at least `gap` seconds after the last one kept.
//!
//! The earliest event is always kept; a later one is kept when its time minus the time of the last kept event is at
//! least `gap`, so a dropped event never moves the reference. Of events at the same time the first given is kept (for
//! a `gap` above 0). Every field of a kept event is unchanged. "At least `gap`" holds to within float rounding (a
//! nanosecond), so 0.1 and 0.3 are a gap of 0.2 apart, though 0.3 - 0.1 is a hair under it in floats.

use crate::formats::Event;
use crate::registry::{Output, Returns};
use crate::rounding::at_least;
use crate::{Error, Operation};

/// The shortest time allowed between two kept events.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The least time between two kept events, in seconds, 0 to 3600: an event is kept when it comes at least this
    /// long after the last kept one. Raising it keeps fewer events; 0 keeps all.
    #[param(default = 0.1, unit = Seconds, min = 0, max = 3600)]
    pub gap: f64,
}

impl Operation for Params {
    const NAME: &'static str = "thin";
    type Inputs = Vec<Event>;
    const RETURNS: Returns = Returns::Events;

    /// The events of `events` from the earliest on that are at least `gap` seconds after the last one kept, each as
    /// it was. The door gives `events` in time order (those at one time as given); the first of them is the earliest.
    ///
    /// ```
    /// use bullterrier_control::builtin::thin;
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::Operation;
    ///
    /// let at = |t| Event::new(t, EventOptions::default());
    /// let events = vec![at(0.5)?, at(0.6)?, at(0.75)?];
    /// // 0.6 is 0.1 after 0.5, too soon; 0.75 is exactly 0.25 after it
    /// let thinned = thin::Params { gap: 0.25 }.run(events)?;
    /// assert!(matches!(thinned, Output::Events(kept) if kept == [at(0.5)?, at(0.75)?]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None: every set of events can be thinned.
    fn run(&self, events: Vec<Event>) -> Result<Output, Error> {
        let mut kept: Vec<Event> = Vec::new();
        for event in events {
            if kept.last().is_none_or(|last| at_least(event.t() - last.t(), self.gap)) {
                kept.push(event);
            }
        }
        Ok(Output::Events(kept))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::Params;
    use crate::formats::{Event, EventOptions};
    use crate::inputs::{Input, InputKind};
    use crate::registry::{Call, Kind, Output, Returns};
    use crate::{Error, Operation, Registry};

    fn event(t: f64, options: EventOptions) -> Result<Event, Error> {
        Event::new(t, options).map_err(|source| Error::Format { source })
    }

    fn moments(times: &[f64]) -> Result<Vec<Event>, Error> {
        times.iter().map(|t| event(*t, EventOptions::default())).collect()
    }

    fn times(events: &[Event]) -> Vec<f64> {
        events.iter().map(Event::t).collect()
    }

    /// `events` through the thing named `thin` in the registry: its input checked, its values read.
    fn by_name(values: serde_json::Value, events: &[Event]) -> Result<Vec<Event>, Error> {
        let call = Call {
            inputs: [("events".to_owned(), Input::from_events(events.to_vec()))].into(),
            values,
            ..Call::default()
        };
        match Registry::builtin().get("thin")?.call(&call)? {
            Output::Events(kept) => Ok(kept),
            other => Err(Error::Call { message: format!("thin gave {other:?}") }),
        }
    }

    /// `events` through the body alone, with the typed parameter.
    fn by_body(gap: f64, events: &[Event]) -> Result<Vec<Event>, Error> {
        match (Params { gap }).run(events.to_vec())? {
            Output::Events(kept) => Ok(kept),
            other => Err(Error::Call { message: format!("thin gave {other:?}") }),
        }
    }

    /// `(id, gap, times given, times kept)`: the rows of `tests/builtin/test_thin.py`, then the nanosecond edge of
    /// "at least": 0.5e-9 under the gap is the gap, 2e-9 under is not.
    #[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
    const KEEPS: [(&str, f64, &[f64], &[f64]); 16] = [
        ("0.6 is 0.1 after 0.5; 0.76 is 0.26 after it; 1.0 is 0.24 after 0.76; 1.05 is 0.29 after", 0.25, &[0.5, 0.6, 0.76, 1.0, 1.05], &[0.5, 0.76, 1.05]),
        ("a distance of exactly the gap keeps the event", 0.25, &[0.5, 0.75, 1.0, 1.25], &[0.5, 0.75, 1.0, 1.25]),
        ("a hair under the gap drops it", 0.25, &[0.5, 0.7499], &[0.5]),
        ("the distance is from the last kept event, not the last seen: 0.7 is 0.2 after 0.5", 0.25, &[0.5, 0.6, 0.7, 0.8, 0.9], &[0.5, 0.8]),
        ("a gap of 0 keeps all, same time included", 0.0, &[1.0, 1.0, 1.5], &[1.0, 1.0, 1.5]),
        ("two at the same time: the second is 0 after the first", 0.25, &[1.0, 1.0], &[1.0]),
        ("the default gap is 0.1 s", 0.1, &[0.5, 0.55, 0.625, 0.75], &[0.5, 0.625, 0.75]),
        ("the longest gap", 3600.0, &[0.0, 3599.0, 3600.0], &[0.0, 3600.0]),
        ("decimals: 0.3 - 0.1 is 0.19999999999999998, which is the gap", 0.2, &[0.1, 0.3], &[0.1, 0.3]),
        ("decimals: 1.3 - 1.1 is a hair under the gap", 0.2, &[1.1, 1.3], &[1.1, 1.3]),
        ("decimals: three events a tenth apart", 0.1, &[0.1, 0.2, 0.3], &[0.1, 0.2, 0.3]),
        ("decimals: 1e-6 short of the gap is dropped", 0.2, &[0.5, 0.699_999], &[0.5]),
        ("decimals: 1e-7 past the gap is kept", 0.2, &[0.5, 0.700_000_1], &[0.5, 0.700_000_1]),
        ("half a nanosecond short of the gap is the gap", 0.25, &[0.5, 0.749_999_999_5], &[0.5, 0.749_999_999_5]),
        ("two nanoseconds short of the gap is not", 0.25, &[0.5, 0.749_999_998], &[0.5]),
        ("no events keep none", 0.25, &[], &[]),
    ];

    #[test]
    fn thin_keeps_an_event_when_it_is_at_least_gap_after_the_last_one_kept() -> Result<(), Error> {
        for (id, gap, given, expected) in KEEPS {
            assert_eq!(times(&by_body(gap, &moments(given)?)?), expected, "{id}: the body");
            assert_eq!(times(&by_name(json!({"gap": gap}), &moments(given)?)?), expected, "{id}: the door");
        }
        Ok(())
    }

    #[test]
    fn given_no_values_thin_uses_a_gap_of_a_tenth() -> Result<(), Error> {
        assert_eq!(times(&by_name(json!({}), &moments(&[0.5, 0.55, 0.625, 0.75])?)?), [0.5, 0.625, 0.75]);
        Ok(())
    }

    #[test]
    fn thin_keeps_the_first_of_events_at_the_same_time_and_every_field_of_what_it_keeps() -> Result<(), Error> {
        let loud = event(1.0, EventOptions { strength: 0.9, label: Some("a".into()), pitch: Some(40.0), ..EventOptions::default() })?;
        let soft = event(1.0, EventOptions { strength: 0.2, label: Some("b".into()), ..EventOptions::default() })?;
        assert_eq!(by_body(0.5, &[loud.clone(), soft.clone()])?, std::slice::from_ref(&loud));
        assert_eq!(by_name(json!({"gap": 0.5}), &[soft.clone(), loud])?, [soft]);
        Ok(())
    }


    /// 0.0 and -0.0 are one time: the door keeps the order given, `a` then `b`, and so does the operation.
    fn tied() -> Result<[Event; 2], Error> {
        let named = |t: f64, name: &str| event(t, EventOptions { label: Some(name.into()), ..EventOptions::default() });
        Ok([named(0.0, "a")?, named(-0.0, "b")?])
    }

    #[test]
    fn thin_keeps_the_first_given_of_events_at_zero_and_minus_zero() -> Result<(), Error> {
        let [a, b] = tied()?;
        assert_eq!(by_name(json!({"gap": 0.5}), &[a.clone(), b.clone()])?, std::slice::from_ref(&a));
        assert_eq!(by_name(json!({"gap": 0.5}), &[b.clone(), a])?, std::slice::from_ref(&b));
        Ok(())
    }

    #[test]
    fn thin_works_in_time_order_whatever_the_order_given() -> Result<(), Error> {
        let shuffled = moments(&[1.05, 0.5, 1.0, 0.76, 0.6])?;
        assert_eq!(times(&by_name(json!({"gap": 0.25}), &shuffled)?), [0.5, 0.76, 1.05]);
        Ok(())
    }

    #[test]
    fn a_gap_beyond_its_range_is_refused_naming_it() -> Result<(), Error> {
        for (id, gap) in [("negative", -0.1), ("above 3600", 3600.5)] {
            let paths: Vec<String> = match by_name(json!({"gap": gap}), &moments(&[0.5])?) {
                Err(Error::Params { shaper, problems, .. }) if shaper == "thin" => {
                    problems.iter().map(|problem| problem.path.clone()).collect()
                }
                other => vec![format!("not thin's parameter error: {other:?}")],
            };
            assert_eq!(paths, ["gap"], "{id}");
        }
        Ok(())
    }

    #[test]
    fn it_is_an_operation_giving_events_from_events() -> Result<(), Error> {
        let thing = Registry::builtin().get("thin")?;
        assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Events));
        assert_eq!(thing.inputs(), [(InputKind::Events, true)]);
        assert!(thing.summary().starts_with("The events, from the earliest on"), "{}", thing.summary());
        Ok(())
    }

    proptest! {
        /// Times and the gap are multiples of 1/64, which floats hold exactly: "at least the gap" is decided exactly.
        #[test]
        fn thin_leaves_events_a_gap_apart_and_drops_only_those_that_were_too_close(
            steps in prop::collection::vec(0_u16..=256, 0..30),
            gap_steps in 0_u16..=128,
        ) {
            let gap = f64::from(gap_steps) / 64.0;
            // the label is the position, so events at equal times can be told apart
            let events: Vec<Event> = steps
                .iter()
                .enumerate()
                .map(|(index, step)| event(f64::from(*step) / 64.0, EventOptions { label: Some(index.to_string()), ..EventOptions::default() }))
                .collect::<Result<_, _>>()
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let mut ordered = events.clone();
            ordered.sort_by(|a, b| a.t().total_cmp(&b.t()));
            let kept = by_name(json!({"gap": gap}), &events).map_err(|error| TestCaseError::fail(error.to_string()))?;
            // the first event is always kept
            prop_assert_eq!(kept.first(), ordered.first());
            // each kept event is a gap after every earlier one kept
            for (index, later) in kept.iter().enumerate() {
                for earlier in &kept[..index] {
                    prop_assert!(later.t() - earlier.t() >= gap);
                }
            }
            // each dropped event is closer than the gap to the last one kept before it
            let mut last = None::<&Event>;
            for candidate in &ordered {
                if kept.contains(candidate) {
                    last = Some(candidate);
                } else {
                    let before = last.map(Event::t).ok_or_else(|| TestCaseError::fail("dropped before any kept"))?;
                    prop_assert!(candidate.t() - before < gap);
                }
            }
        }
    }
}
