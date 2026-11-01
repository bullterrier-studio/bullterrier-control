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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{Inputs, Params};
    use crate::formats::{Event, EventOptions, Grid};
    use crate::inputs::{Input, InputKind};
    use crate::registry::{self, Call, Kind, Output, Returns};
    use crate::{Error, Operation, Registry};

    fn grid(beats: &[f64], meter: u32, first_downbeat: u32) -> Result<Grid, Error> {
        Grid::new(beats.to_vec(), meter, first_downbeat).map_err(|source| Error::Format { source })
    }

    /// A beat every half second, positions 0 to 4.
    fn steady() -> Result<Grid, Error> {
        grid(&[0.0, 0.5, 1.0, 1.5, 2.0], 4, 1)
    }

    /// The first beat a second long (60 bpm), the second half a second (120 bpm).
    fn slowing() -> Result<Grid, Error> {
        grid(&[0.0, 1.0, 1.5], 2, 0)
    }

    /// Beats from 0.3 s: before them the grid goes on backwards, one 0.5 s beat per position (-0.2 s, -0.7 s, ...).
    fn late() -> Result<Grid, Error> {
        grid(&[0.3, 0.8], 1, 0)
    }

    fn named(t: f64, name: &str) -> Result<Event, Error> {
        Event::new(t, EventOptions { label: Some(name.into()), ..EventOptions::default() })
            .map_err(|source| Error::Format { source })
    }

    fn moments(times: &[f64]) -> Result<Vec<Event>, Error> {
        let moment = |t: &f64| Event::new(*t, EventOptions::default()).map_err(|source| Error::Format { source });
        times.iter().map(moment).collect()
    }

    fn times(events: &[Event]) -> Vec<f64> {
        events.iter().map(Event::t).collect()
    }

    fn labels(events: &[Event]) -> Vec<&str> {
        events.iter().map(|event| event.label().unwrap_or("")).collect()
    }

    /// `events` through the thing named `quantize` in the registry: its inputs checked, its values read.
    fn by_name(division: i64, events: &[Event], grid: &Grid) -> Result<Vec<Event>, Error> {
        let call = Call {
            inputs: [
                ("events".to_owned(), Input::from_events(events.to_vec())),
                ("grid".to_owned(), Input::Grid(grid.clone())),
            ]
            .into(),
            values: json!({"division": division}),
            ..Call::default()
        };
        match Registry::builtin().get("quantize")?.call(&call)? {
            Output::Events(snapped) => Ok(snapped),
            other => Err(Error::Call { message: format!("quantize gave {other:?}") }),
        }
    }

    /// `events` (in time order, as the door gives them) through the body alone, with the typed parameter.
    fn by_body(division: i64, events: &[Event], grid: &Grid) -> Result<Vec<Event>, Error> {
        match (Params { division }).run(Inputs { events: events.to_vec(), grid: grid.clone() })? {
            Output::Events(snapped) => Ok(snapped),
            other => Err(Error::Call { message: format!("quantize gave {other:?}") }),
        }
    }

    /// The times `given` (in time order) snap to, both ways of running it, which must agree.
    fn snapped(division: i64, given: &[f64], grid: &Grid) -> Result<Vec<f64>, Error> {
        let events = moments(given)?;
        let snapped = by_body(division, &events, grid)?;
        assert_eq!(by_name(division, &events, grid)?, snapped, "the door and the body agree");
        Ok(times(&snapped))
    }

    fn close(left: &[f64], right: &[f64]) -> bool {
        left.len() == right.len() && left.iter().zip(right).all(|(l, r)| (l - r).abs() <= 1e-12)
    }

    /// `(id, division, times given, times snapped)` on the steady grid: each snapped time a multiple of 1/128, which
    /// floats hold exactly.
    #[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
    const STEADY: [(&str, i64, &[f64], &[f64]); 16] = [
        ("0.2 is 0.4 of a beat: down to the beat at 0", 1, &[0.2], &[0.0]),
        ("0.3 is 0.6 of a beat: up to the beat at 0.5", 1, &[0.3], &[0.5]),
        ("0.74 is 1.48 beats: down to 1", 1, &[0.74], &[0.5]),
        ("exactly halfway between two beats goes to the later one", 1, &[0.25], &[0.5]),
        ("events on a beat stay", 1, &[0.0, 0.5, 1.0], &[0.0, 0.5, 1.0]),
        ("after the last beat its length goes on: 4.6 beats snaps to 5", 1, &[2.3], &[2.5]),
        ("after the last beat: 4.4 beats snaps to 4", 1, &[2.2], &[2.0]),
        ("division 2: 0.3 is 1.2 halves, down to 1", 2, &[0.3], &[0.25]),
        ("division 2: exactly halfway between halves goes up", 2, &[0.125], &[0.25]),
        ("division 2: just under halfway goes down", 2, &[0.12], &[0.0]),
        ("division 4: 0.3 is 2.4 quarters, down to 2", 4, &[0.3], &[0.25]),
        ("division 4: 0.7 is 5.6 quarters, up to 6", 4, &[0.7], &[0.75]),
        ("division 4: exactly halfway between quarters goes up", 4, &[0.0625], &[0.125]),
        ("division 4: just under halfway goes down", 4, &[0.06], &[0.0]),
        ("division 4: after the last beat, 4.3 beats is 17.2 quarters", 4, &[2.15], &[2.125]),
        ("division 64: 0.1 s is 12.8 sixty-fourths, up to 13", 64, &[0.1], &[13.0 / 128.0]),
    ];

    #[test]
    fn quantize_moves_each_event_to_the_nearest_position_and_halfway_to_the_later_one() -> Result<(), Error> {
        let steady = steady()?;
        for (id, division, given, expected) in STEADY {
            assert_eq!(snapped(division, given, &steady)?, expected, "{id}");
        }
        Ok(())
    }

    #[test]
    fn quantize_snaps_in_beats_on_a_grid_whose_tempo_changes() -> Result<(), Error> {
        // 0.4 s is 0.4 beats, 0.6 s 0.6 beats, 1.2 s 1.4 beats, 1.3 s 1.6 beats; in halves 0.3 s is 0.6, 1.2 s 2.8
        // and 1.3 s 3.2, so 0.5 s (1 half), 1.25 s (3 halves: 1.5 beats) twice
        assert_eq!(snapped(1, &[0.4, 0.6, 1.2, 1.3], &slowing()?)?, [0.0, 1.0, 1.0, 1.5]);
        assert_eq!(snapped(2, &[0.3, 1.2, 1.3], &slowing()?)?, [0.5, 1.25, 1.25]);
        Ok(())
    }

    /// `(id, beats, division, time given, time snapped)`: where the nearest position is before 0, or at it.
    #[allow(clippy::type_complexity, reason = "a table of rows, read as one")]
    const BEFORE_ZERO: [(&str, &[f64], i64, f64, f64); 12] = [
        ("the nearest position, at -0.2 s, is before 0: the next one up, the first beat", &[0.3, 0.8], 1, 0.01, 0.3),
        ("time 0: the position at -0.2 s is before 0, so the next one", &[0.3, 0.8], 1, 0.0, 0.3),
        ("the nearest position is the first beat", &[0.3, 0.8], 1, 0.1, 0.3),
        ("between the beats: 0.6 is 0.6 of the beat, up to the second", &[0.3, 0.8], 1, 0.6, 0.8),
        ("after the last beat: 1.6 beats snaps to 2", &[0.3, 0.8], 1, 1.1, 1.3),
        ("by one division: -0.05 s is before 0, so 0.2 s, not the first beat", &[0.7, 1.2], 2, 0.01, 0.2),
        ("beats 0.6, 0.8, 1.0: the position 3 beats back is at 0 within rounding", &[0.6, 0.8, 1.0], 1, 0.0, 0.0),
        ("beats 0.3, 0.4, 0.5: 0.04 is nearer the position at 0 than at 0.1", &[0.3, 0.4, 0.5], 1, 0.04, 0.0),
        ("beats 0.3, 0.4, 0.5: 0.06 is nearer the position at 0.1", &[0.3, 0.4, 0.5], 1, 0.06, 0.1),
        ("the nearest position, 1e-6 s before 0: the next one up", &[0.499_999, 0.999_999], 1, 0.0, 0.499_999),
        ("beats from 1 s: the positions at 0.5 s and 0 s are reached", &[1.0, 1.5], 1, 0.3, 0.5),
        ("in halves, 0.3 s is -2.8 halves: -3 halves is at 0.25 s, after 0, kept", &[1.0, 1.5], 2, 0.3, 0.25),
    ];

    #[test]
    fn quantize_never_moves_an_event_before_zero_but_to_the_next_position_up() -> Result<(), Error> {
        for (id, beats, division, given, expected) in BEFORE_ZERO {
            let got = snapped(division, &[given], &grid(beats, 1, 0)?)?;
            assert!(close(&got, &[expected]), "{id}: {got:?} is not {expected}");
            assert!(got.iter().all(|t| t.is_sign_positive() && *t >= 0.0), "{id}: {got:?} is before 0");
        }
        Ok(())
    }

    #[test]
    fn quantize_at_zero_within_rounding_is_zero_itself() -> Result<(), Error> {
        // the position 3 beats before 0.6 s is 0.6 - 3 * 0.2 = -1.1e-16 s in floats: 0, not a time before it
        let got = snapped(1, &[0.0, 0.04], &grid(&[0.6, 0.8, 1.0], 1, 0)?)?;
        assert_eq!(got.iter().map(|t| t.to_bits()).collect::<Vec<_>>(), [0.0_f64.to_bits(); 2], "{got:?}");
        Ok(())
    }

    #[test]
    fn quantize_keeps_events_that_snap_to_one_time_in_the_door_order() -> Result<(), Error> {
        let steady = steady()?;
        // the door orders by time (0.0 and -0.0 are one time, given order kept); all four snap to 0
        let given = [named(0.2, "a")?, named(0.1, "b")?, named(0.0, "c")?, named(-0.0, "d")?, named(0.3, "e")?];
        let kept = by_name(1, &given, &steady)?;
        assert_eq!(labels(&kept), ["c", "d", "b", "a", "e"]);
        assert_eq!(times(&kept), [0.0, 0.0, 0.0, 0.0, 0.5]);
        let swapped = [given[3].clone(), given[2].clone(), given[1].clone(), given[0].clone()];
        assert_eq!(labels(&by_name(1, &swapped, &steady)?), ["d", "c", "b", "a"]);
        // re-snapped from before 0 and snapped onto the first beat land together, in order too
        let near_zero = [named(0.0, "x")?, named(-0.0, "y")?, named(0.1, "z")?];
        let lifted = by_name(1, &near_zero, &late()?)?;
        assert_eq!(labels(&lifted), ["x", "y", "z"]);
        assert!(close(&times(&lifted), &[0.3, 0.3, 0.3]), "{lifted:?}");
        // the body keeps the order it is given
        assert_eq!(labels(&by_body(1, &[given[3].clone(), given[2].clone()], &steady)?), ["d", "c"]);
        Ok(())
    }

    #[test]
    fn quantize_keeps_the_strength_duration_label_and_pitch() -> Result<(), Error> {
        let options =
            EventOptions { strength: 0.6, duration: Some(0.25), label: Some("lead".into()), pitch: Some(64.5) };
        let note = Event::new(0.3, options.clone()).map_err(|source| Error::Format { source })?;
        let expected = Event::new(0.5, options).map_err(|source| Error::Format { source })?;
        assert_eq!(by_name(1, &[note], &steady()?)?, [expected]);
        Ok(())
    }

    #[test]
    fn quantize_of_no_events_is_no_events() -> Result<(), Error> {
        assert_eq!(snapped(1, &[], &steady()?)?, Vec::<f64>::new());
        Ok(())
    }

    #[test]
    fn quantize_far_from_the_beats_still_gives_a_finite_time() -> Result<(), Error> {
        let got = snapped(1, &[1e300], &steady()?)?;
        assert!(matches!(got[..], [t] if ((t - 1e300) / 1e300).abs() <= 1e-12), "{got:?}");
        Ok(())
    }

    #[test]
    fn quantize_refuses_a_time_too_far_from_the_beats_naming_the_events() -> Result<(), Error> {
        let rows = [
            ("a time whose beat position overflows", f64::MAX, 1),
            ("a position that overflows once multiplied by the division", 1e307, 64),
        ];
        for (id, time, division) in rows {
            let (events, steady) = (moments(&[time])?, steady()?);
            for refused in [by_name(division, &events, &steady), by_body(division, &events, &steady)] {
                let paths: Vec<String> = match refused {
                    Err(Error::Input { problems, .. }) => problems.iter().map(|problem| problem.path.clone()).collect(),
                    other => vec![format!("not an input error: {other:?}")],
                };
                assert_eq!(paths, ["events"], "{id}");
            }
        }
        Ok(())
    }

    #[test]
    fn a_division_beyond_its_range_is_refused_naming_it() -> Result<(), Error> {
        for (id, division) in [("0", 0), ("65", 65)] {
            let paths: Vec<String> = match by_name(division, &moments(&[0.5])?, &steady()?) {
                Err(Error::Params { shaper, problems, .. }) if shaper == "quantize" => {
                    problems.iter().map(|problem| problem.path.clone()).collect()
                }
                other => vec![format!("not quantize's parameter error: {other:?}")],
            };
            assert_eq!(paths, ["division"], "{id}");
        }
        Ok(())
    }

    #[test]
    fn quantize_needs_both_inputs() -> Result<(), Error> {
        let thing = Registry::builtin().get("quantize")?;
        let without = |input: (&str, Input)| Call { inputs: [(input.0.to_owned(), input.1)].into(), ..Call::default() };
        for (id, call) in [
            ("no grid", without(("events", Input::from_events(moments(&[0.5])?)))),
            ("no events", without(("grid", Input::Grid(steady()?)))),
        ] {
            assert!(matches!(thing.call(&call), Err(Error::Call { .. })), "{id}");
        }
        Ok(())
    }

    #[test]
    fn its_inputs_go_back_as_they_were_read() -> Result<(), Error> {
        let inputs = Inputs { events: moments(&[0.25, 1.0])?, grid: steady()? };
        let given: BTreeMap<String, Input> = registry::Inputs::into_inputs(inputs.clone())?
            .into_iter()
            .map(|input| (input.kind().name().to_owned(), input))
            .collect();
        assert_eq!(<Inputs as registry::Inputs>::from_inputs(&given)?, inputs);
        Ok(())
    }

    #[test]
    fn it_is_an_operation_giving_events_from_events_and_a_grid() -> Result<(), Error> {
        let thing = Registry::builtin().get("quantize")?;
        assert_eq!((thing.kind(), thing.returns()), (Kind::Operation, Returns::Events));
        assert_eq!(thing.inputs(), [(InputKind::Events, true), (InputKind::Grid, true)]);
        assert!(thing.summary().starts_with("The events, each moved to the nearest beat"), "{}", thing.summary());
        Ok(())
    }
}
