//! `envelope`: the parameters of the source `envelope`, declared once, here. What it does is still done in Python.

use crate::Choice;

/// The choices of `curve`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Curve {
    /// The choice `exponential`.
    Exponential,
    /// The choice `linear`.
    Linear,
}

/// The choices of `mix`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Mix {
    /// The choice `max`.
    Max,
    /// The choice `sum`.
    Sum,
}

/// Which events count, and the shape each one gives the value: attack, decay, sustain, release.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Only events with this label count; empty takes every event, unlabelled ones too.
    #[param(default = "")]
    pub label: String,
    /// Seconds an event takes to rise from 0 to its strength; 0 jumps at once. Raising it softens the onset.
    #[param(default = 0.0, unit = Seconds, min = 0, max = 60)]
    pub attack: f64,
    /// Seconds the value takes to fall from the strength toward the sustain level after the attack: the time constant
    /// of an exponential curve, the whole fall of a linear one; 0 falls at once. Raising it lengthens the tail.
    #[param(default = 0.25, unit = Seconds, min = 0, max = 3600)]
    pub decay: f64,
    /// The level, as a share of the strength, an event with a duration holds until it ends, 0 to 1; an event without
    /// a duration falls to 0.
    #[param(default = 0.0, unit = Fraction, min = 0, max = 1)]
    pub sustain: f64,
    /// Seconds the value takes to fall to 0 after an event with a duration ends, from wherever it was (time constant
    /// or whole fall, as for decay); 0 drops at once.
    #[param(default = 0.0, unit = Seconds, min = 0, max = 3600)]
    pub release: f64,
    /// How decay and release fall: 'exponential' (fast, then a long tail) or 'linear' (straight to its end).
    #[param(default = Curve::Exponential)]
    pub curve: Curve,
    /// Where events overlap: 'max' takes the larger value, 'sum' adds them (and can exceed 1).
    #[param(default = Mix::Max)]
    pub mix: Mix,
}
