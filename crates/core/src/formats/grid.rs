//! [`Grid`]: the musical clock, stored as the time of every beat.

use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use super::error::{FormatError, Problems};
use super::field::{Number, Whole, check, decode, finite, push, push_with, required, show};

/// Beat times in seconds, strictly increasing, with the beats per bar and which beat starts the first bar. Tempo is
/// derived from the beats, never stored beside them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Grid {
    beats: Vec<f64>,
    meter: u32,
    first_downbeat: u32,
}

impl Grid {
    /// The grid of `beats` (seconds), with `meter` beats per bar, whose first bar starts at `beats[first_downbeat]`.
    ///
    /// ```
    /// use bullterrier_control::formats::Grid;
    ///
    /// // the beats slow down at 1 s: 0.5 s apart, then 0.4 s
    /// let grid = Grid::new(vec![0.0, 0.5, 1.0, 1.4, 1.8], 4, 0)?;
    /// let seconds = grid.seconds_at(&[0.5, 2.5, 5.0])?; // a half beat, then past the last beat at its length
    /// assert!((seconds[0] - 0.25).abs() < 1e-12 && (seconds[1] - 1.2).abs() < 1e-12 && (seconds[2] - 2.2).abs() < 1e-12);
    /// assert_eq!(grid.downbeats(), vec![0.0, 1.8]);
    /// # Ok::<(), bullterrier_control::formats::FormatError>(())
    /// ```
    ///
    /// # Errors
    /// A [`FormatError`] listing every rule broken, in the order of the fields: fewer than 2 beats, a beat below 0 or
    /// not finite, a beat that does not come after the one before it, `meter` below 1, and `first_downbeat` not below
    /// both `meter` and the number of beats.
    pub fn new(beats: Vec<f64>, meter: u32, first_downbeat: u32) -> Result<Self, FormatError> {
        let mut problems = Problems::new();
        Self::build(
            "",
            Some(beats),
            Some(i128::from(meter)),
            Some(i128::from(first_downbeat)),
            &mut problems,
        )
        .ok_or_else(|| FormatError::new(problems))
    }

    /// The one place the rules are written. Every rule broken is recorded in `problems` under `prefix` (empty, or where
    /// this grid sits in a document: `grid`), after what the caller already collected; `None` is a field the data
    /// lacked. Problems come in the order of the fields they are about; the rule across fields comes with the field
    /// it is about, `first_downbeat`. The grid, if none is broken.
    fn build(
        prefix: &str,
        beats: Option<Vec<f64>>,
        meter: Option<i128>,
        first_downbeat: Option<i128>,
        problems: &mut Problems,
    ) -> Option<Self> {
        let before = problems.len();
        match &beats {
            None => required(problems, prefix, "beats"),
            Some(beats) => {
                if beats.len() < 2 {
                    push(problems, prefix, "beats", "needs at least 2 beats");
                }
                for (index, &beat) in beats.iter().enumerate() {
                    let field = format_args!("beats[{index}]");
                    check(problems, prefix, field, beat, "must be >= 0", |x| x >= 0.0);
                    if let Some(&earlier) = index.checked_sub(1).and_then(|i| beats.get(i))
                        && beat.is_finite()
                        && earlier.is_finite()
                        && beat <= earlier
                    {
                        let rule = format!(
                            "{} does not come after beats[{}] = {}",
                            show(beat),
                            index - 1,
                            show(earlier)
                        );
                        push(problems, prefix, field, rule);
                    }
                }
            }
        }
        match meter {
            None => required(problems, prefix, "meter"),
            Some(meter) if meter < 1 => push(problems, prefix, "meter", format!("must be >= 1 (got {meter})")),
            Some(meter) if meter > i128::from(u32::MAX) => push(
                problems,
                prefix,
                "meter",
                format!("must be <= {} (got {meter})", u32::MAX),
            ),
            Some(_) => {}
        }
        match first_downbeat {
            None => required(problems, prefix, "first_downbeat"),
            Some(first) if first < 0 => {
                push(
                    problems,
                    prefix,
                    "first_downbeat",
                    format!("must be >= 0 (got {first})"),
                );
            }
            Some(first) => {
                // not against a meter or a beat count already found broken: that would be the same fault twice
                if let (Some(meter), Some(beats)) = (meter, &beats)
                    && (1..=i128::from(u32::MAX)).contains(&meter)
                    && beats.len() >= 2
                {
                    let count = i128::try_from(beats.len()).unwrap_or(i128::MAX);
                    if first >= meter.min(count) {
                        let rule = format!("{first} must be a beat inside a bar of meter = {meter} ({count} beats)");
                        push(problems, prefix, "first_downbeat", rule);
                    }
                }
            }
        }
        match (beats, meter, first_downbeat) {
            (Some(beats), Some(meter), Some(first)) if problems.len() == before => Some(Self {
                beats,
                // the rules above hold both below `u32::MAX`: `first` is below `meter`
                meter: u32::try_from(meter).ok()?,
                first_downbeat: u32::try_from(first).ok()?,
            }),
            _ => None,
        }
    }

