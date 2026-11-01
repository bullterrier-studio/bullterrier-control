//! The events with every time moved by `offset`, without what would come before time 0.
//!
//! A moment that lands before 0 is dropped (one landing at 0 is kept). An event with a duration that starts before 0
//! and ends after it starts at 0 with the rest of its duration; one that ends at or before 0 is dropped. Landing at 0
//! holds to within float rounding (a nanosecond): 0.1 + 0.2 - 0.3 is 0, not a span of 3e-17 s. Every other field is
//! unchanged, and so is the order.

use crate::formats::{Event, EventOptions};
use crate::registry::{Output, Returns};
use crate::rounding::{at_least, more_than};
use crate::{Error, Operation};

/// How far to move the events.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Seconds to move every event, -3600 to 3600: positive moves them later (a lag), negative earlier (a lead). What
    /// would start before time 0 is cut off there.
    #[param(default = 0.0, unit = Seconds, min = -3600, max = 3600)]
    pub offset: f64,
}

impl Operation for Params {
    const NAME: &'static str = "shift";
    type Inputs = Vec<Event>;
    const RETURNS: Returns = Returns::Events;

    /// The events of `events` with every time moved by `offset` seconds, what lands before time 0 dropped and a span
    /// that starts before it cut there; in time order, every other field as it was.
    ///
    /// ```
    /// use bullterrier_control::builtin::shift;
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::Operation;
    ///
    /// let at = |t| Event::new(t, EventOptions::default());
    /// // a lead of a quarter second: 0.125 would land before 0 and goes; 0.25 lands at 0
    /// let led = shift::Params { offset: -0.25 }.run(vec![at(0.125)?, at(0.25)?, at(1.0)?])?;
    /// assert!(matches!(led, Output::Events(kept) if kept == [at(0.0)?, at(0.75)?]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None for events the door checked: a moved time and a rest of a duration are valid.
    fn run(&self, events: Vec<Event>) -> Result<Output, Error> {
        let mut events = events;
        // stable: events at one time keep their order (a door gives them sorted already; the body does not rely on it)
        events.sort_by(|a, b| a.t().total_cmp(&b.t()));
        let mut moved = Vec::with_capacity(events.len());
        for event in &events {
            let start = event.t() + self.offset;
            if at_least(start, 0.0) {
                // not `max`: as Python's `max(start, 0.0)`, a start of -0.0 stays -0.0
                moved.push(rebuilt(event, if 0.0 > start { 0.0 } else { start }, event.duration())?);
            } else if let Some(duration) = event.duration().filter(|duration| more_than(start + duration, 0.0)) {
                moved.push(rebuilt(event, 0.0, Some(start + duration))?);
            }
        }
        Ok(Output::Events(moved))
    }
}

