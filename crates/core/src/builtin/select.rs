//! The events with `label` (all if it is empty) whose strength is at least `threshold`, in time order.
//!
//! A strength equal to the threshold is kept. Every other field of a kept event is unchanged.

use crate::formats::Event;
use crate::inputs::matching;
use crate::registry::{Output, Returns};
use crate::{Error, Operation};

/// Which events to keep: by label and by strength.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Keep only events with this label; empty keeps every event, unlabelled ones too.
    #[param(default = "")]
    pub label: String,
    /// The weakest strength kept, 0 to 1: an event is kept when its strength is at least this. Raising it keeps only
    /// the stronger events; 0 keeps them all.
    #[param(default = 0.0, unit = Fraction, min = 0, max = 1)]
    pub threshold: f64,
}

impl Operation for Params {
    const NAME: &'static str = "select";
    type Inputs = Vec<Event>;
    const RETURNS: Returns = Returns::Events;

    /// The events of `events` with the label (any, if it is empty) and a strength of at least the threshold, in time
    /// order (events at one time in the order given), each as it was.
    ///
    /// ```
    /// use bullterrier_control::builtin::select;
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::Operation;
    ///
    /// let kick = |t, strength| {
    ///     Event::new(t, EventOptions { strength, label: Some("kick".into()), ..Default::default() })
    /// };
    /// let events = vec![kick(1.0, 0.5)?, kick(0.5, 0.25)?, Event::new(0.75, EventOptions::default())?];
    /// let loud = select::Params { label: "kick".into(), threshold: 0.5 };
    /// // a strength equal to the threshold is kept
    /// assert!(matches!(loud.run(events)?, Output::Events(kept) if kept == [kick(1.0, 0.5)?]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None: every set of events can be selected from.
    fn run(&self, events: Vec<Event>) -> Result<Output, Error> {
        let mut kept = matching(&events, &self.label);
        kept.retain(|event| event.strength() >= self.threshold);
        // stable: events at one time keep their order (a door gives them sorted already; the body does not rely on it)
        kept.sort_by(|a, b| a.t().total_cmp(&b.t()));
        Ok(Output::Events(kept))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::formats::EventOptions;
    use crate::inputs::Input;
    use crate::registry::{Call, Kind};
    use crate::{Registry, builtin::select};

    fn event(t: f64, strength: f64, label: Option<&str>) -> Result<Event, Error> {
        let options = EventOptions {
            strength,
            label: label.map(str::to_owned),
            ..EventOptions::default()
        };
        Event::new(t, options).map_err(|source| Error::Format { source })
    }

    /// The events of the Python tests as `(time, strength, label)`: 0.5 is the threshold the rows use, so a `>`
    /// where `>=` belongs shows.
    fn drums() -> Result<Vec<Event>, Error> {
        [
            (0.5, 0.9, Some("kick")),
            (0.7, 0.45, Some("kick")),
            (1.0, 0.8, None),
            (1.2, 0.5, Some("snare")),
            (1.5, 0.0, Some("hat")),
        ]
        .into_iter()
        .map(|(t, strength, label)| event(t, strength, label))
        .collect()
    }

    fn times(events: &[Event]) -> Vec<f64> {
        events.iter().map(Event::t).collect()
    }

    /// `events` selected by the thing named `select`, through the registry: its input checked, its values read.
    fn by_name(values: serde_json::Value, events: &[Event]) -> Result<Vec<Event>, Error> {
        let call = Call {
            inputs: [("events".to_owned(), Input::from_events(events.to_vec()))].into(),
            values,
            ..Call::default()
        };
        match Registry::builtin().get("select")?.call(&call)? {
            Output::Events(kept) => Ok(kept),
            other => Err(Error::Call { message: format!("select gave {other:?}") }),
        }
    }

    /// `events` selected by the body alone, with the typed parameters.
    fn by_body(label: &str, threshold: f64, events: &[Event]) -> Result<Vec<Event>, Error> {
        match (select::Params { label: label.to_owned(), threshold }).run(events.to_vec())? {
            Output::Events(kept) => Ok(kept),
            other => Err(Error::Call { message: format!("select gave {other:?}") }),
        }
    }

    /// `(id, label, threshold, the times kept)`: the rows of `tests/builtin/test_select.py`, then the edges of them.
    #[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
    const KEEPS: [(&str, &str, f64, &[f64]); 14] = [
        ("the defaults keep every event: unlabelled, and of strength 0", "", 0.0, &[0.5, 0.7, 1.0, 1.2, 1.5]),
        ("a strength equal to the threshold is kept; 0.45 and 0 are not", "", 0.5, &[0.5, 1.0, 1.2]),
        ("a strength equal to a high threshold is kept", "", 0.9, &[0.5]),
        ("above every strength: none", "", 0.95, &[]),
        ("a label keeps the events carrying it", "kick", 0.0, &[0.5, 0.7]),
        ("another label", "snare", 0.0, &[1.2]),
        ("both: a kick of at least 0.5", "kick", 0.5, &[0.5]),
        ("a label whose only event has strength 0", "hat", 0.0, &[1.5]),
        ("a label nothing has: none", "bell", 0.0, &[]),
        ("an empty label keeps the unlabelled one", "", 0.8, &[0.5, 1.0]),
        ("a label is matched exactly, case too", "Kick", 0.0, &[]),
        ("a label that is part of another is not it", "kic", 0.0, &[]),
        ("a threshold of 1 keeps nothing here", "", 1.0, &[]),
        ("a label and a threshold equal to its event's strength", "snare", 0.5, &[1.2]),
    ];

    #[test]
    fn through_the_registry_select_keeps_the_events_that_match_and_reach_the_threshold() -> Result<(), Error> {
        for (id, label, threshold, expected) in KEEPS {
            let kept = by_name(json!({"label": label, "threshold": threshold}), &drums()?)?;
            assert_eq!(times(&kept), expected, "{id}");
        }
        Ok(())
    }

    #[test]
    fn the_body_alone_keeps_the_same_events() -> Result<(), Error> {
        for (id, label, threshold, expected) in KEEPS {
            assert_eq!(times(&by_body(label, threshold, &drums()?)?), expected, "{id}");
        }
        Ok(())
    }

    #[test]
    fn given_no_values_select_keeps_every_event() -> Result<(), Error> {
        assert_eq!(by_name(json!({}), &drums()?)?, drums()?);
        Ok(())
    }

    #[test]
    fn select_keeps_every_field_of_the_events_it_keeps() -> Result<(), Error> {
        let note = Event::new(
            0.5,
            EventOptions { strength: 0.7, duration: Some(0.25), label: Some("lead".into()), pitch: Some(64.5) },
        )
        .map_err(|source| Error::Format { source })?;
        let events = [note.clone(), event(0.6, 0.1, None)?];
        assert_eq!(by_name(json!({"threshold": 0.5}), &events)?, std::slice::from_ref(&note));
        assert_eq!(by_body("", 0.5, &events)?, [note]);
        Ok(())
    }

    #[test]
    fn select_gives_events_in_time_order_whatever_the_order_given() -> Result<(), Error> {
        let mut backwards = drums()?;
        backwards.reverse();
        assert_eq!(times(&by_name(json!({"threshold": 0.5}), &backwards)?), [0.5, 1.0, 1.2]);
        assert_eq!(times(&by_body("", 0.5, &backwards)?), [0.5, 1.0, 1.2]);
        Ok(())
    }

    #[test]
    fn select_keeps_events_at_the_same_time_in_the_order_given() -> Result<(), Error> {
        let (first, second) = (event(1.0, 0.6, Some("a"))?, event(1.0, 0.9, Some("b"))?);
        for given in [[second.clone(), first.clone()], [first.clone(), second.clone()]] {
            assert_eq!(by_name(json!({}), &given)?, given);
            assert_eq!(by_body("", 0.0, &given)?, given);
        }
        Ok(())
    }

    #[test]
    fn many_events_at_two_times_keep_their_order_within_each() -> Result<(), Error> {
        // 300 events alternating between t = 2 and t = 1, each told apart by its duration (i + 1 ms): long enough that
        // a sort which is not stable would shuffle them
        let given: Vec<Event> = (0..300_u32)
            .map(|i| {
                let options = EventOptions { duration: Some(f64::from(i + 1) / 1000.0), ..EventOptions::default() };
                Event::new(if i % 2 == 0 { 2.0 } else { 1.0 }, options).map_err(|source| Error::Format { source })
            })
            .collect::<Result<_, _>>()?;
        let (late, early): (Vec<Event>, Vec<Event>) = given.iter().cloned().partition(|event| event.t() > 1.5);
        let expected: Vec<Event> = early.into_iter().chain(late).collect();
        assert_eq!(by_body("", 0.0, &given)?, expected);
        assert_eq!(by_name(json!({}), &given)?, expected);
        Ok(())
    }

    #[test]
    fn select_of_no_events_is_no_events() -> Result<(), Error> {
        assert_eq!(by_name(json!({"threshold": 0.5}), &[])?, []);
        assert_eq!(by_body("", 0.5, &[])?, []);
        Ok(())
    }

    /// `(id, values, the parameter it names)`.
    const BAD: [(&str, &str, &str); 5] = [
        ("a threshold above 1", r#"{"threshold": 1.5}"#, "threshold"),
        ("a threshold below 0", r#"{"threshold": -0.1}"#, "threshold"),
        ("a label that is not text", r#"{"label": 3}"#, "label"),
        ("a threshold that is not a number", r#"{"threshold": "high"}"#, "threshold"),
        ("a parameter it does not have", r#"{"lable": "kick"}"#, "lable"),
    ];

    #[test]
    fn select_refuses_a_bad_parameter_naming_it() -> Result<(), Error> {
        for (id, values, named) in BAD {
            let values: serde_json::Value = serde_json::from_str(values).map_err(|error| Error::Call { message: error.to_string() })?;
            let refused = by_name(values, &drums()?);
            let paths: Vec<String> = match &refused {
                Err(Error::Params { shaper, problems, .. }) if shaper == "select" => {
                    problems.iter().map(|problem| problem.path.clone()).collect()
                }
                other => vec![format!("not select's parameter error: {other:?}")],
            };
            assert_eq!(paths, [named], "{id}");
        }
        Ok(())
    }

    #[test]
    fn select_needs_its_events_and_takes_them_as_events() -> Result<(), Error> {
        let thing = Registry::builtin().get("select")?;
        let missing = thing.call(&Call::default());
        assert!(matches!(&missing, Err(Error::Call { message }) if message.contains("select") && message.contains("'events'")), "{missing:?}");
        let values = Call {
            inputs: [("events".to_owned(), Input::from_values(vec![1.0])?)].into(),
            ..Call::default()
        };
        let wrong = thing.call(&values);
        assert!(matches!(&wrong, Err(Error::Call { message }) if message.contains("'events' holds values")), "{wrong:?}");
        Ok(())
    }

    #[test]
    fn it_is_an_operation_giving_events_whose_result_is_data_with_no_recipe() -> Result<(), Error> {
        let thing = Registry::builtin().get("select")?;
        assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, crate::registry::Returns::Events));
        assert_eq!(thing.inputs(), [(crate::inputs::InputKind::Events, true)]);
        assert!(thing.summary().starts_with("The events with `label`"), "{}", thing.summary());
        Ok(())
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        /// Events on a coarse grid of times and strengths, so that ties and equal-to-threshold strengths are common.
        fn events() -> impl Strategy<Value = Vec<Event>> {
            let one = (0_u8..12, 0_u8..=4, prop::option::of(prop::sample::select(vec!["a", "b"]))).prop_map(
                |(t, strength, label)| {
                    event(f64::from(t) / 4.0, f64::from(strength) / 4.0, label).map_err(|error| error.to_string())
                },
            );
            prop::collection::vec(one, 0..30).prop_map(|events| events.into_iter().filter_map(Result::ok).collect())
        }

        proptest! {
            #[test]
            fn the_output_is_the_matching_strong_events_of_the_input_in_time_order(
                given in events(),
                label in prop::sample::select(vec!["", "a", "b", "c"]),
                threshold in prop_oneof![prop::sample::select(vec![0.0, 0.25, 0.5, 0.75, 1.0]), 0.0_f64..=1.0],
            ) {
                let kept = by_name(json!({"label": label, "threshold": threshold}), &given)
                    .map_err(|error| TestCaseError::fail(error.to_string()))?;
                // in time order
                prop_assert!(kept.windows(2).all(|pair| pair[0].t() <= pair[1].t()), "{:?}", times(&kept));
                // each kept one is strong enough and carries the label (any, if none is asked for)
                for event in &kept {
                    prop_assert!(event.strength() >= threshold, "{} < {threshold}", event.strength());
                    prop_assert!(label.is_empty() || event.label() == Some(label));
                }
                // a subsequence of the input as sorted by time (ties as given): walk the sorted input once
                let mut sorted = given.clone();
                sorted.sort_by(|a, b| a.t().total_cmp(&b.t()));
                let mut rest = sorted.iter();
                prop_assert!(kept.iter().all(|event| rest.any(|candidate| candidate == event)), "not a subsequence");
                // and nothing that qualifies is left out
                let qualifying = given
                    .iter()
                    .filter(|event| event.strength() >= threshold && (label.is_empty() || event.label() == Some(label)))
                    .count();
                prop_assert_eq!(kept.len(), qualifying);
                // the body alone agrees
                prop_assert_eq!(by_body(label, threshold, &given).map_err(|error| TestCaseError::fail(error.to_string()))?, kept);
            }
        }
    }
}
