//! The events with every time moved by `offset`, without what would come before time 0.
//!
//! A moment that lands before 0 is dropped (one landing at 0 is kept). An event with a duration that starts before 0
//! and ends after it starts at 0 with the rest of its duration; one that ends at or before 0 is dropped. Landing at 0
//! holds to within float rounding (a nanosecond): 0.1 + 0.2 - 0.3 is 0, not a span of 3e-17 s. Every other field is
//! unchanged, and so is the order.

use crate::formats::Event;
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
    /// that starts before it cut there; every other field as it was. The door gives `events` in time order (those at
    /// one time as given), and a move keeps that order.
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
        let mut moved = Vec::with_capacity(events.len());
        for event in &events {
            let start = event.t() + self.offset;
            if at_least(start, 0.0) {
                // not `max`: as Python's `max(start, 0.0)`, a start of -0.0 stays -0.0
                let at = if 0.0 > start { 0.0 } else { start };
                moved.push(event.moved(at, event.duration()).map_err(|source| Error::Format { source })?);
            } else if let Some(duration) = event.duration().filter(|duration| more_than(start + duration, 0.0)) {
                moved.push(event.moved(0.0, Some(start + duration)).map_err(|source| Error::Format { source })?);
            }
        }
        Ok(Output::Events(moved))
    }
}
