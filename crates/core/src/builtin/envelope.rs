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

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "the values worked out by hand are exact in binary")]
mod tests {
    use std::f64::consts::E;

    use proptest::prelude::*;
    use serde_json::json;

    use super::*;
    use crate::formats::EventOptions;
    use crate::inputs::Input;
    use crate::registry::{Call, Output};
    use crate::{Registry, builtin::envelope, lib};

    /// ln(1e9) as Python's `math.log(1e9)` gives it (0x1.4b927f32bffb8p+4): the time constants an exponential fall
    /// is seen for.
    const LN_1E9: f64 = 20.723_265_836_946_41;

    fn event(t: f64, strength: f64, duration: Option<f64>, label: Option<&str>) -> Result<Event, Error> {
        let options = EventOptions {
            strength,
            duration,
            label: label.map(str::to_owned),
            ..EventOptions::default()
        };
        Event::new(t, options).map_err(|source| Error::Format { source })
    }

    fn moment(t: f64, strength: f64) -> Result<Event, Error> {
        event(t, strength, None, None)
    }

    fn shape(attack: f64, decay: f64, sustain: f64, release: f64, curve: Curve) -> envelope::Params {
        envelope::Params {
            attack,
            decay,
            sustain,
            release,
            curve,
            ..envelope::Params::default()
        }
    }

    fn assert_close(read: &[f64], expected: &[f64]) {
        assert_eq!(read.len(), expected.len());
        for (index, (a, b)) in read.iter().zip(expected).enumerate() {
            assert!((a - b).abs() <= 1e-15, "at {index}: {a} vs {b}\n{read:?}\n{expected:?}");
        }
    }

