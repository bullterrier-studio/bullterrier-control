//! Where a read falls in a periodic source: its position, counted in cycles or steps per second or per beat.
//!
//! ```
//! use bullterrier_control::cadence::{Per, positions};
//! use bullterrier_control::formats::Grid;
//!
//! // beats 0.5 s, then 0.6 s, then 0.7 s long
//! let grid = Grid::new(vec![0.0, 0.5, 1.1, 1.8], 4, 0)?;
//! // two cycles per beat: halfway through each beat is a whole cycle further
//! assert_eq!(positions(2.0, Per::Beat, Some(&grid), &[0.25, 0.8, 1.45])?, [1.0, 3.0, 5.0]);
//! // three per second
//! assert_eq!(positions(3.0, Per::Second, None, &[0.0, 0.5, 2.0])?, [0.0, 1.5, 6.0]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Promises:
//! - `positions` turns times (finite, from 0 up) into positions: `rate * t` per second, `rate * grid.beat_at(t)` per
//!   beat, so a source follows a tempo change. Every position is finite; its whole part says which cycle or step, its
//!   fraction where in it. It is negative for a time before a first beat that is not at 0. A step or cycle edge sits
//!   where `rate * t` reaches a whole number, to within float rounding: a time that is a multiple of `1 / rate` in
//!   decimal (frame times at rate 30) may read the step before.
//! - [`Per::Beat`] without a grid is an [`Error::Params`] naming `per`; a grid is ignored per second.
//! - A time too far for the position to be a number (`rate * t` overflows, or the grid cannot reach it) is an
//!   [`Error::Signal`].

use crate::formats::{Grid, Problem};
use crate::{Choice, Error};

/// What a source counts its cycles or steps in: the choice of its parameter `per` ('second' or 'beat').
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
#[non_exhaustive]
pub enum Per {
    /// Seconds: the rate is cycles or steps per second.
    Second,
    /// Beats of a [`Grid`]: the rate is cycles or steps per beat.
    Beat,
}

