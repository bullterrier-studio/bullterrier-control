//! A signal that follows its input like a mass on a spring: it overshoots and rings, or glides in without overshoot.
//!
//! The output starts at the input's value at rest and is pulled toward the input with a natural `frequency` (the
//! speed of its swing) and a `damping` ratio: below 1 it overshoots and rings, 1 is the quickest approach without
//! overshoot, above 1 it glides in slowly, and 0 never stops swinging. After a jump of the input a ringing spring
//! looks like a bounce. Each step of `1 / rate` seconds is solved exactly for an input held over the step (not
//! approximated), so any `frequency` and `damping` are stable at any `rate`. The output is a staircase of those
//! steps. `follow` and `slew` also chase their input, but never overshoot.

use crate::{Error, Shaper, Signal, recurrence};

/// How the output swings toward its input.
#[derive(Clone, Copy, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Swings a second of the spring when it is not damped, 0.01 to 100; raising it makes the output arrive and ring
    /// faster.
    #[param(default = 2.0, unit = Hertz, min = 0.01, max = 100)]
    pub frequency: f64,
    /// How strongly the swing is held back, 0 to 10: 0 never stops, below 1 overshoots and rings (smaller rings
    /// longer), 1 arrives as fast as it can without overshoot, above 1 glides in slowly. Raising it calms the output.
    #[param(default = 0.5, unit = Ratio, min = 0, max = 10)]
    pub damping: f64,
    /// Internal samples per second; the output is exact to one sample. Raising it is more exact and slower.
    #[param(default = 1000.0, unit = Hertz, min = 10, max = 48000, advanced)]
    pub rate: f64,
}

impl Signal {
    /// This signal followed by a spring with the frequency and damping of `params`: the shaper [`spring`](super), as
    /// [`pipe`](Signal::pipe) of `params` makes it (checked, named and recorded alike).
    ///
    /// ```
    /// use bullterrier_control::builtin::spring;
    /// use bullterrier_control::{FromValues, Interpolation, Signal};
    ///
    /// // 0, then 1 from the second sample on, at 10 samples a second
    /// let step = Signal::from_values(&[0.0, 1.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
    /// // a spring that just does not overshoot, at 1 swing a second: after one sample (0.1 s) it has covered
    /// // 1 - e^(-w t) (1 + w t) of the way, with w = 2 pi
    /// let params = spring::Params { frequency: 1.0, damping: 1.0, rate: 10.0 };
    /// let (w, t) = (std::f64::consts::TAU, 0.1);
    /// let covered = step.spring(params)?.at(0.15)?;
    /// assert!((covered - (1.0 - (-w * t).exp() * (1.0 + w * t))).abs() < 1e-12);
    /// assert!(step.spring(spring::Params { damping: 11.0, ..params }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// As [`Signal::pipe`]: [`Error::Params`] under the name `spring` listing every rule `params` breaks (a field out
    /// of its range or not finite).
    pub fn spring(&self, params: Params) -> Result<Signal, Error> {
        self.pipe(params)
    }
}

impl Shaper for Params {
    const NAME: &'static str = "spring";
    type Inputs = ();

    /// `input` followed by the spring, in steps of `1 / rate` seconds.
    fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> {
        recurrence::over(input, self.rate, *self, run)
    }
}

/// The spring's position over `inputs`: at rest on the first input, then each sample solves the motion exactly over
/// the step that ends there, the input held at that sample's value. Output *i* depends on inputs 0 to *i* only. A NaN
/// input makes every later output NaN, for `sample` to report.
fn run(inputs: &[f64], params: &Params) -> Vec<f64> {
    let Some(&first) = inputs.first() else {
        return Vec::new();
    };
    let step = Step::new(params);
    let mut position = first;
    let mut velocity = 0.0;
    inputs
        .iter()
        .enumerate()
        .map(|(index, &target)| {
            if index > 0 {
                (position, velocity) = step.apply(position - target, velocity);
                position += target;
            }
            position
        })
        .collect()
}

