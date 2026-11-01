//! The node reading a [`Series`](crate::formats::Series) at any time.

use super::node::{Next, Node};
use super::{Interpolation, Origin};
use crate::Error;

/// A share of a sample within which a position is the sample it rounds to: `(time - start) * rate` of a time that is
/// a sample's time in decimals can land a float rounding below it (`0.29 * 100` is 28.999999999999996), and a held
/// value is the sample's from its time on. A billionth of a sample is far above that rounding and far below a sample.
pub(crate) const SAMPLE_ROUNDING: f64 = 1e-9;

/// Sample *i* at `start + i / rate` seconds; the first and last values hold beyond the ends.
pub(crate) struct SeriesNode {
    pub(crate) start: f64,
    pub(crate) rate: f64,
    /// Never empty: a `Series` holds at least one value.
    pub(crate) values: Vec<f64>,
    pub(crate) interpolation: Interpolation,
}

impl Node for SeriesNode {
    fn start<'g>(&'g self, times: &[f64], out: &mut [f64]) -> Result<Next<'g>, Box<Error>> {
        for (value, time) in out.iter_mut().zip(times) {
            // finite times and a finite start: never NaN, at most an infinity past either end
            let position = (time - self.start) * self.rate;
            *value = match self.interpolation {
                Interpolation::Linear => self.linear(position),
                Interpolation::Hold => self.held(position),
            };
        }
        Ok(Next::Done)
    }

    fn depth(&self) -> usize {
        1
    }

    fn origin(&self) -> Origin<'_> {
        Origin::Values
    }

    fn label(&self) -> &'static str {
        "series"
    }
}

impl SeriesNode {
    /// The value on the line between the samples either side of `position`, as numpy's `interp` gives it (on a sample,
    /// its value; else the slope times the way from the earlier sample, plus its value). numpy may fuse that
    /// multiply-add where the hardware has it, so the old package's result can differ by one rounding of the operand
    /// scale; this one rounds twice, the same on every target.
    fn linear(&self, position: f64) -> f64 {
        match self.values.as_slice() {
            [first, ..] if position <= 0.0 => *first,
            [.., last] if position >= self.last() => *last,
            values => {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "0 < position < the last index: a whole index of the values"
                )]
                let before = position.floor() as usize;
                let (from, to) = (values[before], values[before + 1]);
                let way = position - position.floor();
                // on a sample its value, not `slope * 0 + value`: an overflowing slope makes NaN, a -0.0 sample 0.0
                if way == 0.0 { from } else { (to - from) * way + from }
            }
        }
    }

    /// The value of the latest sample at or before `position`, to within [`SAMPLE_ROUNDING`].
    fn held(&self, position: f64) -> f64 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0..=the last index: a whole index of the values"
        )]
        let index = (position + SAMPLE_ROUNDING).floor().clamp(0.0, self.last()) as usize;
        self.values[index]
    }

    /// The index of the last sample, as a position.
    fn last(&self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "a count of values is far below 2^53: exact")]
        let last = (self.values.len() - 1) as f64;
        last
    }
}
