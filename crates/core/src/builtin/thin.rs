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
