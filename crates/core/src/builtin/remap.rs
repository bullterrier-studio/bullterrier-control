//! A signal that puts its input, taken as 0 to 1, onto the range from `low` to `high`, bent by an ease.
//!
//! An input outside 0 to 1 is clipped to it first: below 0 gives `low`, above 1 gives `high`. A `low` above `high`
//! inverts. The output is exactly inside the range: an ease that overshoots (`overshoot`, or a Bezier with a y outside
//! 0 to 1) is clipped to `low` and `high`.

use crate::easing::Ease;
use crate::formats::Problem;
use crate::scaling::between;
use crate::{Error, Shaper, Signal};

/// The range the input is put onto, and the ease that bends the way there.
#[derive(Clone, Debug, PartialEq, crate::Params)]
#[param(rule = rule)]
pub struct Params {
    /// The output when the input is 0; set above `high` to flip the signal.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub low: f64,
    /// The output when the input is 1.
    #[param(default = 1.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub high: f64,
    /// How the input is bent before it is put onto the range: one of the eases keyframes take (linear, hold, in, out,
    /// in-out, cubic-in, cubic-out, cubic-in-out, overshoot) or cubic-bezier(x1, y1, x2, y2). 'linear' keeps the
    /// shape.
    #[param(default = "linear")]
    pub ease: String,
}

impl Signal {
    /// This signal, taken as 0 to 1, put onto `params.low`..`params.high` through `params.ease`: the shaper
    /// [`remap`](super), as [`pipe`](Signal::pipe) of `params` makes it (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::builtin::remap;
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// let level = Signal::from_values(&[0.0, 0.5, 2.0], 1.0, FromValues::default(), Interpolation::Hold)?;
    /// let opacity = level.remap(remap::Params { low: 40.0, high: 120.0, ease: "in".to_owned() })?;
    /// assert_eq!(opacity.sample(&[0.0, 1.0, 2.0])?, [40.0, 60.0, 120.0]); // 40 + 80 * 0.5^2; 2 is read as 1
    /// assert!(level.remap(remap::Params { ease: "wobble".to_owned(), ..Default::default() }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe`]: [`Error::Params`] under the name `remap` listing every rule `params` breaks (a field out
    /// of its range or not finite, an `ease` that names no curve).
    pub fn remap(&self, params: Params) -> Result<Signal, Error> {
        self.pipe(params)
    }
}

impl Shaper for Params {
    const NAME: &'static str = "remap";
    type Inputs = ();

    /// `input` through the ease (which clips its progress to 0..1), then onto `low`..`high`; the ease is parsed here,
    /// once, not on every read.
    fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> {
        let ease: Ease = self.ease.parse()?;
        let (low, high) = (self.low, self.high);
        input.mapped(move |value| between(low, high, ease.apply(value)))
    }
}

/// `ease` must name a curve ([`Ease`]: one of its names, or `cubic-bezier(x1, y1, x2, y2)`); the refusal is the one
/// `Ease` gives, which lists the valid forms.
fn rule(params: &Params) -> Vec<Problem> {
    match params.ease.parse::<Ease>() {
        Ok(_) => Vec::new(),
        Err(error) => vec![Problem {
            path: "ease".to_owned(),
            rule: error.to_string(),
        }],
    }
}
