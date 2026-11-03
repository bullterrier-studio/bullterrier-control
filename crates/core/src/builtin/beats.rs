//! Events at the grid's beats and, with a `division`, at equal steps of beats between them, each of strength 1.
//!
//! The label is `downbeat` on the first beat of each bar (every `meter`th beat from `first_downbeat`), `beat` on any
//! other beat, and `sub` between beats. There are events from the first stored beat to the last, none beyond, and a
//! step is an equal share of a beat, so it follows tempo changes. Select some with `select`: the downbeats are
//! `select(events=beats(grid=g), label="downbeat")`.

use crate::formats::{Event, EventOptions, Grid};
use crate::registry::{Output, Returns};
use crate::{Error, Operation};

/// How finely to divide each beat.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Events per beat, 1 to 64: 1 gives the beats alone, 2 adds one `sub` event halfway between neighbouring beats,
    /// 4 adds three, and so on. Raising it gives a finer pulse.
    #[param(default = 1, unit = Count, min = 1, max = 64)]
    pub division: i64,
}

impl Operation for Params {
    const NAME: &'static str = "beats";
    type Inputs = Grid;
    const RETURNS: Returns = Returns::Events;

    /// The events at the beats of `grid` and at `division` equal steps of beats from each to the next, each of
    /// strength 1 and labelled `downbeat`, `beat` or `sub`.
    ///
    /// ```
    /// use bullterrier_control::builtin::beats;
    /// use bullterrier_control::formats::Grid;
    /// use bullterrier_control::registry::Output;
    /// use bullterrier_control::Operation;
    ///
    /// let grid = Grid::new(vec![0.0, 0.5, 1.0], 2, 0)?;
    /// let Output::Events(events) = (beats::Params { division: 2 }).run(grid)? else { unreachable!() };
    /// let labelled: Vec<_> = events.iter().map(|event| (event.t(), event.label())).collect();
    /// assert_eq!(
    ///     labelled,
    ///     [(0.0, Some("downbeat")), (0.25, Some("sub")), (0.5, Some("beat")), (0.75, Some("sub")), (1.0, Some("downbeat"))]
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None for a grid the door checked: its positions have times.
    fn run(&self, grid: Grid) -> Result<Output, Error> {
        // the table keeps the division in 1..=64
        #[expect(clippy::cast_sign_loss, clippy::cast_possible_truncation, reason = "1 to 64")]
        let division = self.division as usize;
        // a grid holds two or more beats
        let steps = (grid.beats().len() - 1) * division;
        let positions: Vec<f64> = (0..=steps).map(|step| whole(step) / whole(division)).collect();
        let times = grid.seconds_at(&positions).map_err(|source| Error::Format { source })?;
        // `usize::MAX` for a count no beat index reaches: a bar of one, a downbeat past the last beat
        let meter = usize::try_from(grid.meter()).unwrap_or(usize::MAX);
        let first = usize::try_from(grid.first_downbeat()).unwrap_or(usize::MAX);
        let events = times
            .into_iter()
            .enumerate()
            .map(|(step, t)| {
                let beat = step / division;
                // a bar starts every `meter` beats from `first_downbeat`; none starts before it
                let label = if step % division != 0 {
                    "sub"
                } else if beat >= first && (beat - first).is_multiple_of(meter) {
                    "downbeat"
                } else {
                    "beat"
                };
                Event::new(t, EventOptions { label: Some(label.to_owned()), ..EventOptions::default() })
                    .map_err(|source| Error::Format { source })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Output::Events(events))
    }
}

/// A count as a number: exact below 2^53, far above any grid.
#[expect(clippy::cast_precision_loss, reason = "a step count is far below 2^53")]
const fn whole(count: usize) -> f64 {
    count as f64
}
