//! The events, each moved to the nearest beat position that is a multiple of 1/`division` of a beat.
//!
//! Nearest is measured in beats, so it follows tempo changes; exactly halfway goes to the later position, and an event
//! whose nearest position would be before time 0 goes to the next position up (a position at 0 to within float
//! rounding, a nanosecond, is 0). Before the first beat and after the last the grid's first or last beat length goes
//! on. The result is in time order, and events that snap to one time keep the order they had; every other field is
//! unchanged (a duration does not move its end). A time too far from the beats to snap is refused, naming `events`.

use std::collections::BTreeMap;

use crate::formats::{Event, FormatError, Grid, Problem};
use crate::inputs::{Input, InputKind};
use crate::registry::{self, Output, Returns};
use crate::rounding::at_least;
use crate::{Error, Operation};

/// How fine the positions are that events snap to.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Snap positions per beat, 1 to 64: 1 snaps to the beats, 2 to the beats and the halves between them, 4 to
    /// quarters. Raising it moves events less.
    #[param(default = 1, unit = Count, min = 1, max = 64)]
    pub division: i64,
}

/// What `quantize` snaps, and to what: both required.
#[derive(Clone, Debug, PartialEq)]
pub struct Inputs {
    /// The events to snap.
    pub events: Vec<Event>,
    /// The beats they snap to.
    pub grid: Grid,
}

impl registry::Inputs for Inputs {
    const DECLARED: &'static [(InputKind, bool)] = &[(InputKind::Events, true), (InputKind::Grid, true)];

    fn from_inputs(given: &BTreeMap<String, Input>) -> Result<Self, Error> {
        Ok(Self { events: registry::Inputs::from_inputs(given)?, grid: registry::Inputs::from_inputs(given)? })
    }

    fn into_inputs(self) -> Result<Vec<Input>, Error> {
        Ok([self.events.into_inputs()?, self.grid.into_inputs()?].concat())
    }
}

impl Operation for Params {
    const NAME: &'static str = "quantize";
    type Inputs = Inputs;
    const RETURNS: Returns = Returns::Events;

    /// The events of `events`, each moved to the nearest position on `grid` that is a whole number of
    /// 1/`division` beats, every other field as it was. The door gives `events` in time order (those at one time as
    /// given); a snap never moves a later event before an earlier one, so that order is kept.
    ///
    /// ```
    /// use bullterrier_control::builtin::quantize;
    /// use bullterrier_control::formats::{Event, EventOptions, Grid};
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::Operation;
    ///
    /// let at = |t| Event::new(t, EventOptions::default());
    /// let grid = Grid::new(vec![0.0, 0.5, 1.0], 4, 0)?;
    /// // in halves of a beat (0.25 s): 0.2 s is 0.8 halves, 0.3 s 1.2 halves
    /// let inputs = quantize::Inputs { events: vec![at(0.2)?, at(0.3)?], grid };
    /// let snapped = quantize::Params { division: 2 }.run(inputs)?;
    /// assert!(matches!(snapped, Output::Events(events) if events == [at(0.25)?, at(0.25)?]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Input`] naming `events` if a time is too far from the beats for its position, or the position
    /// multiplied by `division`, to be a finite number.
    fn run(&self, Inputs { events, grid }: Inputs) -> Result<Output, Error> {
        // the table keeps the division in 1..=64
        #[expect(clippy::cast_precision_loss, reason = "1 to 64")]
        let division = self.division as f64;
        let times: Vec<f64> = events.iter().map(Event::t).collect();
        let snap = || -> Result<Vec<f64>, FormatError> {
            // halfway goes up
            let steps: Vec<f64> = grid.beat_at(&times)?.iter().map(|beat| (beat * division + 0.5).floor()).collect();
            let nearest = grid.seconds_at(&steps.iter().map(|step| step / division).collect::<Vec<_>>())?;
            // a position before 0 (beyond rounding) is one step too early: the next one up is at 0 or after
            let positions: Vec<f64> = steps
                .iter()
                .zip(&nearest)
                .map(|(step, time)| if at_least(*time, 0.0) { step / division } else { (step + 1.0) / division })
                .collect();
            grid.seconds_at(&positions)
        };
        let snapped = snap().map_err(|error| too_far(&error))?;
        events
            .iter()
            .zip(snapped)
            // a position at 0 to within rounding is 0; not `max`: as numpy's `maximum`, -0.0 stays -0.0
            .map(|(event, t)| event.moved(if 0.0 > t { 0.0 } else { t }, event.duration()))
            .collect::<Result<_, _>>()
            .map(Output::Events)
            .map_err(|source| Error::Format { source })
    }
}

/// The refusal of a time `error` says the grid cannot snap, naming `events`.
fn too_far(error: &FormatError) -> Error {
    let rule = format!("a time is too far from the grid's beats to snap ({error})");
    Error::Input { problems: vec![Problem { path: "events".to_owned(), rule }], omitted: 0 }
}
