//! The data a source, shaper or operation takes besides its parameters, and the one place that says what each kind
//! of it is and how it is checked: keyframes (`points`), `events`, a `grid`, `values` and a `series`.
//!
//! ```
//! use bullterrier_control::easing::Ease;
//! use bullterrier_control::inputs::{Input, InputKind, Keyframe};
//! use serde_json::json;
//!
//! // data (JSON, or a Python object through the bindings), by the input's name
//! let kind: InputKind = "points".parse()?;
//! let points = kind.check(&json!([{"t": 0, "value": 0, "ease": "in-out"}, {"t": 5, "value": 0.9}]))?;
//! assert_eq!(points, Input::Points(vec![Keyframe::at(0.0, 0.0).ease(Ease::InOut), Keyframe::at(5.0, 0.9)]));
//!
//! // typed values, held to the same rules
//! let refused = Input::from_points(vec![Keyframe::at(1.0, 0.0), Keyframe::at(1.0, 2.0)]);
//! assert_eq!(refused.map_err(|error| error.to_string()), Err("points[1].t: 1.0 is not after the previous t = 1.0".into()));
//! # Ok::<(), bullterrier_control::Error>(())
//! ```
//!
//! A parameter is a number or a choice; an input is a structure. Adding a kind is one [`InputKind`] and one [`Input`]
//! variant here.
//!
//! Promises:
//! - [`InputKind::check`] returns the input in its checked form, or an [`Error::Input`] listing each broken rule as
//!   `path: rule`, the path from the input's name (`points[1].ease`): the first 100, then `; ... and N more`.
//! - Numbers are read as the formats read them: a JSON number (a whole one is a decimal too), never `true`, text or
//!   `null`; a whole number beyond 2^53 is refused, and NaN and the infinities are refused as not finite. A list's
//!   item of a wrong type, or a keyframe or event with an unknown field, is one problem at that item; the other items
//!   are still read and checked.
//! - `points`: a non-empty list of keyframes `{t, value, ease}` ([`Keyframe::from_deserializer`]'s rules); `t`
//!   strictly increases, each `t` compared with the one before it where both are finite numbers. Checked, it is
//!   [`Input::Points`].
//! - `events`: a list of events as in a bundle, possibly empty, in any order, each one's problems under `events[i]`.
//!   Checked, it is [`Input::Events`], sorted by `t`; events at the same time keep the order they were given in.
//! - `grid` and `series`: as in a bundle, each problem under `grid` or `series` (`grid.beats[1]`).
//! - `values`: a non-empty list of finite numbers. Checked, it is [`Input::Values`].
//! - [`matching`]: the events a `label` parameter picks, shared by everything that has one.

mod items;
mod keyframe;
#[cfg(test)]
mod tests;

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::Deserializer;

use crate::Error;
use crate::easing::Ease;
use crate::formats::{
    Event, FormatError, Grid, Number, Object, Problem, Problems, RawEvent, RawGrid, RawSeries, Series, decode, finite,
    push,
};
use items::{absorb_under, items};
use keyframe::{Frame, RawKeyframe};

pub use keyframe::Keyframe;

/// A kind of input, by the name a function declares it under (`points`).
///
/// ```
/// use bullterrier_control::inputs::InputKind;
///
/// let kind: InputKind = "events".parse()?;
/// assert_eq!((kind, kind.name()), (InputKind::Events, "events"));
/// assert!("keyframes".parse::<InputKind>().is_err());
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InputKind {
    /// Keyframes: [`Input::Points`].
    Points,
    /// Events: [`Input::Events`].
    Events,
    /// A beat grid: [`Input::Grid`].
    Grid,
    /// Numbers to cycle through: [`Input::Values`].
    Values,
    /// A series: [`Input::Series`].
    Series,
}

/// The kinds by name: the one place a name and its kind are paired.
const KINDS: [(&str, InputKind); 5] = [
    ("points", InputKind::Points),
    ("events", InputKind::Events),
    ("grid", InputKind::Grid),
    ("values", InputKind::Values),
    ("series", InputKind::Series),
];

impl InputKind {
    /// The names of the kinds, in the order of the variants.
    ///
    /// ```
    /// use bullterrier_control::inputs::InputKind;
    ///
    /// assert!(InputKind::NAMES.iter().all(|name| name.parse::<InputKind>().is_ok()));
    /// ```
    pub const NAMES: [&'static str; 5] = {
        let mut names = [""; 5];
        let mut index = 0;
        while index < KINDS.len() {
            names[index] = KINDS[index].0;
            index += 1;
        }
        names
    };