/// `event` at `t` with `duration`, every other field as it was.
fn rebuilt(event: &Event, t: f64, duration: Option<f64>) -> Result<Event, Error> {
    let options = EventOptions {
        strength: event.strength(),
        duration,
        label: event.label().map(str::to_owned),
        pitch: event.pitch(),
    };
    Event::new(t, options).map_err(|source| Error::Format { source })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Params;
    use crate::formats::{Event, EventOptions};
    use crate::inputs::{Input, InputKind};
    use crate::registry::{Call, Kind, Output, Returns};
    use crate::{Error, Operation, Registry};

    const LARGEST: f64 = f64::MAX;

    fn event(t: f64, options: EventOptions) -> Result<Event, Error> {
        Event::new(t, options).map_err(|source| Error::Format { source })
    }

    fn moment(t: f64) -> Result<Event, Error> {
        event(t, EventOptions { strength: 0.5, ..EventOptions::default() })
    }

    fn span(t: f64, duration: f64) -> Result<Event, Error> {
        event(t, EventOptions { duration: Some(duration), ..EventOptions::default() })
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
            other => Err(Error::Call { message: format!("shift gave {other:?}") }),
        }
    }

    /// `events` through the body alone, with the typed parameter.
    fn by_body(offset: f64, events: &[Event]) -> Result<Vec<Event>, Error> {
        match (Params { offset }).run(events.to_vec())? {
            Output::Events(moved) => Ok(moved),
            other => Err(Error::Call { message: format!("shift gave {other:?}") }),
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
        ("a positive offset moves every moment later", 0.25, &[0.0, 0.5, 1.0], &[0.25, 0.75, 1.25]),
        ("0.1 would land before 0 and is dropped; 0.25 lands at 0", -0.25, &[0.1, 0.25, 1.0], &[0.0, 0.75]),
        ("a moment landing exactly at 0 is kept", -0.25, &[0.125, 0.25, 0.375], &[0.0, 0.125]),
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
        ("a moment 1e-16 before 0 (0.1 + 0.7 - 0.8) is at 0", -0.8, 0.799_999_999_999_999_9, Some(0.0)),
        ("a decimal moment landing at 0", -0.3, 0.3, Some(0.0)),
        ("a moment a millionth after 0 is kept", -0.499_999, 0.5, Some(1e-6)),
    ];

    #[test]
    fn shift_keeps_a_moment_landing_at_zero_within_rounding() -> Result<(), Error> {
        for (id, offset, given, expected) in LANDINGS {
            let moved = times(&shifted(offset, &[moment(given)?])?);
            assert_eq!(moved.len(), usize::from(expected.is_some()), "{id}");
            assert!(moved.iter().zip(expected).all(|(got, want)| close(*got, want) && *got >= 0.0), "{id}: {moved:?}");
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
        let note = event(0.5, EventOptions { strength: 0.3, duration: Some(0.25), label: Some("lead".into()), pitch: Some(64.5) })?;
        let given = [moment(0.1)?, note, span(0.0, 2.0)?];
        // unchanged, in time order
        let expected = [given[2].clone(), given[0].clone(), given[1].clone()];
        assert_eq!(by_body(0.0, &given)?, expected);
        assert_eq!(by_name(0.0, &given)?, expected);
        Ok(())
    }

    /// `(id, offset, start, duration, the (start, duration) it ends up with)`.
    #[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
    const SPANS: [(&str, f64, f64, f64, (f64, f64)); 9] = [
        ("a span cut at 0 keeps the rest: 1.125 - 0.375", -0.375, 0.125, 1.0, (0.0, 0.75)),
        ("starts at -0.25, ends at 0.25: 0.25 left", -0.5, 0.25, 0.5, (0.0, 0.25)),
        ("starts exactly at 0: whole, not cut", -0.5, 0.5, 1.0, (0.0, 1.0)),
        ("moved later: the duration is unchanged", 0.25, 0.5, 1.0, (0.75, 1.0)),
        ("a span of the largest length", -3600.0, 0.0, LARGEST, (0.0, LARGEST)),
        ("the largest time and span", 3600.0, LARGEST, LARGEST, (LARGEST, LARGEST)),
        ("decimals: a span ending 1e-6 after 0 is kept", -0.3, 0.1, 0.200_001, (0.0, 1e-6)),
        ("decimals: starts at -0.2, ends at 0.5, 0.5 left", -0.3, 0.1, 0.7, (0.0, 0.5)),
        ("decimals: starts at -0.4, ends at 1.0, 1.0 left", -0.7, 0.3, 1.4, (0.0, 1.0)),
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
        let options = EventOptions { strength: 0.6, duration: Some(0.5), label: Some("riser".into()), pitch: Some(60.0) };
        let cut = EventOptions { duration: Some(0.25), ..options.clone() };
        assert_eq!(shifted(-0.5, &[event(0.25, options)?])?, [event(0.0, cut)?]);
        Ok(())
    }

    #[test]
    fn shift_keeps_events_at_the_same_time_in_the_order_given() -> Result<(), Error> {
        let named = |name: &str| event(1.0, EventOptions { label: Some(name.into()), ..EventOptions::default() });
        let (first, second) = (named("a")?, named("b")?);
        let moved = shifted(0.5, &[second.clone(), first.clone()])?;
        assert_eq!(moved.iter().map(|moved| moved.label()).collect::<Vec<_>>(), [Some("b"), Some("a")]);
        Ok(())
    }

    #[test]
    fn shift_works_in_time_order_whatever_the_order_given() -> Result<(), Error> {
        let given = [moment(1.0)?, moment(0.25)?, moment(0.5)?];
        assert_eq!(times(&shifted(0.25, &given)?), [0.5, 0.75, 1.25]);
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
        assert!(thing.summary().starts_with("The events with every time moved"), "{}", thing.summary());
        Ok(())
    }
}
