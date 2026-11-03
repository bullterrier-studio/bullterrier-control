//! A signal that, on each event, rises to the event's strength over `attack`, falls toward `sustain` over `decay`
//! and, once an event with a duration ends, falls to 0 over `release`.
//!
//! Read u seconds after an event of strength s: 0 before it; s * u / attack while it rises; then from s toward
//! s * sustain, exponentially (s * (sustain + (1 - sustain) * e^(-(u - attack) / decay))) or linearly (reaching it
//! `decay` seconds after the attack). An event without a duration never ends and has sustain 0. From the end of an
//! event with a duration, the level L it had there falls to 0: L * e^(-(u - duration) / release), or linearly over
//! `release`. If the duration is shorter than the attack, the rise stops there. Only events with `label` count
//! (all if it is empty). Overlapping events combine by `mix`.
//!
//! The value is never below 0; under `max` never above the largest strength, under `sum` it can exceed 1 (clip it).
//! An exponential tail is dropped once it is below a billionth of its event's strength, so each event's contribution
//! is within 1e-9 times its strength of the uncut formula. Memory grows with the times read plus the events, never
//! with their product.

use std::convert::Infallible;

use crate::formats::Event;
use crate::inputs::matching;
use crate::{Choice, Error, Signal, Source};

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

impl Source for Params {
    const NAME: &'static str = "envelope";
    type Inputs = Vec<Event>;

    /// The envelopes of the matching `events`, combined in the order given (the door's time order) by `mix`.
    ///
    /// ```
    /// use bullterrier_control::builtin::envelope;
    /// use bullterrier_control::formats::{Event, EventOptions};
    /// use bullterrier_control::lib;
    ///
    /// // a kick at 1 s rising over 0.5 s, then falling linearly to 0 over 1 s
    /// let kick = Event::new(1.0, EventOptions::default())?;
    /// let shape = envelope::Params { attack: 0.5, decay: 1.0, curve: envelope::Curve::Linear, ..Default::default() };
    /// let flare = lib::envelope(&[kick], shape)?;
    /// assert_eq!(flare.sample(&[0.5, 1.25, 1.5, 2.0, 2.5])?, [0.0, 0.5, 1.0, 0.5, 0.0]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// None: every set of events makes an envelope, and every read of it is a value.
    fn source(&self, events: Vec<Event>) -> Result<Signal, Error> {
        let stages = Stages {
            attack: self.attack,
            // the rise is a linear fall over the attack run backwards: risen = 1 - left
            rise: Fade { span: self.attack, curve: Curve::Linear },
            decay: Fade { span: self.decay, curve: self.curve },
            release: Fade { span: self.release, curve: self.curve },
        };
        // an event of strength 0 adds nothing under either mix
        let counted: Vec<Counted> = matching(&events, &self.label)
            .iter()
            .filter(|event| event.strength() > 0.0)
            .map(|event| stages.counted(event, self.sustain))
            .collect();
        let mix = self.mix;
        Ok(Signal::from_fn(move |times, out| {
            // each event touches only the reads from its start to its reach end: memory is O(times + events)
            let mut ordered: Vec<(f64, usize)> = times.iter().copied().zip(0..).collect();
            ordered.sort_by(|a, b| a.0.total_cmp(&b.0));
            for event in &counted {
                let first = ordered.partition_point(|&(time, _)| time < event.start);
                let last = ordered.partition_point(|&(time, _)| time <= event.reach_end);
                // a window that is empty (or, for parameters the door refuses, backwards) touches nothing
                for &(time, index) in ordered.get(first..last).unwrap_or_default() {
                    if let Some(value) = out.get_mut(index) {
                        let part = stages.value(event, time - event.start);
                        *value = match mix {
                            Mix::Max => value.max(part),
                            Mix::Sum => *value + part,
                        };
                    }
                }
            }
            Ok::<(), Infallible>(())
        }))
    }
}

/// An exponential fall never quite ends: a contribution is dropped once it is below a billionth of its strength, which
/// is ln(1e9) ~ 20.723 time constants into the fall (e^-x < 1e-9 for x > ln(1e9)).
fn time_constants_to_a_billionth() -> f64 {
    libm::log(1e9)
}

/// A fall over `span` seconds along `curve`.
#[derive(Clone, Copy)]
struct Fade {
    span: f64,
    curve: Curve,
}

impl Fade {
    /// The share of the fall left `elapsed` seconds into it: 1 at its start, toward 0. A span of 0 is decided here,
    /// once, so nothing divides by it.
    fn left(self, elapsed: f64) -> f64 {
        if self.span == 0.0 {
            return 0.0;
        }
        match self.curve {
            // (span - elapsed) / span, not 1 - elapsed / span: it cannot overflow however short the span
            Curve::Linear => (self.span - elapsed).max(0.0) / self.span,
            Curve::Exponential => libm::exp(-elapsed / self.span),
        }
    }

    /// How many seconds into the fall it can still be seen.
    fn reach(self) -> f64 {
        if self.span == 0.0 {
            return 0.0;
        }
        match self.curve {
            Curve::Linear => self.span,
            Curve::Exponential => self.span * time_constants_to_a_billionth(),
        }
    }
}

/// The shape every counted event follows, in seconds from its start.
#[derive(Clone, Copy)]
struct Stages {
    attack: f64,
    rise: Fade,
    decay: Fade,
    release: Fade,
}

/// One counted event, as its reads need it. A moment is an event that never ends and has no sustain: the same
/// formula serves both.
struct Counted {
    start: f64,
    strength: f64,
    /// Seconds from the start to the end: infinite for a moment.
    end: f64,
    sustain: f64,
    /// The last time it can be seen; beyond the largest float (infinite) is right for an end near it.
    reach_end: f64,
    /// Its value at its end, where the release starts.
    level_at_end: f64,
}

impl Stages {
    /// `event` as its reads need it, `sustain` the level an event with a duration holds.
    fn counted(self, event: &Event, sustain: f64) -> Counted {
        let (end, sustain, reach) = match event.duration() {
            // a duration is seen until its release is over; a moment until its decay is
            Some(duration) => (duration, sustain, duration + self.release.reach()),
            None => (f64::INFINITY, 0.0, self.attack + self.decay.reach()),
        };
        Counted {
            start: event.t(),
            strength: event.strength(),
            end,
            sustain,
            reach_end: event.t() + reach,
            level_at_end: self.held(end, event.strength(), sustain),
        }
    }

    /// The value `since` seconds after an event starts, as long as it has not ended.
    fn held(self, since: f64, strength: f64, sustain: f64) -> f64 {
        // numpy's `where` evaluates both branches, so a numpy body clamps `since - attack` at 0; here only the one
        // taken is, and there `since >= attack`, so the difference is never below 0
        if since < self.attack {
            strength * (1.0 - self.rise.left(since))
        } else {
            strength * (sustain + (1.0 - sustain) * self.decay.left(since - self.attack))
        }
    }

    /// The value of `event` `since` seconds after it starts (0 up).
    fn value(self, event: &Counted, since: f64) -> f64 {
        if since < event.end {
            self.held(since, event.strength, event.sustain)
        } else {
            event.level_at_end * self.release.left(since - event.end)
        }
    }
}
