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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::Params;
    use crate::formats::{Event, EventOptions, Grid};
    use crate::inputs::{Input, InputKind};
    use crate::registry::{Call, Kind, Output, Returns};
    use crate::{Error, Operation, Registry};

    fn grid(beats: Vec<f64>, meter: u32, first_downbeat: u32) -> Result<Grid, Error> {
        Grid::new(beats, meter, first_downbeat).map_err(|source| Error::Format { source })
    }

    /// A beat every half second; the first downbeat is the second beat (index 1) and a bar is 4 beats.
    fn steady() -> Result<Grid, Error> {
        grid(vec![0.0, 0.5, 1.0, 1.5, 2.0], 4, 1)
    }

    /// The first beat is half a second long, the second one second.
    fn slowing() -> Result<Grid, Error> {
        grid(vec![0.0, 0.5, 1.5], 4, 0)
    }

    fn times(events: &[Event]) -> Vec<f64> {
        events.iter().map(Event::t).collect()
    }

    fn labels(events: &[Event]) -> Vec<&str> {
        events.iter().map(|event| event.label().unwrap_or("none")).collect()
    }

    /// The events of `grid` through the thing named `beats` in the registry: its input checked, its values read.
    fn by_name(division: i64, grid: &Grid) -> Result<Vec<Event>, Error> {
        let call = Call {
            inputs: [("grid".to_owned(), Input::Grid(grid.clone()))].into(),
            values: json!({"division": division}),
            ..Call::default()
        };
        match Registry::builtin().get("beats")?.call(&call)? {
            Output::Events(events) => Ok(events),
            other => Err(Error::Call { message: format!("beats gave {other:?}") }),
        }
    }

    /// The events of `grid` through the body alone, with the typed parameter.
    fn by_body(division: i64, grid: &Grid) -> Result<Vec<Event>, Error> {
        match (Params { division }).run(grid.clone())? {
            Output::Events(events) => Ok(events),
            other => Err(Error::Call { message: format!("beats gave {other:?}") }),
        }
    }

    /// Both ways of running it, which must agree.
    fn made(division: i64, grid: &Grid) -> Result<Vec<Event>, Error> {
        let events = by_body(division, grid)?;
        assert_eq!(by_name(division, grid)?, events, "the door and the body agree");
        Ok(events)
    }

    #[test]
    fn beats_gives_an_event_on_each_beat_labelled_downbeat_or_beat_each_at_full_strength() -> Result<(), Error> {
        let events = made(1, &steady()?)?;
        assert_eq!(times(&events), [0.0, 0.5, 1.0, 1.5, 2.0]);
        assert_eq!(labels(&events), ["beat", "downbeat", "beat", "beat", "beat"]);
        assert!(events.iter().all(|event| event.strength().to_bits() == 1.0_f64.to_bits() && event.duration().is_none() && event.pitch().is_none()));
        Ok(())
    }

    #[test]
    fn beats_with_a_division_adds_sub_events_between_the_beats() -> Result<(), Error> {
        let events = made(2, &steady()?)?;
        assert_eq!(times(&events), [0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]);
        assert_eq!(labels(&events), ["beat", "sub", "downbeat", "sub", "beat", "sub", "beat", "sub", "beat"]);
        Ok(())
    }

    #[test]
    fn beats_puts_a_sub_halfway_in_beats_between_the_beats_it_lies_between() -> Result<(), Error> {
        // the first beat lasts 0.5 s and the second 1.0 s: the sub-beats are at 0.25 s and at 1.0 s
        let events = made(2, &slowing()?)?;
        assert_eq!(times(&events), [0.0, 0.25, 0.5, 1.0, 1.5]);
        assert_eq!(labels(&events), ["downbeat", "sub", "beat", "sub", "beat"]);
        Ok(())
    }

    #[test]
    fn beats_with_a_division_that_does_not_halve_puts_the_subs_a_third_apart() -> Result<(), Error> {
        let events = made(3, &grid(vec![0.0, 0.6, 1.2], 2, 0)?)?;
        let expected = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2];
        assert!(times(&events).iter().zip(expected).all(|(got, want)| (got - want).abs() <= 1e-12), "{:?}", times(&events));
        assert_eq!(labels(&events), ["downbeat", "sub", "sub", "beat", "sub", "sub", "downbeat"]);
        Ok(())
    }

    /// `(id, meter, first_downbeat, labels of as many beats as the row has words)`.
    #[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
    const BARS: [(&str, u32, u32, &str); 5] = [
        ("a bar of 4 from the second beat", 4, 1, "beat downbeat beat beat beat downbeat beat beat beat"),
        ("a bar of 3 from the third beat", 3, 2, "beat beat downbeat beat beat downbeat beat"),
        ("a bar of 1: every beat is a downbeat", 1, 0, "downbeat downbeat downbeat downbeat"),
        ("a bar of 2 from the first beat", 2, 0, "downbeat beat downbeat beat downbeat"),
        ("the downbeat on the last beat there is", 4, 3, "beat beat beat downbeat"),
    ];

    #[test]
    fn beats_labels_every_meter_th_beat_from_the_first_downbeat_a_downbeat_and_no_other_beat() -> Result<(), Error> {
        for (id, meter, first_downbeat, expected) in BARS {
            let words: Vec<&str> = expected.split(' ').collect();
            let beats = (0_u8..).take(words.len()).map(|index| f64::from(index) * 0.5).collect();
            assert_eq!(labels(&made(1, &grid(beats, meter, first_downbeat)?)?), words, "{id}");
        }
        Ok(())
    }

    #[test]
    fn beats_has_the_count_of_beats_times_division_and_the_subs_between() -> Result<(), Error> {
        let events = made(64, &steady()?)?;
        assert_eq!(events.len(), 4 * 64 + 1);
        assert_eq!(labels(&events).iter().filter(|label| **label == "sub").count(), 4 * 63);
        assert_eq!(labels(&events).iter().filter(|label| **label == "downbeat").count(), 1);
        Ok(())
    }

    #[test]
    fn a_division_beyond_its_range_is_refused_naming_it() -> Result<(), Error> {
        for (id, division) in [("0", 0), ("65", 65)] {
            let paths: Vec<String> = match by_name(division, &steady()?) {
                Err(Error::Params { shaper, problems, .. }) if shaper == "beats" => {
                    problems.iter().map(|problem| problem.path.clone()).collect()
                }
                other => vec![format!("not beats' parameter error: {other:?}")],
            };
            assert_eq!(paths, ["division"], "{id}");
        }
        Ok(())
    }

    #[test]
    fn beats_needs_its_grid_and_takes_it_as_a_grid() -> Result<(), Error> {
        let thing = Registry::builtin().get("beats")?;
        let missing = thing.call(&Call::default());
        assert!(matches!(&missing, Err(Error::Call { message }) if message.contains("beats") && message.contains("'grid'")), "{missing:?}");
        assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Events));
        assert_eq!(thing.inputs(), [(InputKind::Grid, true)]);
        assert!(thing.summary().starts_with("Events at the grid's beats"), "{}", thing.summary());
        Ok(())
    }

    proptest! {
        /// Beat times in 64ths of a second, which floats hold exactly.
        #[test]
        fn beats_lies_on_the_grid_and_its_downbeats_are_the_grids(
            first in 0_u32..=64,
            gaps in prop::collection::vec(1_u32..=64, 1..12),
            meter in 1_u32..=8,
            division in 1_i64..=8,
            pick in 0_u32..8,
        ) {
            let mut steps = vec![first];
            for gap in &gaps {
                steps.push(steps[steps.len() - 1] + gap);
            }
            let first_downbeat = pick % meter.min(u32::try_from(steps.len()).unwrap_or(1));
            let beats: Vec<f64> = steps.iter().map(|step| f64::from(*step) / 64.0).collect();
            let g = grid(beats.clone(), meter, first_downbeat).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let events = made(division, &g).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let step = usize::try_from(division).unwrap_or(1);
            prop_assert_eq!(events.len(), (steps.len() - 1) * step + 1);
            // the whole beats are the grid's own times
            prop_assert_eq!(times(&events).into_iter().step_by(step).collect::<Vec<_>>(), beats.clone());
            // strictly later each
            prop_assert!(events.windows(2).all(|pair| pair[0].t() < pair[1].t()));
            let on = |wanted: &[&str]| -> Vec<f64> {
                events.iter().filter(|event| wanted.contains(&event.label().unwrap_or(""))).map(Event::t).collect()
            };
            prop_assert_eq!(on(&["downbeat"]), g.downbeats());
            prop_assert_eq!(on(&["beat", "downbeat"]), beats);
            prop_assert!(events.iter().enumerate().all(|(index, event)| index % step == 0 || event.label() == Some("sub")));
        }
    }

    #[test]
    fn events_are_made_with_the_defaults_of_an_event_but_a_label() -> Result<(), Error> {
        let expected = Event::new(0.5, EventOptions { label: Some("downbeat".into()), ..EventOptions::default() })
            .map_err(|source| Error::Format { source })?;
        assert_eq!(made(1, &steady()?)?[1], expected);
        Ok(())
    }
}