    #[test]
    fn a_moment_rises_over_the_attack_then_falls_linearly_over_the_decay() -> Result<(), Error> {
        // strength 0.8 at 1 s, attack 0.25 s, then down to 0 over 0.5 s: every value is exact in binary
        let flare = lib::envelope(&[moment(1.0, 0.8)?], shape(0.25, 0.5, 0.3, 0.0, Curve::Linear))?;
        let times = [0.0, 1.0_f64.next_down(), 1.0, 1.125, 1.25, 1.5, 1.75, 1.75_f64.next_up(), 2.0, 1e300];
        // a moment has no sustain: the 0.3 is for events with a duration
        let expected = [0.0, 0.0, 0.0, 0.4, 0.8, 0.4, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(flare.sample(&times)?, expected);
        Ok(())
    }

    #[test]
    fn a_moment_falls_exponentially_and_is_cut_ln_1e9_time_constants_into_the_fall() -> Result<(), Error> {
        let flare = lib::envelope(&[moment(1.0, 0.8)?], shape(0.25, 0.5, 0.0, 0.0, Curve::Exponential))?;
        let reach = 1.0 + (0.25 + 0.5 * LN_1E9); // the cut: the attack's end plus ln(1e9) time constants
        let times = [1.125, 1.25, 1.75, 2.25, 11.0, reach, reach.next_up(), 1e300];
        let expected = [0.4, 0.8, 0.8 / E, 0.8 / E / E, 0.8 * (-19.5_f64).exp(), 0.8 * 1e-9, 0.0, 0.0];
        let read = flare.sample(&times)?;
        assert_close(&read, &expected);
        assert!(read[5] > 0.0 && read[6] == 0.0, "seen at the cut, gone one ulp after: {read:?}");
        Ok(())
    }

    #[test]
    fn the_cut_is_ln_1e9_as_python_computes_it() -> Result<(), Error> {
        // decay 1, no attack: the reach is ln(1e9) exactly, so the last read seen is at LN_1E9 and the next is 0
        let flare = lib::envelope(&[moment(0.0, 1.0)?], shape(0.0, 1.0, 0.0, 0.0, Curve::Exponential))?;
        let read = flare.sample(&[LN_1E9, LN_1E9.next_up()])?;
        assert!(read[0] > 0.0 && read[1] == 0.0, "{read:?}");
        assert_eq!(LN_1E9.to_bits(), 0x4034_b927_f32b_ffb8);
        Ok(())
    }

    #[test]
    fn an_event_with_a_duration_falls_to_its_sustain_holds_and_releases_from_where_it_is() -> Result<(), Error> {
        // strength 1 at 2 s for 1 s: down to the sustain 0.5 over 0.25 s, held, then down to 0 over 0.5 s from 3 s
        let note = [event(2.0, 1.0, Some(1.0), None)?];
        let linear = lib::envelope(&note, shape(0.0, 0.25, 0.5, 0.5, Curve::Linear))?;
        let times = [2.0, 2.125, 2.25, 2.75, 3.0_f64.next_down(), 3.0, 3.25, 3.5, 4.0];
        assert_eq!(linear.sample(&times)?, [1.0, 0.75, 0.5, 0.5, 0.5, 0.5, 0.25, 0.0, 0.0]);
        // exponentially: the level at the end is L = 0.5 + 0.5 e^-4, then L e^-1 one release later, cut at
        // 3 + 0.5 ln(1e9)
        let exponential = lib::envelope(&note, shape(0.0, 0.25, 0.5, 0.5, Curve::Exponential))?;
        let level = 0.5 + 0.5 * (-4.0_f64).exp();
        let reach = 2.0 + (1.0 + 0.5 * LN_1E9);
        let times = [2.0, 2.25, 3.0, 3.5, reach, reach.next_up()];
        let expected = [1.0, 0.5 + 0.5 / E, level, level / E, level * 1e-9, 0.0];
        assert_close(&exponential.sample(&times)?, &expected);
        Ok(())
    }

    #[test]
    fn a_duration_shorter_than_the_attack_stops_the_rise_and_releases_from_where_it_got() -> Result<(), Error> {
        let short = [event(0.0, 1.0, Some(0.5), None)?];
        let flare = lib::envelope(&short, shape(1.0, 0.25, 0.0, 0.5, Curve::Linear))?;
        assert_eq!(flare.sample(&[0.25, 0.5, 0.75, 1.0, 1.25])?, [0.25, 0.5, 0.25, 0.0, 0.0]);
        Ok(())
    }

    #[test]
    fn spans_of_0_jump_at_once() -> Result<(), Error> {
        for (attack, decay, release, event, times, expected, rule) in [
            (0.0, 0.0, 0.0, moment(1.0, 0.8)?, [1.0, 1.5, 3.0], [0.0, 0.0, 0.0], "a moment without attack or decay"),
            (0.5, 0.0, 0.0, moment(1.0, 0.8)?, [1.25, 1.5, 1.75], [0.4, 0.0, 0.0], "up over the attack, then 0"),
            (
                0.0,
                0.0,
                0.0,
                event(1.0, 0.8, Some(1.0), None)?,
                [1.0, 1.5, 2.0],
                [0.4, 0.4, 0.0],
                "straight to the sustain 0.8 * 0.5 until the end, then 0",
            ),
            (
                0.0,
                0.25,
                0.0,
                event(1.0, 1.0, Some(5e-324), None)?,
                [1.0, 1.0_f64.next_up(), 2.0],
                [1.0, 0.0, 0.0],
                "the shortest duration: its strength at its start only",
            ),
        ] {
            let flare = lib::envelope(&[event], shape(attack, decay, 0.5, release, Curve::Exponential))?;
            assert_eq!(flare.sample(&times)?, expected, "{rule}");
        }
        Ok(())
    }

    #[test]
    fn overlapping_events_take_the_larger_or_add_by_mix() -> Result<(), Error> {
        // 1.0 at 1 s and 0.75 at 1.5 s falling linearly over 1 s: at 1.75 s 0.25 and 0.5625
        let events = [moment(1.0, 1.0)?, moment(1.5, 0.75)?];
        for (mix, expected) in [(Mix::Max, [0.0, 0.75, 0.75, 0.5625, 0.0]), (Mix::Sum, [0.0, 0.75, 1.25, 0.8125, 0.0])] {
            let params = envelope::Params {
                mix,
                ..shape(0.0, 1.0, 0.0, 0.0, Curve::Linear)
            };
            assert_eq!(lib::envelope(&events, params.clone())?.sample(&[0.5, 1.25, 1.5, 1.75, 2.5])?, expected);
            let swapped = [events[1].clone(), events[0].clone()];
            assert_eq!(params.source(swapped.to_vec())?.sample(&[0.5, 1.25, 1.5, 1.75, 2.5])?, expected, "{mix:?}");
        }
        Ok(())
    }

    #[test]
    fn under_sum_events_are_added_in_the_order_given_which_decides_the_last_bit() -> Result<(), Error> {
        // three events at one time, read at it: each gives its strength, and (0.1 + 0.2) + 0.3 is 0.6000000000000001
        // while (0.3 + 0.2) + 0.1 is 0.6; the door keeps equal times in the order given
        let sum = envelope::Params {
            mix: Mix::Sum,
            ..envelope::Params::default()
        };
        let rising = [moment(1.0, 0.1)?, moment(1.0, 0.2)?, moment(1.0, 0.3)?];
        let falling = [moment(1.0, 0.3)?, moment(1.0, 0.2)?, moment(1.0, 0.1)?];
        assert_eq!(lib::envelope(&rising, sum.clone())?.at(1.0)?, 0.600_000_000_000_000_1);
        assert_eq!(lib::envelope(&falling, sum.clone())?.at(1.0)?, 0.6);
        assert_eq!(sum.source(rising.to_vec())?.at(1.0)?, 0.600_000_000_000_000_1);
        assert_eq!(sum.source(falling.to_vec())?.at(1.0)?, 0.6);
        // under max the order changes nothing
        let max = envelope::Params::default();
        assert_eq!(lib::envelope(&rising, max.clone())?.at(1.0)?, 0.3);
        assert_eq!(lib::envelope(&falling, max)?.at(1.0)?, 0.3);
        Ok(())
    }

    #[test]
    fn events_at_0_and_minus_0_both_count_from_time_0() -> Result<(), Error> {
        let events = [moment(-0.0, 0.25)?, moment(0.0, 0.5)?];
        for (mix, expected) in [(Mix::Max, 0.5), (Mix::Sum, 0.75)] {
            let params = envelope::Params {
                mix,
                ..envelope::Params::default()
            };
            let read = lib::envelope(&events, params)?.sample(&[-0.0, 0.0, -1.0])?;
            assert_eq!(read, [expected; 3], "{mix:?}");
            assert!(read.iter().all(|value| value.is_sign_positive()));
        }
        Ok(())
    }

    #[test]
    fn an_event_of_strength_0_adds_nothing() -> Result<(), Error> {
        let events = [moment(1.0, 0.0)?, moment(1.0, 0.5)?];
        for mix in [Mix::Max, Mix::Sum] {
            let params = envelope::Params {
                mix,
                ..envelope::Params::default()
            };
            let read = lib::envelope(&events, params)?.sample(&[0.5, 1.0])?;
            assert_eq!(read, [0.0, 0.5], "{mix:?}");
            assert!(read[0].is_sign_positive());
        }
        Ok(())
    }

    #[test]
    fn the_label_chooses_the_events_that_count() -> Result<(), Error> {
        // a kick at 1 s, a snare at 2 s, an unlabelled event at 3 s; a linear decay of 0.5 s ends each before the next
        let events = [
            event(1.0, 0.875, None, Some("kick"))?,
            event(2.0, 0.5, None, Some("snare"))?,
            event(3.0, 0.75, None, None)?,
        ];
        for (label, expected) in [
            ("kick", [0.875, 0.0, 0.0]),
            ("snare", [0.0, 0.5, 0.0]),
            ("", [0.875, 0.5, 0.75]),
            ("hat", [0.0, 0.0, 0.0]),
            ("Kick", [0.0, 0.0, 0.0]),
        ] {
            let params = envelope::Params {
                label: label.to_owned(),
                ..shape(0.0, 0.5, 0.0, 0.0, Curve::Linear)
            };
            assert_eq!(lib::envelope(&events, params)?.sample(&[1.0, 2.0, 3.0])?, expected, "{label:?}");
        }
        Ok(())
    }

    #[test]
    fn no_events_give_0_everywhere() -> Result<(), Error> {
        for mix in [Mix::Max, Mix::Sum] {
            let params = envelope::Params {
                mix,
                ..envelope::Params::default()
            };
            assert_eq!(lib::envelope(&[], params)?.sample(&[0.0, 1.0, 1e300, f64::MAX])?, [0.0; 4]);
        }
        Ok(())
    }

    #[test]
    fn times_and_events_at_the_float_limit_read_as_numbers() -> Result<(), Error> {
        // an event at 1e308 lasting 1e308: its end and reach are beyond the largest float; 0.7e308 s in, it is still
        // held at the sustain 0.5
        let far = [event(1e308, 1.0, Some(1e308), None)?];
        let held = shape(0.0, 0.25, 0.5, 1.0, Curve::Exponential);
        assert_eq!(lib::envelope(&far, held.clone())?.sample(&[0.0, 1e308, 1.7e308, f64::MAX])?, [0.0, 1.0, 0.5, 0.5]);
        // a kick at 1 s, read far away: long fallen
        let kick = [moment(1.0, 1.0)?];
        for curve in [Curve::Exponential, Curve::Linear] {
            let params = shape(60.0, 3600.0, 1.0, 3600.0, curve);
            assert_eq!(lib::envelope(&kick, params)?.sample(&[1e15, 1e300, 1.797e308, f64::MAX])?, [0.0; 4]);
        }
        Ok(())
    }

    #[test]
    fn many_events_and_one_read_give_the_brute_force_value() -> Result<(), Error> {
        // 200 000 kicks, one every 5 ms, decay 0.05: the oracle is the largest exponential of those started
        let strengths: Vec<f64> = (0..200_000_u32).map(|i| f64::from(i % 97 + 3) / 100.0).collect();
        let events = strengths
            .iter()
            .zip(0_u32..)
            .map(|(&strength, i)| moment(f64::from(i) * 0.005, strength))
            .collect::<Result<Vec<_>, _>>()?;
        let flare = lib::envelope(&events, shape(0.0, 0.05, 0.0, 0.0, Curve::Exponential))?;
        let time = 512.3456;
        let oracle = events
            .iter()
            .filter(|kick| kick.t() <= time)
            .map(|kick| kick.strength() * (-(time - kick.t()) / 0.05).exp())
            .fold(0.0, f64::max);
        assert!((flare.at(time)? - oracle).abs() <= 1e-9, "{} vs {oracle}", flare.at(time)?);
        Ok(())
    }

    #[test]
    fn through_the_registry_it_takes_events_and_refuses_a_parameter_beyond_its_range_naming_it() -> Result<(), Error> {
        let thing = Registry::builtin().get("envelope")?;
        let call = |values| -> Result<Call, Error> {
            Ok(Call {
                inputs: [("events".to_owned(), Input::from_events(vec![moment(1.0, 0.5)?]))].into(),
                values,
                ..Call::default()
            })
        };
        let Output::Signal(flare) = thing.call(&call(json!({"decay": 1.0, "curve": "linear"}))?)? else {
            return Err(Error::Call { message: "not a signal".into() });
        };
        assert_eq!(flare.sample(&[0.5, 1.0, 1.5])?, [0.0, 0.5, 0.25]);
        assert_eq!(flare.recipe().map(|recipe| recipe.name.as_str()), Some("envelope"));
        for (values, path) in [(json!({"decay": -1.0}), "decay"), (json!({"mix": "add"}), "mix")] {
            let refused = thing.call(&call(values)?);
            assert!(
                matches!(&refused, Err(Error::Params { shaper, problems, .. })
                    if shaper == "envelope" && problems.len() == 1 && problems[0].path == path),
                "{refused:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn parameters_beyond_their_table_called_directly_never_panic() -> Result<(), Error> {
        // the door refuses these; a direct call gives numbers or the read's refusal, never a panic
        let events = vec![event(1.0, 1.0, Some(0.5), None)?, moment(0.5, 0.5)?];
        for value in [f64::NAN, f64::INFINITY, -1.0, f64::MAX, -0.0] {
            for curve in [Curve::Exponential, Curve::Linear] {
                for mix in [Mix::Max, Mix::Sum] {
                    let params = envelope::Params {
                        mix,
                        ..shape(value, value, value, value, curve)
                    };
                    let read = params.source(events.clone())?.sample(&[0.0, 0.75, 1.25, 2.0, 1e300]);
                    assert!(read.is_ok() || matches!(read, Err(Error::Signal { .. })), "{value} {read:?}");
                }
            }
        }
        Ok(())
    }

    /// The oracle: every event's value at `time` from the formulas of the help, uncut, combined by `mix`.
    fn by_hand(time: f64, events: &[Event], params: &envelope::Params) -> f64 {
        let fall = |elapsed: f64, span: f64| match params.curve {
            _ if span == 0.0 => 0.0,
            Curve::Exponential => (-elapsed / span).exp(),
            Curve::Linear => (1.0 - elapsed / span).max(0.0),
        };
        let before_the_end = |u: f64, strength: f64, sustain: f64| {
            if u < params.attack {
                strength * u / params.attack
            } else {
                strength * (sustain + (1.0 - sustain) * fall(u - params.attack, params.decay))
            }
        };
        let one = |event: &Event| {
            let u = time - event.t();
            let (end, sustain) = event.duration().map_or((f64::INFINITY, 0.0), |duration| (duration, params.sustain));
            if u < 0.0 {
                0.0
            } else if u < end {
                before_the_end(u, event.strength(), sustain)
            } else {
                before_the_end(end, event.strength(), sustain) * fall(u - end, params.release)
            }
        };
        let counted = events.iter().filter(|event| params.label.is_empty() || event.label() == Some(&*params.label));
        match params.mix {
            Mix::Max => counted.map(one).fold(0.0, f64::max),
            Mix::Sum => counted.map(one).sum(),
        }
    }

    fn events() -> impl Strategy<Value = Vec<Event>> {
        let one = (0.0..20.0_f64, 0.0..=1.0_f64, prop::option::of(0.01..5.0_f64), prop_oneof![Just(None), Just(Some("kick"))]);
        prop::collection::vec(one, 0..8).prop_map(|events| {
            events
                .into_iter()
                .filter_map(|(t, strength, duration, label)| event(t, strength, duration, label).ok())
                .collect()
        })
    }

    fn params() -> impl Strategy<Value = envelope::Params> {
        let span = |max: f64| prop_oneof![Just(0.0), 0.0..=max];
        (
            span(2.0),
            span(3.0),
            0.0..=1.0_f64,
            span(3.0),
            prop_oneof![Just(Curve::Exponential), Just(Curve::Linear)],
            prop_oneof![Just(Mix::Max), Just(Mix::Sum)],
            prop_oneof![Just(""), Just("kick")],
        )
            .prop_map(|(attack, decay, sustain, release, curve, mix, label)| envelope::Params {
                label: label.to_owned(),
                attack,
                decay,
                sustain,
                release,
                curve,
                mix,
            })
    }

    proptest! {
        #[test]
        fn the_signal_is_every_events_value_combined_by_mix(
            events in events(), params in params(), times in prop::collection::vec(0.0..40.0_f64, 1..20),
        ) {
            // within a billionth of each strength (8 events at most): what the cut of an exponential tail may drop
            let read = lib::envelope(&events, params.clone())?.sample(&times)?;
            for (value, time) in read.iter().zip(&times) {
                let expected = by_hand(*time, &events, &params);
                prop_assert!((value - expected).abs() <= 8e-9, "at {time}: {value} vs {expected}");
            }
        }

        #[test]
        fn under_max_it_is_never_below_0_nor_above_the_largest_strength(
            events in events(),
            params in params(),
            // mostly where the events are, now and then far beyond them
            times in prop::collection::vec(prop_oneof![9 => 0.0..40.0_f64, 1 => 0.0..1e300_f64], 1..20),
        ) {
            let largest = events.iter().map(Event::strength).fold(0.0, f64::max);
            let max = envelope::Params { mix: Mix::Max, ..params };
            for value in lib::envelope(&events, max)?.sample(&times)? {
                prop_assert!(value.is_sign_positive() && value <= largest, "{value} vs {largest}");
            }
        }

        #[test]
        fn it_is_the_fold_of_the_single_event_signals_in_their_order(
            events in events(), params in params(), times in prop::collection::vec(0.0..30.0_f64, 1..20),
        ) {
            // the events of a single-event signal each, folded in order with max or +, give the same bits
            let mut expected = vec![0.0_f64; times.len()];
            for one in &events {
                let single = lib::envelope(std::slice::from_ref(one), params.clone())?.sample(&times)?;
                for (value, part) in expected.iter_mut().zip(single) {
                    *value = match params.mix { Mix::Max => value.max(part), Mix::Sum => *value + part };
                }
            }
            // called directly, in the order generated (any order, not the door's): folded in that order
            prop_assert_eq!(params.source(events)?.sample(&times)?, expected);
        }

        #[test]
        fn a_read_gives_the_same_values_alone_in_any_order_and_again(
            events in events(), params in params(), times in prop::collection::vec(0.0..30.0_f64, 1..20),
        ) {
            let flare = lib::envelope(&events, params)?;
            let together = flare.sample(&times)?;
            let reversed: Vec<f64> = times.iter().rev().copied().collect();
            let mut backwards = flare.sample(&reversed)?;
            backwards.reverse();
            let alone = times.iter().map(|time| flare.at(*time)).collect::<Result<Vec<_>, _>>()?;
            prop_assert_eq!(&together, &backwards);
            prop_assert_eq!(&together, &alone);
            prop_assert_eq!(&together, &flare.sample(&times)?);
            prop_assert!(together.iter().all(|value| value.is_finite()));
        }
    }
}
