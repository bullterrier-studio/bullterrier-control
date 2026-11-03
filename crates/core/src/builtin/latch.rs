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
