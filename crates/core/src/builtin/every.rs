//! The events at positions `offset`, `offset + n`, `offset + 2n`, ... of the events in time order.
//!
//! Events at the same time count one by one, in the order given. Every field of a kept event is unchanged.

use crate::formats::{Event, Problem};
use crate::registry::{Output, Returns};
use crate::{Error, Operation};

/// Which of the events in time order to keep: every `n`th, counting from `offset`.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// Keep one event in this many, 1 to 1000: 1 keeps all, 2 every second one, 3 every third.
    #[param(default = 2, unit = Count, min = 1, max = 1000)]
    pub n: i64,
    /// How many events to skip before the first one kept, 0 to 999 and below `n`: 0 starts with the first event, 1
    /// with the second.
    #[param(default = 0, unit = Count, min = 0, max = 999)]
    pub offset: i64,
}

impl Operation for Params {
    const NAME: &'static str = "every";
    type Inputs = Vec<Event>;
    const RETURNS: Returns = Returns::Events;

    /// The events at positions `offset`, `offset + n`, ... of `events`, each as it was. The door gives `events` in time
    /// order (those at one time as given), and the positions count in that order.
    ///
    /// ```
    /// use bullterrier_control::builtin::every;
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::Operation;
    ///
    /// let events = (0..5)
    ///     .map(|i| Event::new(f64::from(i), EventOptions::default()))
    ///     .collect::<Result<Vec<_>, _>>()?;
    /// let second = every::Params { n: 2, offset: 1 };
    /// assert!(matches!(second.run(events.clone())?, Output::Events(kept) if kept == [events[1].clone(), events[3].clone()]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None: every set of events can be thinned.
    fn run(&self, events: Vec<Event>) -> Result<Output, Error> {
        // counted in i64, the type of the parameters, which the table keeps in 1..=1000 and 0..=999
        let kept = (0_i64..)
            .zip(events)
            .filter(|(position, _)| *position >= self.offset && (position - self.offset) % self.n == 0)
            .map(|(_, event)| event)
            .collect();
        Ok(Output::Events(kept))
    }
}