/// One step of the motion of `e'' + 2 d w e' + w^2 e = 0` (the gap `e` between the spring and its input, `w` the
/// angular frequency and `d` the damping ratio) over `h = 1 / rate` seconds, as the matrix taking `(e, e')` to its
/// values `h` later. The solution is `e^(-d w t)` times a pair of waves: `cos` and `sin` below `d = 1`, `1` and `t` at
/// 1, `cosh` and `sinh` above. All three give `(e, e') -> E (c + s d w, s; -w^2 s, c - s d w)` with `c` the first wave and `s`
/// the second divided by its own angular frequency (`sin(a t) / a`, `t`, `sinh(b t) / b`).
struct Step {
    decay: f64,
    even: f64,
    odd: f64,
    damping_per_second: f64,
    stiffness: f64,
}

impl Step {
    fn new(params: &Params) -> Self {
        let omega = std::f64::consts::TAU * params.frequency;
        let h = 1.0 / params.rate;
        let sigma = params.damping * omega;
        let (even, odd) = if params.damping < 1.0 {
            let swing = omega * libm::sqrt(1.0 - params.damping * params.damping);
            (libm::cos(swing * h), libm::sin(swing * h) / swing)
        } else if params.damping > 1.0 {
            let glide = omega * libm::sqrt(params.damping * params.damping - 1.0);
            (libm::cosh(glide * h), libm::sinh(glide * h) / glide)
        } else {
            (1.0, h)
        };
        Self {
            decay: libm::exp(-sigma * h),
            even,
            odd,
            damping_per_second: sigma,
            stiffness: omega * omega,
        }
    }

