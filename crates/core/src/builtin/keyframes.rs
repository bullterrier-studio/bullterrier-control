//! A signal that passes through the given values at the given times, easing from each point to the next.
//!
//! Before the first point it holds the first value, from the last point on the last value. The segment from one point
//! to the next follows the ease of the point it starts at (the last point's is ignored). A read exactly at a point
//! gives that point's value exactly, and between two points of one value that value is returned exactly.

use std::convert::Infallible;

use crate::inputs::{Keyframe, NO_POINTS, empty};
use crate::{Error, Signal, Source};

/// Keyframes have no parameters: the points, with their eases, say everything.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params;

impl Source for Params {
    const NAME: &'static str = "keyframes";
    type Inputs = Vec<Keyframe>;

    /// The signal through `points` (at least one, `t` strictly increasing, as the door checks them).
    ///
    /// ```
    /// use bullterrier_control::easing::Ease;
    /// use bullterrier_control::inputs::Keyframe;
    /// use bullterrier_control::lib;
    ///
    /// // from 0 at 1 s to 1 at 3 s, eased in (p squared): a quarter of the way is 1/16 of the rise
    /// let fade = lib::keyframes(&[Keyframe::at(1.0, 0.0).ease(Ease::In), Keyframe::at(3.0, 1.0)])?;
    /// assert_eq!(fade.sample(&[0.0, 1.5, 3.0, 9.0])?, [0.0, 0.0625, 1.0, 1.0]);
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Input`] if there are no `points` (the door's refusal of them, here too when called directly). Every
    /// read of the signal is a value.
    fn source(&self, points: Vec<Keyframe>) -> Result<Signal, Error> {
        let Some(&first) = points.first() else {
            return Err(empty(NO_POINTS));
        };
        Ok(Signal::from_fn(move |times, out| {
            for (value, time) in out.iter_mut().zip(times) {
                *value = value_at(first, &points, *time);
            }
            Ok::<(), Infallible>(())
        }))
    }
}

/// The value at `time` of `points` (`t` strictly increasing), whose first is `first`.
#[expect(clippy::float_cmp, reason = "a held value is two values that are equal, not near")]
fn value_at(first: Keyframe, points: &[Keyframe], time: f64) -> f64 {
    // a read exactly at a point belongs to the segment that starts there, so it gives that point's value
    let segment = points.partition_point(|point| point.t <= time);
    let Some(start) = segment.checked_sub(1).and_then(|before| points.get(before)) else {
        return first.value;
    };
    let Some(end) = points.get(segment) else {
        return start.value;
    };
    let eased = start.ease.apply((time - start.t) / (end.t - start.t));
    // a + (b - a) * e would overflow for values near the float limit; (1 - e) * a + e * b is exact at e = 0 and e = 1
    // but not when a == b (a held value must read back exactly), so that case is taken as it is
    if start.value == end.value {
        start.value
    } else {
        (1.0 - eased) * start.value + eased * end.value
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "the values worked out by hand are exact in binary")]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;
    use crate::easing::Ease;
    use crate::formats::Problem;
    use crate::inputs::Input;
    use crate::registry::{Call, Kind, Output};
    use crate::{Registry, lib};

    fn at(t: f64, value: f64, ease: Ease) -> Keyframe {
        Keyframe::at(t, value).ease(ease)
    }

    /// The signs of `values`, `true` where negative: what tells -0.0 from 0.0.
    fn signs(values: &[f64]) -> Vec<bool> {
        values.iter().map(|value| value.is_sign_negative()).collect()
    }

    // Three segments of different lengths and eases (2 s `in`, 4 s `out`, 1 s `cubic-in`); the last point's ease is
    // ignored. Each expected value is (1 - e) a + e b with p = (t - a.t) / (b.t - a.t), worked out by hand.
    #[test]
    fn each_segment_follows_the_ease_of_the_point_it_starts_at() -> Result<(), Error> {
        let four = lib::keyframes(&[
            at(0.0, 0.0, Ease::In),
            at(2.0, 1.0, Ease::Out),
            at(6.0, 0.5, Ease::CubicIn),
            at(7.0, 2.0, Ease::InOut),
        ])?;
        let rows = [
            (0.5, 0.0625, "p 0.25 in: 0.25^2"),
            (1.0, 0.25, "p 0.5 in: 0.5^2"),
            (3.0, 0.78125, "p 0.25 out: 1 - 0.75^2 = 0.4375 of the fall from 1 to 0.5"),
            (5.0, 0.53125, "p 0.75 out: 1 - 0.25^2 = 0.9375 of the fall"),
            (6.5, 0.6875, "p 0.5 cubic-in: 0.125 of the rise from 0.5 to 2"),
            (6.75, 1.132_812_5, "p 0.75 cubic-in: 0.421875 of the rise"),
            (0.0, 0.0, "at the first point"),
            (2.0, 1.0, "at a point"),
            (6.0, 0.5, "at a point"),
            (7.0, 2.0, "at the last point"),
            (7.5, 2.0, "after the last point, whatever its ease"),
        ];
        for (time, expected, row) in rows {
            assert_eq!(four.sample(&[time])?, [expected], "t = {time}: {row}");
        }
        // in one read, in any order, the same values
        let (times, expected): (Vec<f64>, Vec<f64>) = rows.iter().rev().map(|(time, value, _)| (*time, *value)).unzip();
        assert_eq!(four.sample(&times)?, expected);
        Ok(())
    }

    #[test]
    fn every_ease_bends_its_segment() -> Result<(), Error> {
        // from 2 at 1 s to 6 at 3 s, read at 1.5 s (p = 0.25): 2 + 4 e, each e worked out by hand
        let rows = [
            (Ease::Linear, 0.25),
            (Ease::Hold, 0.0),
            (Ease::In, 0.0625),
            (Ease::Out, 0.4375),
            (Ease::InOut, 0.156_25),        // 3 p^2 - 2 p^3
            (Ease::CubicIn, 0.015_625),     // p^3
            (Ease::CubicOut, 0.578_125),    // 1 - 0.75^3
            (Ease::CubicInOut, 0.0625),     // 4 p^3
            (Ease::Bezier { x1: 0.0, y1: 0.0, x2: 1.0, y2: 1.0 }, 0.25), // the straight line, solved by bisection
        ];
        for (ease, eased) in rows {
            let signal = lib::keyframes(&[at(1.0, 2.0, ease), at(3.0, 6.0, Ease::Hold)])?;
            let read = signal.sample(&[1.5])?[0];
            assert!((read - (2.0 + 4.0 * eased)).abs() < 1e-12, "{ease:?}: {read}");
        }
        // Penner's back-out with s = 1.70158: 1 + (s + 1)(p - 1)^3 + s (p - 1)^2 = 1 - 2.70158 * 0.421875 + 1.70158 * 0.5625
        let overshoot = lib::keyframes(&[at(1.0, 2.0, Ease::Overshoot), at(3.0, 6.0, Ease::Linear)])?.sample(&[1.5])?[0];
        assert!((overshoot - (2.0 + 4.0 * 0.817_409_687_5)).abs() < 1e-12, "{overshoot}");
        Ok(())
    }

    #[test]
    fn the_first_value_holds_before_the_first_point_and_the_last_from_the_last_on() -> Result<(), Error> {
        // at 3 s: p = 0.5, out = 0.75, and 0.25 * 0.75 + 0.75 * 0.25 = 0.375
        let signal = lib::keyframes(&[at(2.0, 0.75, Ease::Out), at(4.0, 0.25, Ease::Linear)])?;
        let times = [-1.0, -0.0, 0.0, 5e-324, 1.99, 2.0, 3.0, 4.0, 9.0, 1e15, 1e300, f64::MAX];
        assert_eq!(
            signal.sample(&times)?,
            [0.75, 0.75, 0.75, 0.75, 0.75, 0.75, 0.375, 0.25, 0.25, 0.25, 0.25, 0.25]
        );
        // one point is its value everywhere, whatever its ease
        let one = lib::keyframes(&[at(1.0, 0.7, Ease::Out)])?;
        assert_eq!(one.sample(&[0.0, 1.0, 3.0, f64::MAX])?, [0.7; 4]);
        Ok(())
    }

    #[test]
    fn hold_keeps_the_start_value_until_the_next_point_and_gives_its_value_there() -> Result<(), Error> {
        let signal = lib::keyframes(&[at(1.0, 3.0, Ease::Hold), at(3.0, 5.0, Ease::Linear)])?;
        assert_eq!(signal.sample(&[1.0, 2.0, 3.0_f64.next_down(), 3.0, 4.0])?, [3.0, 3.0, 3.0, 5.0, 5.0]);
        Ok(())
    }

    #[test]
    fn a_held_value_and_its_sign_of_zero_read_back_exactly() -> Result<(), Error> {
        for value in [1.0 / 3.0, -7.7, 1e-5, 1e300] {
            for ease in [Ease::Linear, Ease::InOut, Ease::Overshoot] {
                let points = [at(1.0, value, ease), at(2.0, value, ease), at(4.0, value, ease)];
                let read = lib::keyframes(&points)?.sample(&[0.0, 1.0, 1.3, 2.0 + 1.0 / 3.0, 3.9, 4.0, 1e6])?;
                assert_eq!(read, [value; 7], "{value} {ease:?}");
            }
        }
        // -0.0 and 0.0 are equal, so between them the start's zero holds, sign and all; from the last point on, its own
        let down = lib::keyframes(&[at(0.0, -0.0, Ease::Linear), at(1.0, 0.0, Ease::Linear)])?.sample(&[0.0, 0.5, 1.0, 2.0])?;
        let up = lib::keyframes(&[at(0.0, 0.0, Ease::Linear), at(1.0, -0.0, Ease::Linear)])?.sample(&[0.0, 0.5, 1.0, 2.0])?;
        assert_eq!((signs(&down), signs(&up)), (vec![true, true, false, false], vec![false, false, true, true]));
        Ok(())
    }

    #[test]
    fn a_first_point_at_minus_zero_is_a_point_at_zero() -> Result<(), Error> {
        let signal = lib::keyframes(&[at(-0.0, 1.0, Ease::Linear), at(2.0, 3.0, Ease::Linear)])?;
        assert_eq!(signal.sample(&[-0.0, 0.0, 1.0, 2.0])?, [1.0, 1.0, 2.0, 3.0]);
        Ok(())
    }

    #[test]
    fn values_near_the_float_limit_are_read_exactly_at_the_points_and_half_way_between() -> Result<(), Error> {
        // b - a would overflow to infinity here
        for (low, high) in [(-1e308, 1e308), (-f64::MAX, f64::MAX)] {
            let signal = lib::keyframes(&[at(0.0, low, Ease::Linear), at(2.0, high, Ease::Linear)])?;
            assert_eq!(signal.sample(&[0.0, 1.0, 2.0])?, [low, 0.0, high]);
        }
        Ok(())
    }

    #[test]
    fn no_points_are_refused_when_made_as_the_door_refuses_them_even_called_directly() {
        let problems = |made: Result<Signal, Error>| match made {
            Err(Error::Input { problems, .. }) => Ok(problems),
            other => Err(format!("{other:?}")),
        };
        let rule = Problem { path: "points".to_owned(), rule: "needs at least one keyframe".to_owned() };
        assert_eq!(
            (problems(Params.source(vec![])), problems(lib::keyframes(&[]))),
            (Ok(vec![rule.clone()]), Ok(vec![rule]))
        );
    }

    #[test]
    fn points_at_one_time_or_out_of_order_are_refused_naming_the_later_one() {
        for points in [
            [at(1.0, 0.0, Ease::Linear), at(1.0, 2.0, Ease::Linear)],
            [at(2.0, 0.0, Ease::Linear), at(1.0, 2.0, Ease::Linear)],
            [at(0.0, 0.0, Ease::Linear), at(-0.0, 2.0, Ease::Linear)],
        ] {
            let refused = lib::keyframes(&points).map(|_| ());
            assert!(
                matches!(&refused, Err(Error::Input { problems, .. }) if problems.len() == 1 && problems[0].path == "points[1].t"),
                "{points:?}: {refused:?}"
            );
        }
        assert!(matches!(lib::keyframes(&[]), Err(Error::Input { .. })));
    }

    #[test]
    fn through_the_registry_it_reads_its_points_and_records_them() -> Result<(), Error> {
        let thing = Registry::builtin().get("keyframes")?;
        assert_eq!(thing.kind(), Kind::Source);
        let points = Input::from_points(vec![at(0.0, 0.0, Ease::Linear), at(2.0, 1.0, Ease::Linear)])?;
        let call = |values| Call { inputs: [("points".to_owned(), points.clone())].into(), values, ..Call::default() };
        let Output::Signal(ramp) = thing.call(&call(json!({})))? else {
            return Err(Error::Call { message: "not a signal".into() });
        };
        assert_eq!(ramp.sample(&[0.5, 1.0, 3.0])?, [0.25, 0.5, 1.0]);
        let recipe = ramp.recipe().map(|recipe| (recipe.name.clone(), recipe.inputs.clone()));
        assert_eq!(recipe, Some(("keyframes".to_owned(), vec![("points".to_owned(), points.clone())])));
        let refused = thing.call(&call(json!({"rate": 1.0})));
        assert!(
            matches!(&refused, Err(Error::Params { problems, .. }) if problems.len() == 1 && problems[0].path == "rate"),
            "{refused:?}"
        );
        Ok(())
    }

    /// Points from strictly positive gaps, values and eases: times are the running sums of the gaps.
    fn points() -> impl Strategy<Value = Vec<Keyframe>> {
        let ease = prop::sample::select(vec![Ease::Linear, Ease::Hold, Ease::Out, Ease::InOut, Ease::Overshoot]);
        prop::collection::vec((0.001..100.0_f64, -1e3..1e3_f64, ease), 1..8).prop_map(|steps| {
            let mut t = 0.0;
            steps
                .into_iter()
                .map(|(gap, value, ease)| {
                    t += gap;
                    at(t, value, ease)
                })
                .collect()
        })
    }

    proptest! {
        #[test]
        fn a_read_exactly_at_a_point_gives_exactly_its_value(points in points()) {
            let times: Vec<f64> = points.iter().map(|point| point.t).collect();
            let values: Vec<f64> = points.iter().map(|point| point.value).collect();
            prop_assert_eq!(lib::keyframes(&points)?.sample(&times)?, values);
        }

        #[test]
        fn linear_points_are_the_straight_lines_between_them(
            points in points().prop_map(|points| points.into_iter().map(|point| point.ease(Ease::Linear)).collect::<Vec<_>>()),
            time in 0.0..900.0_f64,
        ) {
            // the oracle: the two points around `time` found by a scan, and the line a + (b - a) p
            let read = lib::keyframes(&points)?.sample(&[time])?[0];
            let before = points.iter().rev().find(|point| point.t <= time);
            let after = points.iter().find(|point| point.t > time);
            let line = match (before, after) {
                (Some(a), Some(b)) => a.value + (b.value - a.value) * (time - a.t) / (b.t - a.t),
                (Some(held), None) | (None, Some(held)) => held.value,
                (None, None) => f64::NAN,
            };
            prop_assert!((read - line).abs() <= 1e-9, "{read} vs {line}");
        }
    }
}
