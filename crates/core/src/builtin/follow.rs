//! `follow`: smooths its input with separate attack and release times. Its parameters are declared here, once; what it
//! does is documented once, on the Python shaper.


/// How quickly the output chases its input upward and downward.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// Seconds the output takes to close about two thirds (1 - 1/e) of the gap to a higher input; larger is slower. 0
    /// follows a rise at once.
    #[param(default = 0.01, unit = Seconds, min = 0, max = 3600)]
    pub attack: f64,
    /// Seconds the output takes to close about two thirds (1 - 1/e) of the gap to a lower input; larger is slower. 0
    /// follows a fall at once.
    #[param(default = 0.25, unit = Seconds, min = 0, max = 3600)]
    pub release: f64,
    /// Internal samples per second; the output is exact to one sample. Raising it is more exact and slower.
    #[param(default = 1000.0, unit = Hertz, min = 10, max = 48000, advanced)]
    pub rate: f64,
}

/// The share of the gap an input leaves after one step: `e^(-step / time)`, and none for a time of 0 (so the output is
/// the input, and nothing divides).
fn left(step: f64, time: f64) -> f64 {
    if time > 0.0 { (-step / time).exp() } else { 0.0 }
}

/// The followed series: it starts at the first input and, with every next one, keeps a share of its level and takes the
/// rest from the input, the shares of `attack` while the input is above the level and of `release` otherwise. The
/// result is `kept * level + taken * target`, a mix that cannot overflow however far apart the two are.
pub(crate) fn run(inputs: &[f64], params: &Params) -> Vec<f64> {
    let step = 1.0 / params.rate;
    let left_up = left(step, params.attack);
    let left_down = left(step, params.release);
    let rising = (left_up, 1.0 - left_up);
    let falling = (left_down, 1.0 - left_down);
    let Some(&first) = inputs.first() else {
        return Vec::new();
    };
    let mut level = first;
    inputs
        .iter()
        .map(|&target| {
            // a NaN target is not above the level: it takes the falling share, and comes through for `sample` to report
            let (kept, taken) = if target > level { rising } else { falling };
            level = kept * level + taken * target;
            level
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn params(attack: f64, release: f64, rate: f64) -> Params {
        Params { attack, release, rate }
    }

    #[test]
    fn times_of_zero_return_the_input_exactly_even_at_the_ends_of_the_float_range() {
        let inputs = [0.0, 1.7e308, -1.7e308, 1.7e308, 0.25, -1.7e308];
        assert_eq!(run(&inputs, &params(0.0, 0.0, 1000.0)), inputs);
    }

    #[test]
    fn a_step_up_closes_one_minus_e_to_the_minus_one_of_the_gap_in_its_first_sample() {
        // rate 10: a sample is 0.1 s; attack 0.1 s: the gap shrinks by e^(-0.1 / 0.1) = e^-1; 1 - e^-1 from the formula
        let out = run(&[0.0, 1.0], &params(0.1, 0.1, 10.0));
        assert_eq!(out.len(), 2);
        assert!(out[0].abs() <= 1e-12 && (out[1] - 0.632_120_558_828_557_7).abs() <= 1e-12, "{out:?}");
    }

    #[test]
    fn a_step_down_uses_the_release_time_not_the_attack() {
        // from 1 to 0 with release 0.2 s at rate 10: the gap left is e^(-0.1 / 0.2) = e^-0.5 = 0.6065306597126334
        let out = run(&[1.0, 0.0], &params(0.1, 0.2, 10.0));
        assert_eq!(out.len(), 2);
        assert!((out[0] - 1.0).abs() <= 1e-12 && (out[1] - 0.606_530_659_712_633_4).abs() <= 1e-12, "{out:?}");
    }

    #[test]
    fn no_inputs_give_no_outputs() {
        assert_eq!(run(&[], &params(0.1, 0.2, 10.0)), Vec::<f64>::new());
    }

    #[test]
    fn a_nan_input_comes_through_where_it_is_without_a_panic() {
        // a broken upstream signal: the NaN reaches the output at its sample, where `sample` reports it
        let out = run(&[0.0, f64::NAN, 1.0], &params(0.1, 0.2, 10.0));
        assert_eq!(out.len(), 3);
        assert!(out[0].abs() <= 1e-12 && out[1].is_nan(), "{out:?}");
    }

    proptest! {
        #[test]
        fn the_output_stays_within_the_lowest_and_highest_input_read_so_far(
            inputs in prop::collection::vec(-5.0_f64..=5.0, 1..40),
            attack in 0.0_f64..=3.0,
            release in 0.0_f64..=3.0,
            rate in 10.0_f64..=48000.0,
        ) {
            let out = run(&inputs, &params(attack, release, rate));
            prop_assert_eq!(out.len(), inputs.len());
            let (mut lowest, mut highest) = (f64::INFINITY, f64::NEG_INFINITY);
            for (value, target) in out.iter().zip(&inputs) {
                (lowest, highest) = (lowest.min(*target), highest.max(*target));
                prop_assert!(*value >= lowest - 1e-9 && *value <= highest + 1e-9, "{value} beyond {lowest}..{highest}");
            }
        }
    }
}
