//! [`Keyframe`]: one point of a keyframed signal, and the rules each point follows.

use serde::{Deserialize, Deserializer};

use crate::Error;
use crate::easing::Ease;
use crate::formats::{Number, Problems, check, decode, finite, push, push_with, required, show};

/// One point of a keyframed signal: the `value` at `t` seconds, and the `ease` of the segment that starts here
/// (ignored on the last point).
///
/// ```
/// use bullterrier_control::easing::Ease;
/// use bullterrier_control::inputs::Keyframe;
///
/// let frame = Keyframe::at(2.0, 0.5).ease(Ease::Out);
/// assert_eq!((frame.t, frame.value, frame.ease), (2.0, 0.5, Ease::Out));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    /// When, in seconds: from 0, finite.
    pub t: f64,
    /// The value there: finite.
    pub value: f64,
    /// The curve of the segment that starts here.
    pub ease: Ease,
}

impl Keyframe {
    /// The keyframe of `value` at `t`, eased linearly; checked where it is used ([`Input::from_points`](super::Input::from_points)).
    ///
    /// ```
    /// use bullterrier_control::easing::Ease;
    /// use bullterrier_control::inputs::Keyframe;
    ///
    /// assert_eq!(Keyframe::at(1.0, 3.0).ease, Ease::Linear);
    /// ```
    #[must_use]
    pub const fn at(t: f64, value: f64) -> Self {
        Self {
            t,
            value,
            ease: Ease::Linear,
        }
    }

    /// The same keyframe with the segment that starts here eased by `ease`.
    ///
    /// ```
    /// use bullterrier_control::easing::Ease;
    /// use bullterrier_control::inputs::Keyframe;
    ///
    /// assert_eq!(Keyframe::at(1.0, 3.0).ease(Ease::Hold).ease, Ease::Hold);
    /// ```
    #[must_use]
    pub const fn ease(self, ease: Ease) -> Self {
        Self { ease, ..self }
    }

    /// Read one keyframe `{t, value, ease}` from any serde format (JSON, a Python object) and check it: `t` and
    /// `value` are required numbers, finite, `t` >= 0; `ease` is a name or `cubic-bezier(..)` as [`Ease`] parses it,
    /// linear when absent or `null`.
    ///
    /// ```
    /// use bullterrier_control::easing::Ease;
    /// use bullterrier_control::inputs::Keyframe;
    /// use serde_json::json;
    ///
    /// let frame = Keyframe::from_deserializer(&json!({"t": 2.5, "value": -0.75, "ease": "in"}))?;
    /// assert_eq!(frame, Keyframe::at(2.5, -0.75).ease(Ease::In));
    ///
    /// let refused = Keyframe::from_deserializer(&json!({"t": -1, "value": 0})).map_err(|error| error.to_string());
    /// assert_eq!(refused, Err("t: must be >= 0 (got -1.0)".to_owned()));
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Input`]: the one problem that stopped the reading (a wrong type for a field, an unknown field,
    /// something that is not an object, a whole number beyond 2^53), or, when the data was read, every rule it breaks.
    pub fn from_deserializer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, Error> {
        let mut problems = Problems::new();
        match decode::<RawKeyframe, _>(deserializer) {
            Ok(raw) => Frame::from(raw).read("", None, &mut problems),
            Err(error) => {
                problems.absorb(&error);
                None
            }
        }
        .ok_or_else(|| super::refused(problems))
    }
}

// The fields as data spells them, before any rule is checked; an absent field and a `null` one are both `None`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawKeyframe {
    t: Option<Number>,
    value: Option<Number>,
    ease: Option<String>,
}

/// A keyframe as given, typed or read from data, before its rules are checked: `None` is a field the data lacked, and
/// `ease` is the curve or why its text names none.
pub(super) struct Frame {
    t: Option<f64>,
    value: Option<f64>,
    ease: Result<Ease, String>,
}

impl From<RawKeyframe> for Frame {
    fn from(raw: RawKeyframe) -> Self {
        Self {
            t: raw.t.map(|number| number.0),
            value: raw.value.map(|number| number.0),
            ease: raw.ease.map_or(Ok(Ease::Linear), |text| {
                text.parse().map_err(|error: Error| error.to_string())
            }),
        }
    }
}

impl From<Keyframe> for Frame {
    fn from(frame: Keyframe) -> Self {
        Self {
            t: Some(frame.t),
            value: Some(frame.value),
            ease: Ok(frame.ease),
        }
    }
}

impl Frame {
    /// Its `t` if it is a finite number: the one a later keyframe is compared with.
    pub(super) fn known_t(&self) -> Option<f64> {
        self.t.filter(|t| t.is_finite())
    }

    /// The one place a keyframe's rules are written. Every rule broken is recorded in `problems` under `prefix`
    /// (empty, or `points[2]`), in field order: `t` required, finite and >= 0, and after `previous` (the known `t`
    /// of the keyframe before, if any); `value` required and finite; `ease` a curve. The keyframe, if none is broken.
    pub(super) fn read(self, prefix: &str, previous: Option<f64>, problems: &mut Problems) -> Option<Keyframe> {
        let before = problems.len();
        match self.t {
            Some(t) => check(problems, prefix, "t", t, "must be >= 0", |t| t >= 0.0),
            None => required(problems, prefix, "t"),
        }
        if let (Some(t), Some(previous)) = (self.known_t(), previous)
            && t <= previous
        {
            push_with(problems, prefix, "t", || {
                format!("{} is not after the previous t = {}", show(t), show(previous))
            });
        }
        match self.value {
            Some(value) => {
                finite(problems, prefix, "value", value);
            }
            None => required(problems, prefix, "value"),
        }
        if let Err(rule) = &self.ease {
            push(problems, prefix, "ease", rule.clone());
        }
        match (self.t, self.value, self.ease) {
            (Some(t), Some(value), Ok(ease)) if problems.len() == before => Some(Keyframe { t, value, ease }),
            _ => None,
        }
    }
}