    /// Read a grid from any serde format and check it with [`Grid::new`]'s rules.
    ///
    /// # Errors
    /// A [`FormatError`]: the one problem that stopped the reading (a wrong type, `meter` written as a decimal, an
    /// unknown or repeated field, something that is not an object), or, when the data was read, every rule it breaks.
    pub fn from_deserializer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, FormatError> {
        let raw: RawGrid = decode(deserializer)?;
        let mut problems = Problems::new();
        raw.read("", &mut problems).ok_or_else(|| FormatError::new(problems))
    }

    /// The time of every beat, in seconds.
    #[must_use]
    pub fn beats(&self) -> &[f64] {
        &self.beats
    }

    /// Beats per bar.
    #[must_use]
    pub const fn meter(&self) -> u32 {
        self.meter
    }

    /// The index of the first beat that starts a bar.
    #[must_use]
    pub const fn first_downbeat(&self) -> u32 {
        self.first_downbeat
    }

    /// The time of the first beat of every bar: `beats[first_downbeat::meter]`.
    #[must_use]
    pub fn downbeats(&self) -> Vec<f64> {
        // `usize::MAX` for a meter no slice reaches past: one downbeat
        let every = usize::try_from(self.meter).unwrap_or(usize::MAX);
        let first = usize::try_from(self.first_downbeat).unwrap_or(usize::MAX);
        self.beats.iter().skip(first).step_by(every).copied().collect()
    }

    /// The time in seconds of each beat position in `positions`.
    ///
    /// Position 0 is the first beat, 1 the second, and so on; a fraction is linear between two beats. Before the first
    /// beat and after the last, the first or last beat length continues, so every position has a time.
    ///
    /// # Errors
    /// A [`FormatError`] naming each position that is not finite (`positions[2]`), or, when all are, each whose time is
    /// not (too far from the beats for an `f64`).
    pub fn seconds_at(&self, positions: &[f64]) -> Result<Vec<f64>, FormatError> {
        convert("positions", positions, |position| self.time_at(position))
    }

    /// The beat position of each time in `seconds`: the inverse of [`Grid::seconds_at`], with the same extension
    /// outside the beats.
    ///
    /// # Errors
    /// A [`FormatError`] naming each time that is not finite (`seconds[2]`), or, when all are, each whose position is
    /// not (too far from the beats for an `f64`).
    pub fn beat_at(&self, seconds: &[f64]) -> Result<Vec<f64>, FormatError> {
        convert("seconds", seconds, |moment| self.position_at(moment))
    }

    // `beats` holds two or more beats, finite and increasing, as `build` makes sure: the indexing below is inside it.

