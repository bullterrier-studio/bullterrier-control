//! The events from `start` up to (not including) `end`, then the earliest `first` of them.
//!
//! The range comes first, then `first`: `start = 8, first = 4` is the first four events at or after 8 s. An event at
//! `start` is kept and one at `end` is not; "at" holds to within float rounding (a nanosecond), as in `thin`. An `end`
//! of 0 means no end, and a `first` of 0 means all. Every field of a kept event is unchanged.

use crate::formats::{Event, Problem};
use crate::registry::{Output, Returns};
use crate::rounding::at_least;
use crate::{Error, Operation};

/// Which stretch of the events to keep: from a time, to a time, and at most this many.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// The time the window opens, in seconds, 0 to 86400: events at or after it are kept. 0 (the default) starts at the
    /// beginning.
    #[param(default = 0.0, unit = Seconds, min = 0, max = 86400)]
    pub start: f64,
    /// The time the window closes, in seconds, 0 to 86400 and not before `start`: events before it are kept, one at
    /// this time is not. 0 (the default) means no end: every event from `start` on.
    #[param(default = 0.0, unit = Seconds, min = 0, max = 86400)]
    pub end: f64,
    /// How many events to keep, 0 to 10000: the first this many in time order of the events left by `start` and `end`
    /// (the range is applied first). Raising it keeps more; 0 (the default) keeps all of them.
    #[param(default = 0, unit = Count, min = 0, max = 10000)]
    pub first: i64,
}

impl Operation for Params {
    const NAME: &'static str = "window";
    type Inputs = Vec<Event>;
    const RETURNS: Returns = Returns::Events;

    /// The events of `events` at or after `start` and before `end` (if it is above 0), the first `first` of them (if it
    /// is above 0), each as it was. The range is applied first, then `first` counts from the start of what the range
    /// left: `start = 8, first = 4` is the first four events at or after 8 s, not the events at or after 8 s among the
    /// first four. The door gives `events` in time order, so "the first" are the earliest.
    ///
    /// ```
    /// use bullterrier_control::builtin::window;
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::Operation;
    ///
    /// let at = |t| Event::new(t, EventOptions::default());
    /// let events = vec![at(1.0)?, at(2.0)?, at(3.0)?, at(4.0)?];
    /// // from 2 s on, the first two
    /// let kept = window::Params { start: 2.0, end: 0.0, first: 2 }.run(events)?;
    /// assert!(matches!(kept, Output::Events(kept) if kept == [at(2.0)?, at(3.0)?]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None: every set of events has a window; the parameters are checked before this runs.
    fn run(&self, events: Vec<Event>) -> Result<Output, Error> {
        // first is in 0..=10000 by the table; 0 is all
        let most = usize::try_from(self.first).ok().filter(|first| *first > 0).unwrap_or(usize::MAX);
        let kept = events
            .into_iter()
            .filter(|event| at_least(event.t(), self.start) && (self.end == 0.0 || !at_least(event.t(), self.end)))
            .take(most)
            .collect();
        Ok(Output::Events(kept))
    }
}

/// A window that closes (`end` above 0) closes at or after it opens.
fn rule(params: &Params) -> Vec<Problem> {
    if params.end == 0.0 || params.end >= params.start {
        return Vec::new();
    }
    vec![Problem {
        path: "end".to_owned(),
        rule: format!("must not be before start = {} (got {})", params.start, params.end),
    }]
}
