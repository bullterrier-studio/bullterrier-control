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