    fn time_at(&self, position: f64) -> f64 {
        let beats = &self.beats;
        let last = beats.len() - 1;
        let last_position = whole(last);
        if position <= 0.0 {
            beats[0] + position * (beats[1] - beats[0])
        } else if position >= last_position {
            beats[last] + (position - last_position) * (beats[last] - beats[last - 1])
        } else {
            let index = segment(position);
            beats[index] + (position - whole(index)) * (beats[index + 1] - beats[index])
        }
    }

    fn position_at(&self, moment: f64) -> f64 {
        let beats = &self.beats;
        let last = beats.len() - 1;
        if moment <= beats[0] {
            (moment - beats[0]) / (beats[1] - beats[0])
        } else if moment >= beats[last] {
            whole(last) + (moment - beats[last]) / (beats[last] - beats[last - 1])
        } else {
            // the beat at or before `moment`: one exists, `moment` is after the first
            let index = beats.partition_point(|&beat| beat <= moment).saturating_sub(1);
            whole(index) + (moment - beats[index]) / (beats[index + 1] - beats[index])
        }
    }
}

/// `convert` every one of `values` (named `name`, in a message `name[2]`), or say which is not finite or has a result
/// that is not.
fn convert(name: &str, values: &[f64], convert: impl Fn(f64) -> f64) -> Result<Vec<f64>, FormatError> {
    let mut problems = Problems::new();
    for (index, &value) in values.iter().enumerate() {
        finite(&mut problems, "", format_args!("{name}[{index}]"), value);
    }
    if !problems.is_empty() {
        return Err(FormatError::new(problems));
    }
    let converted: Vec<f64> = values.iter().map(|&value| convert(value)).collect();
    for (index, (&value, result)) in values.iter().zip(&converted).enumerate() {
        if !result.is_finite() {
            push_with(&mut problems, "", format_args!("{name}[{index}]"), || {
                format!(
                    "{} is too far from the beats: its result is not a finite number",
                    show(value)
                )
            });
        }
    }
    if problems.is_empty() {
        Ok(converted)
    } else {
        Err(FormatError::new(problems))
    }
}

/// A beat count or index as a number.
#[expect(clippy::cast_precision_loss, reason = "a count of beats is far below 2^53")]
const fn whole(index: usize) -> f64 {
    index as f64
}

/// The index of the beat a position strictly between the first and last beat falls after.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a position above 0 and below the last index"
)]
fn segment(position: f64) -> usize {
    position.floor() as usize
}

// The fields as a file spells them, before any rule is checked; the one description of the file's shape the bundle
// schema is generated from (the attributes state the rules a schema can).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "Grid",
    description = "Beat times in seconds, strictly increasing. Tempo is derived from them, never stored beside them."
)]
pub(crate) struct RawGrid {
    #[schemars(with = "Vec<Number>", length(min = 2), inner(range(min = 0)))]
    beats: Option<Vec<Number>>,
    #[schemars(with = "Whole", range(min = 1))]
    meter: Option<Whole>,
    #[schemars(with = "Whole", range(min = 0))]
    first_downbeat: Option<Whole>,
}

impl RawGrid {
    /// Check the rules of [`Grid::new`], recording each broken one in `problems` under `prefix` (`grid`); the grid if
    /// none is broken.
    pub(crate) fn read(self, prefix: &str, problems: &mut Problems) -> Option<Grid> {
        Grid::build(
            prefix,
            self.beats
                .map(|beats| beats.into_iter().map(|number| number.0).collect()),
            self.meter.map(|whole| whole.0),
            self.first_downbeat.map(|whole| whole.0),
            problems,
        )
    }
}

impl<'de> Deserialize<'de> for Grid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_deserializer(deserializer).map_err(D::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "the getters give back exactly what was given")]
mod tests {
    use super::super::test_support::{Expected, check_against};
    use super::*;
    use proptest::prelude::*;

