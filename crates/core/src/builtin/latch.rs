//! A signal that is `before` until the first event with `label` and `after` from the moment of that event on.
//!
//! At exactly the event's time the value is already `after`. The first event is the earliest by time, whatever the
//! order of `events`; their strength, duration and the later events change nothing, and it never switches back. With
//! no matching event (an empty `label` matches every event) the value is `before` always.

use std::convert::Infallible;

use crate::formats::Event;
use crate::inputs::matching;
use crate::{Error, Signal, Source};

/// Which events count, and the level before and after the first of them.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Only events with this label count; empty takes every event, unlabelled ones too.
    #[param(default = "")]
    pub label: String,
    /// The level until the first matching event, 0 to 1; it is also the level when no event matches.
    #[param(default = 0.0, unit = Fraction, min = 0, max = 1)]
    pub before: f64,
    /// The level from the first matching event on, 0 to 1; lower than `before` makes an inverted latch.
    #[param(default = 1.0, unit = Fraction, min = 0, max = 1)]
    pub after: f64,
}

impl Source for Params {
    const NAME: &'static str = "latch";
    type Inputs = Vec<Event>;

    /// `before` up to the earliest matching event's time, `after` from it on.
    ///
    /// ```
    /// use bullterrier_control::builtin::latch;
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::lib;
    ///
    /// let drop = Event::new(12.0, EventOptions::default())?;
    /// let wake = lib::latch(&[drop], latch::Params { before: 0.2, after: 0.7, ..Default::default() })?;
    /// assert_eq!(wake.sample(&[11.9, 12.0, 30.0])?, [0.2, 0.7, 0.7]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None: every set of events makes a latch.
    fn source(&self, events: Vec<Event>) -> Result<Signal, Error> {
        // no match switches at infinity, which no read reaches: the same comparison serves
        let switch = matching(&events, &self.label).iter().map(Event::t).fold(f64::INFINITY, f64::min);
        let (before, after) = (self.before, self.after);
        Ok(Signal::from_fn(move |times, out| {
            for (value, time) in out.iter_mut().zip(times) {
                *value = if *time >= switch { after } else { before };
            }
            Ok::<_, Infallible>(())
        }))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::formats::EventOptions;
    use crate::inputs::Input;
    use crate::registry::{Call, Output};
    use crate::{Registry, builtin::latch, lib};

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
            assert_eq!(lib::latch(&events, levels(label))?.sample(&reads)?, expected, "{label:?}");
        }
        Ok(())
    }

    #[test]
    fn with_no_events_it_is_before_forever_and_with_one_at_0_after_from_the_start() -> Result<(), Error> {
        assert_eq!(lib::latch(&[], levels(""))?.sample(&[0.0, 12.0, 1e300])?, [0.2, 0.2, 0.2]);
        assert_eq!(lib::latch(&[event(0.0, None)?], levels(""))?.sample(&[0.0, 1.0])?, [0.7, 0.7]);
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
            return Err(Error::Call { message: "not a signal".into() });
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
}
