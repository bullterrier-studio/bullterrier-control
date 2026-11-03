//! A signal that takes its input's value at each event and keeps it until the next event.
//!
//! The value at time t is the input at the time of the latest event at or before t: at exactly an event's time it
//! is already that event's value. Before the first event, and when there are no events, it is the input at time 0.
//! Events at the same time are one; their strength, duration, label and pitch change nothing (choose events first
//! with an operation on them). Quantizing a signal to the beat grid is this, with the grid's beats as events.

use std::convert::Infallible;

use crate::formats::Event;
use crate::{Error, Shaper, Signal};

/// Hold has no parameters: the events say when the value is taken.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params;

impl Signal {
    /// This signal sampled at each of `events` and held until the next: the shaper [`hold`](super), as
    /// [`pipe_with`](Signal::pipe_with) of its `Params` and `events` makes it (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// let ramp = Signal::from_values(&[0.0, 1.0], 1.0, FromValues::default(), Interpolation::Linear)?;
    /// let kicks = [Event::new(0.25, EventOptions::default())?, Event::new(0.5, EventOptions::default())?];
    /// // the input at 0 before the first kick, then at each kick's time
    /// assert_eq!(ramp.hold(&kicks)?.sample(&[0.1, 0.25, 0.4, 0.9])?, [0.0, 0.25, 0.25, 0.5]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe_with`]; hold has no parameters to refuse.
    pub fn hold(&self, events: &[Event]) -> Result<Signal, Error> {
        self.pipe_with(Params, events.to_vec())
    }
}

impl Shaper for Params {
    const NAME: &'static str = "hold";
    type Inputs = Vec<Event>;

    /// `input` read at the latest of 0 and the events' times at or before each time.
    fn shape(&self, input: &Signal, events: Vec<Event>) -> Result<Signal, Error> {
        // time 0 comes first: until the first event the input at time 0 is held, and no read needs a branch for it
        let mut moments: Vec<f64> = std::iter::once(0.0).chain(events.iter().map(Event::t)).collect();
        // a door gives the events in time order; the body does not rely on it
        moments.sort_by(f64::total_cmp);
        input.retimed(move |times| {
            // a read exactly at an event belongs to it: the moments up to and including it, and the last of those
            Ok::<_, Infallible>(times.iter().map(|time| moments[moments.partition_point(|moment| moment <= time) - 1]).collect())
        })
    }
}