    /// The name a function declares this input under.
    ///
    /// ```
    /// use bullterrier_control::inputs::InputKind;
    ///
    /// assert_eq!(InputKind::Grid.name(), "grid");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        // the variants are in the order of `KINDS`
        Self::NAMES[self as usize]
    }

    /// What `help()` says about the input: what it is and how it is written.
    ///
    /// ```
    /// use bullterrier_control::inputs::InputKind;
    ///
    /// assert!(InputKind::Points.doc().contains("cubic-bezier(x1, y1, x2, y2)"));
    /// ```
    #[must_use]
    pub fn doc(self) -> String {
        match self {
            Self::Points => format!(
                "Keyframes: a list of {{t, value, ease}}, t in seconds from 0 and strictly increasing, ease the curve of \
                 the segment that starts there (ignored on the last point; default linear): one of {}, or \
                 cubic-bezier(x1, y1, x2, y2).",
                Ease::NAMES.join(", ")
            ),
            Self::Events => "Events: a list of {t, strength, duration, label, pitch}, as in a bundle, in any order \
                             (checked into time order)."
                .to_owned(),
            Self::Grid => "A beat grid: {beats, meter, first_downbeat} as in a bundle (beats in seconds, strictly \
                           increasing, at least two). Converts seconds to beats, so a source can follow tempo changes."
                .to_owned(),
            Self::Values => {
                "Values: a non-empty list of numbers, in the order a source cycles through them.".to_owned()
            }
            Self::Series => "A series: {start, rate, values, unit, scaling} as in a bundle (values at a fixed rate in \
                             samples per second from a start time in seconds)."
                .to_owned(),
        }
    }

    /// `data` checked as this kind of input: a list or an object as JSON or a Python object holds it.
    ///
    /// `data` is a value held in memory (a `&serde_json::Value`, or the reader the Python bindings give): the items of
    /// a list are read one by one, a broken one recorded and the next read after it, which a reader that streams JSON
    /// text cannot do.
    ///
    /// ```
    /// use bullterrier_control::Error;
    /// use bullterrier_control::inputs::{Input, InputKind};
    /// use serde_json::json;
    ///
    /// assert_eq!(InputKind::Values.check(&json!([1, 0.5]))?, Input::Values(vec![1.0, 0.5]));
    ///
    /// let refused = InputKind::Values.check(&json!([true, 1.0, "x"]));
    /// let paths = match &refused {
    ///     Err(Error::Input { problems, .. }) => problems.iter().map(|problem| problem.path.as_str()).collect(),
    ///     _ => vec![],
    /// };
    /// assert_eq!(paths, ["values[0]", "values[2]"]);
    /// # Ok::<(), Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Input`] listing every rule `data` breaks (the module's promises say which).
    pub fn check<'de, D: Deserializer<'de>>(self, data: D) -> Result<Input, Error> {
        let name = self.name();
        let mut problems = Problems::new();
        let found = match self {
            Self::Points => {
                let mut rules = Points::new(&problems);
                items::<Object<RawKeyframe>, _>(data, |frame| {
                    rules.give(frame.map(|Object(raw)| Frame::from(raw)), &mut problems);
                })
                .map(|()| rules.end(&mut problems).map(Input::Points))
            }
            Self::Events => {
                let mut rules = Events::new(&problems);
                items::<Object<RawEvent>, _>(data, |event| rules.give(event, &mut problems))
                    .map(|()| rules.end(&problems).map(Input::from_events))
            }
            Self::Grid => decode::<RawGrid, _>(data).map(|raw| raw.read(name, &mut problems).map(Input::Grid)),
            Self::Values => {
                let mut rules = Values::new(&problems);
                items::<Number, _>(data, |value| {
                    rules.give(value.map(|Number(value)| value), &mut problems);
                })
                .map(|()| rules.end(&mut problems).map(Input::Values))
            }
            Self::Series => decode::<RawSeries, _>(data).map(|raw| raw.read(name, &mut problems).map(Input::Series)),
        };
        match found {
            Ok(Some(input)) => Ok(input),
            Ok(None) => Err(refused(problems)),
            Err(error) => {
                absorb_under(&mut problems, name, error);
                Err(refused(problems))
            }
        }
    }
}

impl FromStr for InputKind {
    type Err = Error;

