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

    /// The events of `events` with the label (any, if it is empty) and a strength of at least the threshold, each as
    /// it was and in the order given: the door gives them in time order (events at one time as given), so they stay so.
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
        Ok(Output::Events(kept))
    }
}