    /// A case: its name, the beats, `meter`, `first_downbeat` and the problems wanted.
    type Row = (&'static str, Vec<f64>, u32, u32, Expected);

    #[test]
    fn each_rule_holds_on_one_side_of_its_boundary_and_breaks_on_the_other() {
        let eight: Vec<f64> = (0..8).map(f64::from).collect();
        #[rustfmt::skip]
        let rows: Vec<Row> = vec![
            ("two beats, one per bar, the smallest grid", vec![0.0, 1.0], 1, 0, &[]),
            ("a first beat of negative zero is zero", vec![-0.0, 1.0], 4, 1, &[]),
            ("one beat", vec![0.0], 1, 0, &[("beats", "at least 2")]),
            ("no beats", vec![], 1, 0, &[("beats", "at least 2")]),
            ("one beat and a first downbeat past it", vec![0.0], 4, 1, &[("beats", "at least 2")]),
            ("a beat below zero", vec![-0.5, 1.0], 1, 0, &[("beats[0]", ">= 0 (got -0.5)")]),
            ("a NaN beat", vec![0.0, f64::NAN, 2.0], 1, 0, &[("beats[1]", "finite (got NaN)")]),
            ("an infinite beat", vec![0.0, f64::INFINITY], 1, 0, &[("beats[1]", "finite (got Infinity)")]),
            ("a repeated beat", vec![0.0, 1.0, 1.0], 1, 0, &[("beats[2]", "1.0 does not come after beats[1] = 1.0")]),
            ("a beat out of order", vec![0.0, 2.0, 1.0], 1, 0, &[("beats[2]", "1.0 does not come after beats[1] = 2.0")]),
            ("two beats out of order", vec![0.0, 3.0, 2.0, 1.0], 1, 0,
                &[("beats[2]", "2.0 does not come after beats[1] = 3.0"), ("beats[3]", "after beats[2] = 2.0")]),
            ("meter zero", eight.clone(), 0, 0, &[("meter", ">= 1 (got 0)")]),
            ("meter zero and a first downbeat: one fault, not two", eight.clone(), 0, 3, &[("meter", ">= 1 (got 0)")]),
            ("first downbeat just inside the meter", eight.clone(), 4, 3, &[]),
            ("first downbeat equal to the meter", eight.clone(), 4, 4, &[("first_downbeat", "4 must be a beat inside a bar of meter = 4 (8 beats)")]),
            ("first downbeat just inside the beats", vec![0.0, 1.0, 2.0], 8, 2, &[]),
            ("first downbeat equal to the number of beats", vec![0.0, 1.0, 2.0], 8, 3, &[("first_downbeat", "3 must be a beat inside a bar of meter = 8 (3 beats)")]),
            ("beats and meter broken", vec![0.0, 0.0], 0, 5,
                &[("beats[1]", "does not come after"), ("meter", ">= 1")]),
            ("beats and first downbeat broken", vec![0.0, 0.0], 2, 5,
                &[("beats[1]", "does not come after"), ("first_downbeat", "5 must be a beat inside a bar of meter = 2 (2 beats)")]),
        ];
        for (id, beats, meter, first_downbeat, expected) in rows {
            check_against(id, Grid::new(beats, meter, first_downbeat), expected);
        }
    }

    #[test]
    fn a_grid_keeps_what_it_was_given() -> Result<(), FormatError> {
        let grid = Grid::new(vec![0.5, 1.0, 1.75], 3, 2)?;
        assert_eq!(
            (grid.beats(), grid.meter(), grid.first_downbeat()),
            (&[0.5, 1.0, 1.75][..], 3, 2)
        );
        Ok(())
    }

    #[test]
    fn the_downbeats_are_every_meter_th_beat_from_the_first_downbeat() -> Result<(), FormatError> {
        let ten: Vec<f64> = (0..10).map(f64::from).collect();
        for (meter, first, wanted) in [
            (4, 1, vec![1.0, 5.0, 9.0]),
            (4, 0, vec![0.0, 4.0, 8.0]),
            (4, 3, vec![3.0, 7.0]),
            (1, 0, ten.clone()),
            (10, 9, vec![9.0]),
        ] {
            let grid = Grid::new(ten.clone(), meter, first)?;
            assert_eq!(grid.downbeats(), wanted, "meter {meter}, first downbeat {first}");
        }
        Ok(())
    }

    fn shifted() -> Result<Grid, FormatError> {
        // every beat length different: the first 0.5 s, the middle 1 s, the last 0.5 s
        Grid::new(vec![0.0, 0.5, 1.5, 2.0], 2, 0)
    }

    fn lengthening() -> Result<Grid, FormatError> {
        // the first beat is 1 s long, the middle one 2 s, the last 6 s
        Grid::new(vec![1.0, 2.0, 4.0, 10.0], 4, 0)
    }

    /// A grid, then the pairs (position, seconds) that hand work says belong together on it.
    type Pairs = (&'static str, Grid, Vec<(f64, f64)>);

    /// A case of a conversion: its name, the result and the problems wanted.
    type Conversion = (&'static str, Result<Vec<f64>, FormatError>, Expected);

    #[test]
    fn positions_and_times_pair_up_linearly_inside_and_by_the_end_beat_lengths_outside() -> Result<(), FormatError> {
        #[rustfmt::skip]
        let pairs: [Pairs; 2] = [
            ("beats at 0, 0.5, 1.5, 2", shifted()?, vec![
                (0.0, 0.0), (1.0, 0.5), (2.0, 1.5), (3.0, 2.0),    // the beats themselves
                (0.5, 0.25), (1.5, 1.0), (2.5, 1.75),              // halfway between two
                (0.25, 0.125), (1.75, 1.25),                       // a quarter into a beat
                (-1.0, -0.5), (-2.0, -1.0),                        // before the first beat: its length of 0.5 continues
                (4.0, 2.5), (5.5, 3.25),                           // after the last beat: its length of 0.5 continues
            ]),
            ("beats at 1, 2, 4, 10", lengthening()?, vec![
                (0.0, 1.0), (1.0, 2.0), (2.0, 4.0), (3.0, 10.0),
                (1.5, 3.0), (2.25, 5.5), (2.5, 7.0),
                (-1.0, 0.0), (-2.5, -1.5),                         // the first beat is 1 s long
                (4.0, 16.0), (3.5, 13.0), (5.0, 22.0),             // the last beat is 6 s long
            ]),
        ];
        for (id, grid, pairs) in pairs {
            let (positions, times): (Vec<f64>, Vec<f64>) = pairs.into_iter().unzip();
            assert_eq!(grid.seconds_at(&positions)?, times, "{id}: seconds_at");
            assert_eq!(grid.beat_at(&times)?, positions, "{id}: beat_at");
        }
        Ok(())
    }

    #[test]
    fn no_position_or_time_is_a_no_op_and_the_answer_has_the_same_length() -> Result<(), FormatError> {
        let grid = shifted()?;
        assert_eq!(grid.seconds_at(&[])?, Vec::<f64>::new());
        assert_eq!(grid.beat_at(&[])?, Vec::<f64>::new());
        assert_eq!(grid.seconds_at(&[0.0, 1.0, 2.0, 3.0, 4.0])?.len(), 5);
        Ok(())
    }

    #[test]
    fn a_conversion_of_many_broken_values_keeps_the_first_hundred_and_counts_the_rest() -> Result<(), FormatError> {
        let grid = lengthening()?;
        for result in [
            grid.seconds_at(&[f64::NAN; 1_000]),
            grid.beat_at(&[f64::INFINITY; 1_000]),
        ] {
            let Err(error) = result else {
                return Err(FormatError::at("", "accepted"));
            };
            assert_eq!(error.problems.len(), 100);
            assert_eq!(error.omitted(), 900);
        }
        let hair = Grid::new(vec![0.0, 1.0e-300], 1, 0)?;
        let Err(error) = hair.beat_at(&[1.0e10; 1_000]) else {
            return Err(FormatError::at("", "accepted"));
        };
        assert_eq!((error.problems.len(), error.omitted()), (100, 900));
        Ok(())
    }

    #[test]
    fn what_is_not_finite_or_cannot_be_reached_is_refused_naming_where() -> Result<(), FormatError> {
        let grid = lengthening()?;
        // a time 1e-300 s from the first, so a position 1e10 beats away is a time beyond f64
        let hair = Grid::new(vec![0.0, 1.0e-300], 1, 0)?;
        // a first beat 1e10 s long: a position 1e308 beats before is a time beyond f64
        let wide = Grid::new(vec![0.0, 1.0e10], 1, 0)?;
        #[rustfmt::skip]
        let rows: Vec<Conversion> = vec![
            ("seconds_at: NaN", grid.seconds_at(&[0.0, f64::NAN]), &[("positions[1]", "finite (got NaN)")]),
            ("seconds_at: two infinities", grid.seconds_at(&[f64::INFINITY, 1.0, f64::NEG_INFINITY]),
                &[("positions[0]", "got Infinity"), ("positions[2]", "got -Infinity")]),
            ("seconds_at: far beyond the beats", grid.seconds_at(&[1.0, 1.0e308]), &[("positions[1]", "too far from the beats")]),
            ("seconds_at: far before the beats", wide.seconds_at(&[-1.0e308]), &[("positions[0]", "too far from the beats")]),
            ("beat_at: NaN", grid.beat_at(&[f64::NAN, 1.0]), &[("seconds[0]", "finite (got NaN)")]),
            ("beat_at: an infinity", grid.beat_at(&[f64::NEG_INFINITY]), &[("seconds[0]", "got -Infinity")]),
            ("beat_at: a short beat and a far time", hair.beat_at(&[1.0e10]), &[("seconds[0]", "too far from the beats")]),
            ("beat_at: far before a short beat", hair.beat_at(&[-1.0e10]), &[("seconds[0]", "too far from the beats")]),
            ("seconds_at: a large position that still fits", grid.seconds_at(&[1.0e300]), &[]),
            ("beat_at: a large time that still fits", grid.beat_at(&[1.0e300]), &[]),
        ];
        for (id, result, expected) in rows {
            check_against(id, result, expected);
        }
        Ok(())
    }

    fn read(text: &str) -> Result<Grid, FormatError> {
        Grid::from_deserializer(&mut serde_json::Deserializer::from_str(text))
    }

    #[test]
    fn json_is_read_strictly_and_every_broken_rule_is_listed_with_its_path() {
        #[rustfmt::skip]
        let rows: &[(&str, &str, Expected)] = &[
            ("a grid", r#"{"beats": [0, 0.5, 1.25], "meter": 4, "first_downbeat": 1}"#, &[]),
            ("nothing said", "{}", &[("beats", "is required"), ("meter", "is required"), ("first_downbeat", "is required")]),
            ("null is not said", r#"{"beats": null, "meter": null, "first_downbeat": 0}"#, &[("beats", "is required"), ("meter", "is required")]),
            ("meter as a decimal", r#"{"beats": [0, 1], "meter": 4.0, "first_downbeat": 0}"#, &[("meter", "whole number (got 4.0)")]),
            ("first downbeat as a decimal", r#"{"beats": [0, 1], "meter": 4, "first_downbeat": 0.0}"#, &[("first_downbeat", "whole number")]),
            ("meter as text", r#"{"beats": [0, 1], "meter": "4", "first_downbeat": 0}"#, &[("meter", "string")]),
            ("meter as a bool", r#"{"beats": [0, 1], "meter": true, "first_downbeat": 0}"#, &[("meter", "boolean")]),
            ("a negative meter", r#"{"beats": [0, 1], "meter": -4, "first_downbeat": 0}"#, &[("meter", ">= 1 (got -4)")]),
            ("a negative first downbeat", r#"{"beats": [0, 1], "meter": 4, "first_downbeat": -1}"#, &[("first_downbeat", ">= 0 (got -1)")]),
            ("a meter beyond what a count holds", r#"{"beats": [0, 1], "meter": 4294967296, "first_downbeat": 0}"#, &[("meter", "<= 4294967295")]),
            ("a bool beat", r#"{"beats": [0, false], "meter": 4, "first_downbeat": 0}"#, &[("beats[1]", "boolean")]),
            ("an unknown field", r#"{"beats": [0, 1], "meter": 4, "first_downbeat": 0, "tempo": 120}"#, &[("tempo", "unknown field")]),
            ("a repeated field", r#"{"beats": [0, 1], "meter": 4, "meter": 3, "first_downbeat": 0}"#, &[("meter", "duplicate")]),
            ("an array", "[[0, 1], 4, 0]", &[("", "an object")]),
            ("rules broken in several fields", r#"{"beats": [1, 1], "meter": 2, "first_downbeat": 2}"#,
                &[("beats[1]", "does not come after"), ("first_downbeat", "2 must be a beat inside a bar of meter = 2 (2 beats)")]),
        ];
        for (id, text, expected) in rows {
            check_against(id, read(text), expected);
        }
    }

    prop_compose! {
        /// Strictly increasing beats: a start and positive gaps, with the grid's meter and a first downbeat inside it.
        fn grids()(
            start in 0.0..100.0_f64,
            gaps in prop::collection::vec(0.01..10.0_f64, 1..20),
        ) -> Vec<f64> {
            gaps.iter().scan(start, |at, gap| { *at += gap; Some(*at) }).fold(vec![start], |mut all, at| { all.push(at); all })
        }
    }

    proptest! {
        #[test]
        fn a_position_read_back_as_a_time_and_a_time_as_a_position_are_the_same_place(
            beats in grids(),
            positions in prop::collection::vec(-60.0..80.0_f64, 0..30),
        ) {
            let grid = Grid::new(beats, 1, 0).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let times = grid.seconds_at(&positions).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let back = grid.beat_at(&times).map_err(|e| TestCaseError::fail(e.to_string()))?;
            for (position, again) in positions.iter().zip(&back) {
                prop_assert!((position - again).abs() <= 1e-9 * (1.0 + position.abs()), "{position} came back as {again}");
            }
        }

        #[test]
        fn later_positions_are_later_times(
            beats in grids(),
            mut positions in prop::collection::vec(-60.0..80.0_f64, 2..30),
        ) {
            let grid = Grid::new(beats, 1, 0).map_err(|e| TestCaseError::fail(e.to_string()))?;
            positions.sort_by(f64::total_cmp);
            let times = grid.seconds_at(&positions).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert!(times.is_sorted(), "{times:?}");
        }

        #[test]
        fn a_whole_position_inside_the_beats_is_exactly_that_beat(beats in grids()) {
            let grid = Grid::new(beats.clone(), 1, 0).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let positions: Vec<f64> = (0..beats.len()).map(|index| f64::from(u32::try_from(index).unwrap_or(0))).collect();
            prop_assert_eq!(grid.seconds_at(&positions), Ok(beats.clone()));
            prop_assert_eq!(grid.beat_at(&beats), Ok(positions));
        }

        #[test]
        fn a_valid_grid_survives_json_text_exactly(beats in grids(), meter in 1_u32..9, first in 0_u32..9) {
            prop_assume!(first < meter.min(u32::try_from(beats.len()).unwrap_or(0)));
            let grid = Grid::new(beats, meter, first).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let text = serde_json::to_string(&grid).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(read(&text), Ok(grid), "{}", text);
        }
    }
}