/// `offset` must lie below `n`: the kept positions are `offset`, `offset + n`, ... and the first lies within the first
/// `n`.
fn rule(params: &Params) -> Vec<Problem> {
    if params.offset < params.n {
        return Vec::new();
    }
    vec![Problem {
        path: "offset".to_owned(),
        rule: format!("must be below n = {} (got {})", params.n, params.offset),
    }]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Params;
    use crate::Params as _;
    use crate::formats::{Event, EventOptions, check_problems};
    use crate::inputs::{Input, InputKind};
    use crate::registry::{Call, Kind, Output, Returns};
    use crate::{Error, Operation, Registry};

    fn event(t: f64, options: EventOptions) -> Result<Event, Error> {
        Event::new(t, options).map_err(|source| Error::Format { source })
    }

    /// Seven events a quarter second apart, each telling its index by its label.
    fn seven() -> Result<Vec<Event>, Error> {
        (0..7_u8)
            .map(|i| event(f64::from(i) * 0.25, EventOptions { label: Some(i.to_string()), ..EventOptions::default() }))
            .collect()
    }

    fn indices(events: &[Event]) -> Vec<u8> {
        events.iter().map(|kept| kept.label().and_then(|label| label.parse().ok()).unwrap_or(u8::MAX)).collect()
    }

    /// `events` through the thing named `every` in the registry: its input checked, its values read.
    fn by_name(values: serde_json::Value, events: &[Event]) -> Result<Vec<Event>, Error> {
        let call = Call {
            inputs: [("events".to_owned(), Input::from_events(events.to_vec()))].into(),
            values,
            ..Call::default()
        };
        match Registry::builtin().get("every")?.call(&call)? {
            Output::Events(kept) => Ok(kept),
            other => Err(Error::Call { message: format!("every gave {other:?}") }),
        }
    }

    /// `events` through the body alone, with the typed parameters.
    fn by_body(n: i64, offset: i64, events: &[Event]) -> Result<Vec<Event>, Error> {
        match (Params { n, offset }).run(events.to_vec())? {
            Output::Events(kept) => Ok(kept),
            other => Err(Error::Call { message: format!("every gave {other:?}") }),
        }
    }

    /// `(id, n, offset, the indices kept of seven events)`: the rows of `tests/builtin/test_every.py`.
    #[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
    const KEEPS: [(&str, i64, i64, &[u8]); 8] = [
        ("the defaults keep every second event, from the first", 2, 0, &[0, 2, 4, 6]),
        ("every third from the second: 1, 4 (7 is past the end)", 3, 1, &[1, 4]),
        ("every third from the first", 3, 0, &[0, 3, 6]),
        ("n = 1 keeps every event", 1, 0, &[0, 1, 2, 3, 4, 5, 6]),
        ("an offset starts at that event", 2, 1, &[1, 3, 5]),
        ("the last offset below n keeps one", 4, 3, &[3]),
        ("n above the count keeps only the first", 10, 0, &[0]),
        ("an offset past the last event keeps none", 10, 9, &[]),
    ];

    #[test]
    fn every_keeps_the_events_at_offset_plus_whole_multiples_of_n() -> Result<(), Error> {
        for (id, n, offset, expected) in KEEPS {
            assert_eq!(indices(&by_body(n, offset, &seven()?)?), expected, "{id}: the body");
            assert_eq!(indices(&by_name(json!({"n": n, "offset": offset}), &seven()?)?), expected, "{id}: the door");
        }
        Ok(())
    }

    #[test]
    fn every_counts_in_time_order_whatever_the_order_given() -> Result<(), Error> {
        let mut backwards = seven()?;
        backwards.reverse();
        assert_eq!(indices(&by_name(json!({"n": 3, "offset": 1}), &backwards)?), [1, 4]);
        Ok(())
    }

    #[test]
    fn every_counts_events_at_the_same_time_one_by_one_in_the_order_given() -> Result<(), Error> {
        let named: Vec<Event> = "abcd"
            .chars()
            .map(|name| event(1.0, EventOptions { label: Some(name.to_string()), ..EventOptions::default() }))
            .collect::<Result<_, _>>()?;
        let reversed: Vec<Event> = named.iter().rev().cloned().collect();
        assert_eq!(by_body(2, 0, &named)?, [named[0].clone(), named[2].clone()]);
        assert_eq!(by_name(json!({}), &reversed)?, [reversed[0].clone(), reversed[2].clone()]);
        Ok(())
    }


    /// 0.0 and -0.0 are one time: the door keeps the order given, `a` then `b`, and so does the operation.
    fn tied() -> Result<[Event; 2], Error> {
        let named = |t: f64, name: &str| event(t, EventOptions { label: Some(name.into()), ..EventOptions::default() });
        Ok([named(0.0, "a")?, named(-0.0, "b")?])
    }

    #[test]
    fn every_counts_events_at_zero_and_minus_zero_as_one_time_in_the_order_given() -> Result<(), Error> {
        let [a, b] = tied()?;
        assert_eq!(by_name(json!({}), &[a.clone(), b.clone()])?, std::slice::from_ref(&a));
        assert_eq!(by_name(json!({}), &[b.clone(), a])?, std::slice::from_ref(&b));
        Ok(())
    }

    #[test]
    fn every_keeps_every_field_of_the_events_it_keeps() -> Result<(), Error> {
        let options = EventOptions { strength: 0.7, duration: Some(0.25), label: Some("lead".into()), pitch: Some(64.5) };
        let note = event(0.5, options)?;
        let events = [note.clone(), event(0.75, EventOptions::default())?];
        assert_eq!(by_body(2, 0, &events)?, std::slice::from_ref(&note));
        assert_eq!(by_name(json!({"n": 2}), &events)?, [note]);
        Ok(())
    }

    #[test]
    fn every_of_no_events_is_no_events() -> Result<(), Error> {
        assert_eq!(by_body(2, 0, &[])?, []);
        assert_eq!(by_name(json!({}), &[])?, []);
        Ok(())
    }

    #[test]
    fn n_and_offset_are_accepted_up_to_1000_and_999_and_refused_beyond() -> Result<(), Error> {
        assert_eq!(by_name(json!({"n": 1000, "offset": 999}), &seven()?)?, []);
        assert_eq!(indices(&by_name(json!({"n": 1000}), &seven()?)?), [0]);
        for (id, values, named) in [
            ("n above 1000", json!({"n": 1001}), "n"),
            ("n below 1", json!({"n": 0}), "n"),
            ("offset above 999", json!({"n": 1000, "offset": 1000}), "offset"),
            ("offset below 0", json!({"offset": -1}), "offset"),
            ("an offset not below n", json!({"n": 3, "offset": 3}), "offset"),
        ] {
            let paths: Vec<String> = match by_name(values, &seven()?) {
                Err(Error::Params { shaper, problems, .. }) if shaper == "every" => {
                    problems.iter().map(|problem| problem.path.clone()).collect()
                }
                other => vec![format!("not every's parameter error: {other:?}")],
            };
            assert_eq!(paths, [named], "{id}");
        }
        Ok(())
    }

    #[test]
    fn it_is_an_operation_giving_events_from_events() -> Result<(), Error> {
        let thing = Registry::builtin().get("every")?;
        assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Events));
        assert_eq!(thing.inputs(), [(InputKind::Events, true)]);
        assert!(thing.summary().starts_with("The events at positions"), "{}", thing.summary());
        Ok(())
    }

    fn check(id: &str, given: &serde_json::Value, expected: &'static [(&'static str, &'static str)]) {
        match Params::parse(given) {
            Ok(params) => assert!(expected.is_empty(), "{id}: accepted {params:?}, wanted {expected:?}"),
            Err(error) => check_problems(id, &error.problems, expected),
        }
    }

    #[test]
    fn the_offset_must_be_below_n() {
        for (id, given, expected) in [
            ("the defaults", json!({}), &[][..]),
            ("one below n", json!({"n": 3, "offset": 2}), &[]),
            ("the first event of many", json!({"n": 7, "offset": 0}), &[]),
            ("equal to n", json!({"n": 3, "offset": 3}), &[("offset", "must be below n = 3 (got 3)")]),
            ("past n", json!({"n": 2, "offset": 5}), &[("offset", "must be below n = 2 (got 5)")]),
        ] {
            check(id, &given, expected);
        }
    }

    #[test]
    fn a_broken_field_rule_hides_the_rule_between_fields() {
        check(
            "offset past its own range",
            &json!({"n": 3, "offset": 1000}),
            &[("offset", "<= 999")],
        );
    }
}