    /// The kind called `name`, one of [`InputKind::NAMES`].
    ///
    /// ```
    /// use bullterrier_control::inputs::InputKind;
    ///
    /// assert_eq!("series".parse::<InputKind>()?, InputKind::Series);
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Input`] naming the kinds there are, if there is none of that name.
    fn from_str(name: &str) -> Result<Self, Error> {
        if let Some(&(_, kind)) = KINDS.iter().find(|(known, _)| *known == name) {
            return Ok(kind);
        }
        let (last, others) = Self::NAMES.split_last().unwrap_or((&"", &[]));
        let mut problems = Problems::new();
        problems.push(Problem {
            path: String::new(),
            rule: format!("unknown input '{name}': use {} or {last}", others.join(", ")),
        });
        Err(refused(problems))
    }
}

impl fmt::Display for InputKind {
    /// Its [`name`](InputKind::name).
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// An input in its checked form. Made by [`InputKind::check`] or the `from_` functions, it keeps the promises of its
/// kind (a variant built by hand is not checked).
///
/// ```
/// use bullterrier_control::formats::{Event, EventOptions};
/// use bullterrier_control::inputs::Input;
///
/// let late = Event::new(2.0, EventOptions::default())?;
/// let early = Event::new(0.5, EventOptions::default())?;
/// assert_eq!(Input::from_events(vec![late.clone(), early.clone()]), Input::Events(vec![early, late]));
/// # Ok::<(), bullterrier_control::formats::FormatError>(())
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Input {
    /// Keyframes: at least one, `t` from 0 and strictly increasing, every number finite.
    Points(Vec<Keyframe>),
    /// Events in time order; events at the same time in the order they were given.
    Events(Vec<Event>),
    /// A beat grid.
    Grid(Grid),
    /// At least one number, every one finite.
    Values(Vec<f64>),
    /// A series.
    Series(Series),
}

impl Input {
    /// `points` checked as keyframes, by the rules [`InputKind::check`] holds `points` data to.
    ///
    /// ```
    /// use bullterrier_control::inputs::{Input, Keyframe};
    ///
    /// assert!(Input::from_points(vec![Keyframe::at(0.0, 1.0), Keyframe::at(2.0, 0.0)]).is_ok());
    /// assert!(Input::from_points(vec![]).is_err());
    /// ```
    ///
    /// # Errors
    /// [`Error::Input`] listing every rule broken: no keyframe, a `t` below 0, a number not finite, a `t` not after
    /// the one before it.
    pub fn from_points(points: Vec<Keyframe>) -> Result<Self, Error> {
        let mut problems = Problems::new();
        let mut rules = Points::new(&problems);
        for frame in points {
            rules.give(Ok(Frame::from(frame)), &mut problems);
        }
        rules
            .end(&mut problems)
            .map(Self::Points)
            .ok_or_else(|| refused(problems))
    }

    /// `events` in time order; events at the same time keep the order they are given in.
    ///
    /// ```
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::inputs::Input;
    ///
    /// assert_eq!(Input::from_events(vec![]), Input::Events(vec![]));
    /// ```
    #[must_use]
    pub fn from_events(events: Vec<Event>) -> Self {
        Self::Events(in_time_order(events))
    }

    /// `values` checked: at least one, each finite.
    ///
    /// ```
    /// use bullterrier_control::inputs::Input;
    ///
    /// assert_eq!(Input::from_values(vec![0.25, -1.0])?, Input::Values(vec![0.25, -1.0]));
    /// assert!(Input::from_values(vec![f64::NAN]).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Input`] listing every rule broken: no value, or a value that is not finite (`values[2]`).
    pub fn from_values(values: Vec<f64>) -> Result<Self, Error> {
        let mut problems = Problems::new();
        let mut rules = Values::new(&problems);
        for value in values {
            rules.give(Ok(value), &mut problems);
        }
        rules
            .end(&mut problems)
            .map(Self::Values)
            .ok_or_else(|| refused(problems))
    }
}

/// The events whose label is `label`, in their order; every event, unlabelled ones too, when `label` is empty.
///
/// ```
/// use bullterrier_control::formats::{Event, EventOptions};
/// use bullterrier_control::inputs::matching;
///
/// let kick = Event::new(0.5, EventOptions { label: Some("kick".into()), ..Default::default() })?;
/// let bare = Event::new(0.7, EventOptions::default())?;
/// let events = [kick.clone(), bare];
/// assert_eq!(matching(&events, "kick"), [kick]);
/// assert_eq!(matching(&events, "").len(), 2);
/// # Ok::<(), bullterrier_control::formats::FormatError>(())
/// ```
#[must_use]
pub fn matching(events: &[Event], label: &str) -> Vec<Event> {
    events
        .iter()
        .filter(|event| label.is_empty() || event.label() == Some(label))
        .cloned()
        .collect()
}

/// The rules of `points`, applied to each keyframe as it is given (read, or the problem that stopped its reading), so
/// that a broken one costs nothing once the problems are full: at least one; each one's own rules under `points[i]`,
/// its `t` after the known `t` of the one before.
struct Points {
    kept: Vec<Keyframe>,
    given: usize,
    /// The known `t` of the keyframe given last, if any.
    previous: Option<f64>,
    /// How many problems were recorded before the first keyframe.
    before: usize,
}

impl Points {
    fn new(problems: &Problems) -> Self {
        Self {
            kept: Vec::new(),
            given: 0,
            previous: None,
            before: problems.len(),
        }
    }