/// The position of each of `times` (seconds): `rate` cycles or steps per second or per beat of `grid`.
///
/// ```
/// use bullterrier_control::cadence::{Per, positions};
///
/// assert_eq!(positions(0.0, Per::Second, None, &[0.0, 9.0])?, [0.0, 0.0]); // rate 0 never moves
/// let refused = positions(1.0, Per::Beat, None, &[0.0]).map_err(|error| error.to_string());
/// assert!(matches!(refused, Err(text) if text.contains("per: 'beat' needs a grid")));
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
///
/// # Errors
/// [`Error::Params`] (its field `per`) if `per` is [`Per::Beat`] and there is no `grid`; [`Error::Signal`], naming the
/// time, if a time is too far from 0 for its position to be a finite number (the grid cannot reach it, or `rate * t`
/// overflows) or is not finite itself.
pub fn positions(rate: f64, per: Per, grid: Option<&Grid>, times: &[f64]) -> Result<Vec<f64>, Error> {
    let amounts = match (per, grid) {
        (Per::Second, _) => times.to_vec(),
        (Per::Beat, Some(grid)) => grid.beat_at(times).map_err(|error| Error::Signal {
            message: error.to_string(),
        })?,
        (Per::Beat, None) => {
            return Err(Error::Params {
                shaper: "cadence".to_owned(),
                problems: vec![Problem {
                    path: "per".to_owned(),
                    rule: "'beat' needs a grid".to_owned(),
                }],
                omitted: 0,
            });
        }
    };
    let counted: Vec<f64> = amounts.iter().map(|amount| rate * amount).collect();
    match counted.iter().position(|position| !position.is_finite()) {
        None => Ok(counted),
        Some(index) => Err(Error::Signal {
            message: format!(
                "the position is not a finite number: the time {:e} is too far from 0",
                times.get(index).copied().unwrap_or(f64::NAN)
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{Per, positions};
    use crate::Error;
    use crate::formats::Grid;

    /// Beats of 0.5, 0.6 and 0.7 s.
    fn slowing() -> Result<Grid, crate::formats::FormatError> {
        Grid::new(vec![0.0, 0.5, 1.1, 1.8], 4, 0)
    }

    /// Beats of 0.5 s from 1.0 s on.
    fn late() -> Result<Grid, crate::formats::FormatError> {
        Grid::new(vec![1.0, 1.5, 2.0], 1, 0)
    }

    // each expected position is worked out by hand from the beat lengths of the grid
    #[test]
    fn the_position_counts_per_second_or_per_beat_of_the_grid() -> Result<(), Box<dyn std::error::Error>> {
        let (slowing, late) = (slowing()?, late()?);
        for (rate, per, grid, times, expected, rule) in [
            (
                3.0,
                Per::Second,
                None,
                vec![0.0, 0.5, 2.0],
                vec![0.0, 1.5, 6.0],
                "per second: rate times t",
            ),
            (
                0.0,
                Per::Second,
                None,
                vec![0.0, 9.0],
                vec![0.0, 0.0],
                "rate 0 never moves",
            ),
            (
                2.0,
                Per::Second,
                Some(&slowing),
                vec![1.0],
                vec![2.0],
                "a grid is ignored per second",
            ),
            (
                1.0,
                Per::Beat,
                Some(&slowing),
                vec![0.0, 0.5, 1.1, 1.8],
                vec![0.0, 1.0, 2.0, 3.0],
                "a beat is one count",
            ),
            (
                2.0,
                Per::Beat,
                Some(&slowing),
                vec![0.25, 0.8, 1.45],
                vec![1.0, 3.0, 5.0],
                "halfway through each beat",
            ),
            (
                1.0,
                Per::Beat,
                Some(&slowing),
                vec![2.5],
                vec![4.0],
                "past the last beat: its length (0.7 s) goes on",
            ),
            (
                1.0,
                Per::Beat,
                Some(&late),
                vec![0.0, 0.75],
                vec![-2.0, -0.5],
                "before the first beat: negative",
            ),
            (
                1000.0,
                Per::Second,
                None,
                vec![1e300],
                vec![1e303],
                "a far time the position can hold is still read",
            ),
            (1.0, Per::Second, None, vec![], vec![], "no times, no positions"),
        ] {
            let got = positions(rate, per, grid, &times)?;
            assert_eq!(got.len(), expected.len(), "{rule}");
            for (got, expected) in got.iter().zip(&expected) {
                assert!(
                    (got - expected).abs() <= 1e-9 * (1.0 + expected.abs()),
                    "{rule}: {got} vs {expected}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn per_beat_without_a_grid_is_refused_naming_per() {
        let refused = positions(1.0, Per::Beat, None, &[0.0]);
        assert!(
            matches!(&refused, Err(Error::Params { problems, .. }) if problems.len() == 1 && problems[0].path == "per"),
            "{refused:?}"
        );
    }

    #[test]
    fn a_time_too_far_for_a_position_is_a_signal_error_naming_it() -> Result<(), Box<dyn std::error::Error>> {
        let slowing = slowing()?;
        for (per, rate, times, rule) in [
            (
                Per::Second,
                1000.0,
                vec![0.0, 1.797e308],
                "rate times the time overflows",
            ),
            (Per::Beat, 1.0, vec![0.0, 1.797e308], "the grid cannot reach the time"),
            (Per::Second, 1.0, vec![0.0, f64::NAN], "a time that is not a number"),
            (Per::Second, 1.0, vec![f64::INFINITY], "an infinite time"),
            (
                Per::Beat,
                1.0,
                vec![0.5, f64::NAN],
                "a time that is not a number, per beat",
            ),
        ] {
            let refused = positions(rate, per, Some(&slowing), &times);
            assert!(matches!(&refused, Err(Error::Signal { .. })), "{rule}: {refused:?}");
        }
        let refused = positions(1000.0, Per::Second, None, &[0.0, 1.797e308]).map_err(|error| error.to_string());
        assert!(
            matches!(&refused, Err(text) if text.contains("1.797e308")),
            "{refused:?}"
        );
        Ok(())
    }

    proptest! {
        // a grid whose beats are `spacing` apart: the position per beat is `rate * t / spacing`, worked out directly
        #[test]
        fn a_steady_grid_gives_rate_times_time_over_the_beat_length(
            spacing in 0.2_f64..=2.0,
            rate in 0.0_f64..=8.0,
            times in prop::collection::vec(0.0_f64..=100.0, 1..20),
        ) {
            let beats: Vec<f64> = (0..200).map(|index| f64::from(index) * spacing).collect();
            let grid = Grid::new(beats, 4, 0).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let got = positions(rate, Per::Beat, Some(&grid), &times).map_err(|error| TestCaseError::fail(error.to_string()))?;
            for (position, time) in got.iter().zip(&times) {
                let expected = rate * time / spacing;
                prop_assert!((position - expected).abs() <= 1e-9 * (1.0 + expected.abs()), "{} vs {}", position, expected);
            }
        }

        #[test]
        fn positions_never_fall_as_time_rises(a in 0.0_f64..=1e6, b in 0.0_f64..=1e6, rate in 0.0_f64..=100.0) {
            let (early, late) = if a <= b { (a, b) } else { (b, a) };
            let got = positions(rate, Per::Second, None, &[early, late]).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert!(got[0] <= got[1]);
        }
    }
}