    /// `(gap, rate of change of the gap)` one step later.
    fn apply(&self, gap: f64, speed: f64) -> (f64, f64) {
        let Self { decay, even, odd, damping_per_second: sigma, stiffness } = *self;
        (
            decay * ((even + sigma * odd) * gap + odd * speed),
            decay * (-stiffness * odd * gap + (even - sigma * odd) * speed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    /// 0 for the first sample, then 1: a step of the input at the second sample.
    fn step_input(len: usize) -> Vec<f64> {
        std::iter::once(0.0).chain(std::iter::repeat_n(1.0, len - 1)).collect()
    }

    /// The textbook response of a spring at rest to a unit step, `time` seconds after it, from its closed form.
    fn closed_form(frequency: f64, damping: f64, time: f64) -> f64 {
        let omega = std::f64::consts::TAU * frequency;
        let sigma = damping * omega;
        let fade = (-sigma * time).exp();
        if damping < 1.0 {
            let swing = omega * (1.0 - damping * damping).sqrt();
            1.0 - fade * ((swing * time).cos() + sigma / swing * (swing * time).sin())
        } else if damping > 1.0 {
            let glide = omega * (damping * damping - 1.0).sqrt();
            1.0 - fade * ((glide * time).cosh() + sigma / glide * (glide * time).sinh())
        } else {
            1.0 - fade * (1.0 + omega * time)
        }
    }

    #[test]
    fn a_step_gets_the_textbook_response_whether_it_rings_arrives_or_glides() {
        // rate 100: output n after the step (the step at sample 1) has had n steps of 0.01 s
        for damping in [0.0, 0.2, 0.7, 1.0, 1.5, 4.0] {
            let params = Params { frequency: 2.0, damping, rate: 100.0 };
            let out = run(&step_input(200), &params);
            for (index, got) in out.iter().enumerate().skip(1) {
                let want = closed_form(2.0, damping, index as f64 * 0.01);
                assert!((got - want).abs() <= 1e-9, "damping {damping}, sample {index}: {got} against {want}");
            }
        }
    }

    #[test]
    fn a_spring_of_no_damping_swings_between_its_start_and_twice_the_jump() {
        // 1 swing a second at 400 samples a second: the half swing (0.5 s) is 200 steps, so output 200 is at 2, the far end
        let out = run(&step_input(1500), &Params { frequency: 1.0, damping: 0.0, rate: 400.0 });
        assert!((out[200] - 2.0).abs() <= 1e-9, "{}", out[200]);
        let (low, high) = out[1..].iter().fold((f64::MAX, f64::MIN), |(low, high), v| (low.min(*v), high.max(*v)));
        assert!(low >= -1e-9 && high <= 2.0 + 1e-9, "{low} {high}");
    }

    #[test]
    fn at_or_above_damping_1_a_step_never_overshoots_and_only_rises() {
        for damping in [1.0, 1.01, 2.0, 10.0] {
            let out = run(&step_input(3000), &Params { frequency: 3.0, damping, rate: 1000.0 });
            assert!(out.windows(2).all(|pair| pair[1] >= pair[0]), "{damping}");
            assert!(out.iter().all(|value| *value <= 1.0 + 1e-12), "{damping}");
        }
    }

    #[test]
    fn the_response_changes_smoothly_through_damping_1() {
        let at = |damping: f64| run(&step_input(300), &Params { frequency: 5.0, damping, rate: 1000.0 });
        let (below, exact, above) = (at(1.0_f64.next_down()), at(1.0), at(1.0_f64.next_up()));
        for ((low, mid), high) in below.iter().zip(&exact).zip(&above) {
            assert!((low - mid).abs() <= 1e-6 && (high - mid).abs() <= 1e-6, "{low} {mid} {high}");
        }
    }

    #[test]
    fn it_stays_finite_at_the_ends_of_every_range() {
        for (frequency, damping, rate) in [(0.01, 0.0, 10.0), (100.0, 10.0, 10.0), (100.0, 0.0, 10.0), (100.0, 10.0, 48000.0)] {
            let out = run(&step_input(500), &Params { frequency, damping, rate });
            assert!(out.iter().all(|value| value.is_finite()), "{frequency} {damping} {rate}");
        }
    }

    #[test]
    fn nothing_follows_nothing_and_a_nan_poisons_what_follows_it() {
        assert!(run(&[], &Params::default()).is_empty());
        let out = run(&[1.0, f64::NAN, 1.0], &Params::default());
        assert!(out[0] == 1.0 && out[1].is_nan() && out[2].is_nan(), "{out:?}");
    }

    #[test]
    fn the_core_runs_it_by_name_with_its_parameters_checked() -> Result<(), Error> {
        let input = Signal::constant(0.5)?;
        let out = crate::shape("spring", &input, &json!({"frequency": 3.0}))?.sample(&[0.0, 2.0])?;
        assert_eq!(out, [0.5, 0.5], "an input that never moves leaves it at rest");
        let refused = crate::shape("spring", &input, &json!({"damping": 11.0}));
        assert!(matches!(&refused, Err(Error::Params { shaper, .. }) if shaper == "spring"), "{refused:?}");
        Ok(())
    }

    proptest! {
        #[test]
        fn the_output_is_linear_in_the_input_and_never_depends_on_what_comes_later(
            first in prop::collection::vec(-10.0_f64..=10.0, 1..40),
            second in prop::collection::vec(-10.0_f64..=10.0, 1..40),
            extra in prop::collection::vec(-10.0_f64..=10.0, 0..10),
            frequency in 0.01_f64..=100.0,
            damping in 0.0_f64..=10.0,
        ) {
            let params = Params { frequency, damping, rate: 100.0 };
            let len = first.len().min(second.len());
            let sum: Vec<f64> = first.iter().zip(&second).map(|(a, b)| a + b).collect();
            let (a, b, together) = (run(&first[..len], &params), run(&second[..len], &params), run(&sum, &params));
            for index in 0..len {
                prop_assert!((a[index] + b[index] - together[index]).abs() <= 1e-9, "{index}");
            }
            let longer: Vec<f64> = first.iter().chain(&extra).copied().collect();
            prop_assert_eq!(&run(&longer, &params)[..first.len()], &run(&first, &params)[..]);
        }
    }
}