    fn give(&mut self, frame: Result<Frame, FormatError>, problems: &mut Problems) {
        let prefix = format!("points[{}]", self.given);
        self.given += 1;
        self.previous = match frame {
            Ok(frame) => {
                let t = frame.known_t();
                self.kept.extend(frame.read(&prefix, self.previous, problems));
                t
            }
            Err(error) => {
                absorb_under(problems, &prefix, error);
                None
            }
        };
    }

    /// The keyframes, if no rule is broken.
    fn end(self, problems: &mut Problems) -> Option<Vec<Keyframe>> {
        if self.given == 0 {
            push(problems, "", "points", "needs at least one keyframe");
            return None;
        }
        (problems.len() == self.before).then_some(self.kept)
    }
}

/// The rules of `events`, applied to each event as it is given (as data gives it, or the problem that stopped its
/// reading): each one's rules under `events[i]`.
struct Events {
    kept: Vec<Event>,
    given: usize,
    /// How many problems were recorded before the first event.
    before: usize,
}

impl Events {
    fn new(problems: &Problems) -> Self {
        Self {
            kept: Vec::new(),
            given: 0,
            before: problems.len(),
        }
    }

    fn give(&mut self, event: Result<Object<RawEvent>, FormatError>, problems: &mut Problems) {
        let prefix = format!("events[{}]", self.given);
        self.given += 1;
        match event {
            Ok(Object(raw)) => self.kept.extend(raw.read(&prefix, problems)),
            Err(error) => absorb_under(problems, &prefix, error),
        }
    }

    /// The events, in the order given, if no rule is broken.
    fn end(self, problems: &Problems) -> Option<Vec<Event>> {
        (problems.len() == self.before).then_some(self.kept)
    }
}

/// `events` sorted by time, those at the same time in the order given.
fn in_time_order(mut events: Vec<Event>) -> Vec<Event> {
    // `sort_by` is stable. An event's time is finite, so always comparable; `partial_cmp`, unlike `total_cmp`, says
    // -0 and 0 are the same time.
    events.sort_by(|early, late| early.t().partial_cmp(&late.t()).unwrap_or(Ordering::Equal));
    events
}

/// The rules of `values`, applied to each number as it is given (read, or the problem that stopped its reading): at
/// least one, each finite (`values[i]`).
struct Values {
    kept: Vec<f64>,
    given: usize,
    /// How many problems were recorded before the first value.
    before: usize,
}

impl Values {
    fn new(problems: &Problems) -> Self {
        Self {
            kept: Vec::new(),
            given: 0,
            before: problems.len(),
        }
    }

    fn give(&mut self, value: Result<f64, FormatError>, problems: &mut Problems) {
        let index = self.given;
        self.given += 1;
        match value {
            Ok(value) => {
                if finite(problems, "", format_args!("values[{index}]"), value) {
                    self.kept.push(value);
                }
            }
            Err(error) => absorb_under(problems, &format!("values[{index}]"), error),
        }
    }

    /// The values, if no rule is broken.
    fn end(self, problems: &mut Problems) -> Option<Vec<f64>> {
        if self.given == 0 {
            push(problems, "", "values", "needs at least one value");
            return None;
        }
        (problems.len() == self.before).then_some(self.kept)
    }
}

/// The error for the problems recorded, at least one.
fn refused(problems: Problems) -> Error {
    let (problems, omitted) = problems.into_parts();
    Error::Input { problems, omitted }
}
