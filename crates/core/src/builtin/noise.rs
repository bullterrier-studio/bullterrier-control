//! Seeded random noise between `low` and `high`: a new random value every 1 / `rate` seconds, held, joined by straight
//! lines or eased between them.
//!
//! The random value at whole position k (k = floor(rate * t)) is the k-th number of the stream of `seed` (`SplitMix64`,
//! mapped to 0..1), so the signal is a pure function of seed and time: it can be read at any time in any order and is
//! the same on every machine. A read at position k + x (x from 0 to 1) is the value at k for 'hold', the straight line
//! from k to k + 1 for 'linear', or that line eased by 3x^2 - 2x^3 for 'smooth', which has no slope at a lattice point.
//! The value is low + (high - low) times that, and never leaves the range between `low` and `high`. A time too far
//! from 0 for the position to be a number is a `SignalError`.

use crate::cadence::{Per, positions};
use crate::hashing::uniform;
use crate::scaling::between;
use crate::{Choice, Error, Signal, Source};

/// The choices of `shape`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Shape {
    /// The choice `hold`.
    Hold,
    /// The choice `linear`.
    Linear,
    /// The choice `smooth`.
    Smooth,
}

/// The noise: how it moves between its random values, how often it draws a new one, and its seed and range.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// How the value moves from one random value to the next: 'hold' (steps), 'linear' (straight lines, with corners)
    /// or 'smooth' (eased, no corners).
    #[param(default = Shape::Smooth)]
    pub shape: Shape,
    /// Random values per second: a new one every 1 / rate seconds. Raising it makes the noise busier.
    #[param(default = 4.0, unit = Hertz, min = 0.001, max = 1000)]
    pub rate: f64,
    /// Which noise, 0 to 4294967295: the same seed gives the same values on every machine, read in any order; another
    /// seed gives other values.
    #[param(default = 0, unit = Count, min = 0, max = 4294967295)]
    pub seed: i64,
    /// The bottom of the range the values fall in; set above `high` to flip the noise.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub low: f64,
    /// The top of the range the values fall in.
    #[param(default = 1.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub high: f64,
}

/// 2^64: a position is reduced to the index the hash reads, so whole numbers wrap here (and the float 2^64 is exact).
const INDEX_PERIOD: f64 = 18_446_744_073_709_551_616.0;

impl Params {
    /// The value at `position` (cycles counted from 0): the lattice values around it joined as `shape` says.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "seed is 0 to 4294967295; a position is not negative (rate and time are not), and whole % 2^64 is a whole number from 0 to 2^64 - 1"
    )]
    fn value_at(&self, position: f64) -> f64 {
        let seed = self.seed as u64;
        let whole = position.floor();
        let fraction = position - whole;
        // fmod is exact, so a position beyond 2^64 reads the index its integer wraps to
        let first = (whole % INDEX_PERIOD) as u64;
        let here = uniform(seed, first);
        let progress = match self.shape {
            Shape::Hold => return between(self.low, self.high, here),
            Shape::Linear => fraction,
            // 3x^2 - 2x^3: 0 at 0 and 1 at 1, with no slope at either end
            Shape::Smooth => fraction * fraction * (3.0 - 2.0 * fraction),
        };
        let there = uniform(seed, first.wrapping_add(1));
        // plain operators: Rust never fuses a multiply and an add
        between(self.low, self.high, here + (there - here) * progress)
    }
}

impl Source for Params {
    const NAME: &'static str = "noise";
    type Inputs = ();

    /// The signal of the noise of `seed`, read at any time.
    ///
    /// ```
    /// use bullterrier_control::builtin::noise;
    /// use bullterrier_control::lib;
    ///
    /// // one value a second, held: the same seed gives the same value at the same position, in the range
    /// let wobble = lib::noise(noise::Params { shape: noise::Shape::Hold, rate: 1.0, seed: 7, ..Default::default() })?;
    /// let values = wobble.sample(&[0.5, 0.9, 1.5])?;
    /// assert_eq!(values[0], values[1]); // both in the first second
    /// assert!(values.iter().all(|value| (0.0..=1.0).contains(value)));
    /// assert_eq!(wobble.sample(&[1.5])?, [values[2]]);
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// None when made; a read is an [`Error::Signal`] at a time too far from 0 for `rate * t` to be a number.
    fn source(&self, (): ()) -> Result<Signal, Error> {
        let params = self.clone();
        Ok(Signal::from_fn(move |times, out| {
            for (value, position) in out.iter_mut().zip(positions(params.rate, Per::Second, None, times)?) {
                *value = params.value_at(position);
            }
            Ok::<(), Error>(())
        }))
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "the lattice values are promised equal everywhere, not merely close")]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;
    use crate::registry::{Call, Kind, Output, Returns};
    use crate::{Registry, builtin::noise, lib};

    /// The first outputs of `SplitMix64` as published (Vigna's `splitmix64.c`; the `rand_core` test vectors), the same
    /// vectors `hashing` is held to: `(seed, outputs from index 0)`.
    const PUBLISHED: [(u64, &[u64]); 4] = [
        (0, &[0xE220_A839_7B1D_CDAF, 0x6E78_9E6A_A1B9_65F4, 0x06C4_5D18_8009_454F, 0xF88B_B8A8_724C_81EC]),
        (
            1_234_567,
            &[6_457_827_717_110_365_317, 3_203_168_211_198_807_973, 9_817_491_932_198_370_423, 4_593_380_528_125_082_431],
        ),
        (7, &[0x63CB_E1E4_5932_0DD7, 0x044C_3CD7_F43C_661C, 0xE698_4080_BAB1_2A02]),
        (4_294_967_295, &[0x73B1_3BA2_AFF1_81C0, 0x6120_4305_1340_D3B4]),
    ];

    /// The value a published 64-bit output stands for: its top 53 bits over 2^53.
    #[allow(clippy::cast_precision_loss, reason = "53 significant bits are exact in an f64")]
    fn value_of(output: u64) -> f64 {
        (output / 2048) as f64 / 9_007_199_254_740_992.0
    }

    /// `SplitMix64` in 128-bit integers masked to 64 bits (no wrapping arithmetic): the value at any index below 2^64.
    #[allow(clippy::cast_precision_loss, reason = "53 significant bits are exact in an f64")]
    fn reference(seed: u64, index: u128) -> f64 {
        const MASK: u128 = (1 << 64) - 1;
        let mut z = (u128::from(seed) + (index + 1) * 0x9E37_79B9_7F4A_7C15) & MASK;
        z = ((z ^ (z >> 30)) * 0xBF58_476D_1CE4_E5B9) & MASK;
        z = ((z ^ (z >> 27)) * 0x94D0_49BB_1331_11EB) & MASK;
        z ^= z >> 31;
        (z >> 11) as f64 / 9_007_199_254_740_992.0
    }

    fn params(shape: Shape, rate: f64, seed: i64) -> noise::Params {
        noise::Params { shape, rate, seed, ..noise::Params::default() }
    }

    fn read(params: noise::Params, times: &[f64]) -> Result<Vec<f64>, Error> {
        lib::noise(params)?.sample(times)
    }

    #[test]
    fn the_reference_hash_is_the_published_one() {
        for (seed, outputs) in PUBLISHED {
            for (index, &output) in (0..).zip(outputs) {
                assert_eq!(reference(seed, index), value_of(output), "seed {seed} index {index}");
            }
        }
    }

    #[test]
    fn hold_reads_the_published_value_of_the_lattice_point_it_has_passed() -> Result<(), Error> {
        // rate 1: time 0.5 + k is inside position k, anywhere in it (0.0, 0.999 too)
        for (seed, outputs) in PUBLISHED {
            let seed_param = i64::try_from(seed).map_err(|e| Error::Call { message: e.to_string() })?;
            for (index, &output) in (0..).zip(outputs) {
                let at = f64::from(index);
                for into in [0.0, 0.5, 0.999] {
                    let got = read(params(Shape::Hold, 1.0, seed_param), &[at + into])?;
                    assert_eq!(got, [value_of(output)], "seed {seed} index {index} +{into}");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn a_step_edge_sits_where_rate_times_time_reaches_a_whole_number() -> Result<(), Error> {
        let (v0, v1) = (value_of(PUBLISHED[0].1[0]), value_of(PUBLISHED[0].1[1]));
        let below_one = f64::from_bits(1.0_f64.to_bits() - 1);
        assert_eq!(read(params(Shape::Hold, 1.0, 0), &[below_one, 1.0])?, [v0, v1]);
        // rate 4: position 1 is time 0.25
        assert_eq!(read(params(Shape::Hold, 4.0, 0), &[0.2499, 0.25])?, [v0, v1]);
        Ok(())
    }

    #[test]
    fn linear_and_smooth_join_the_lattice_values_by_a_line_and_by_3x_squared_minus_2x_cubed() -> Result<(), Error> {
        // (shape, [(fraction, weight of the next value)]): 3 * 0.25^2 - 2 * 0.25^3 = 0.15625, 0.5 -> 0.5, 0.75 -> 0.84375
        let rows = [
            (Shape::Linear, [(0.0, 0.0), (0.25, 0.25), (0.5, 0.5), (0.75, 0.75)]),
            (Shape::Smooth, [(0.0, 0.0), (0.25, 0.15625), (0.5, 0.5), (0.75, 0.84375)]),
        ];
        for (shape, weights) in rows {
            for k in 0..3_usize {
                let (here, there) = (value_of(PUBLISHED[0].1[k]), value_of(PUBLISHED[0].1[k + 1]));
                for (fraction, weight) in weights {
                    let time = f64::from(u32::try_from(k).map_err(|e| Error::Call { message: e.to_string() })?) + fraction;
                    let got = read(params(shape, 1.0, 0), &[time])?[0];
                    // the other way round from the code: a weighted mean of the two values
                    let expected = here * (1.0 - weight) + there * weight;
                    assert!((got - expected).abs() <= 1e-15, "{shape:?} k {k} +{fraction}: {got} vs {expected}");
                    if fraction == 0.0 {
                        assert_eq!(got, here, "{shape:?} k {k}: exactly the lattice value");
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn the_value_is_put_on_low_to_high_and_low_above_high_flips_it() -> Result<(), Error> {
        let v0 = value_of(PUBLISHED[0].1[0]);
        for (low, high, expected) in [(-1.0, 3.0, -1.0 + 4.0 * v0), (3.0, -1.0, 3.0 - 4.0 * v0), (2.0, 2.0, 2.0)] {
            let got = read(noise::Params { shape: Shape::Hold, rate: 1.0, seed: 0, low, high }, &[0.5])?[0];
            assert!((got - expected).abs() <= 1e-12, "{low}..{high}: {got} vs {expected}");
        }
        // the top is exactly high, where 0.3 + 0.6 * 1 is not
        let near_one = noise::Params { shape: Shape::Linear, rate: 1.0, seed: 0, low: 0.3, high: 0.9 };
        assert!(read(near_one, &[0.5, 1.5, 2.5])?.iter().all(|value| (0.3..=0.9).contains(value)));
        Ok(())
    }

    #[test]
    fn a_far_position_reads_the_index_its_integer_wraps_to() -> Result<(), Error> {
        let past_u64 = f64::from_bits(2.0_f64.powi(64).to_bits() + 1); // 2^64 + 4096
        // (time at rate 1, the index it wraps to)
        let rows: [(f64, u128); 8] = [
            (9_007_199_254_740_992.0, 1 << 53),
            (9_223_372_036_854_775_808.0, 1 << 63),
            (1e19, 10_000_000_000_000_000_000),
            (3e19, 11_553_255_926_290_448_384), // 3e19 is exact: 3e19 - 2^64
            (1e22, 1_864_712_049_423_024_128), // 1e22 is exact: 1e22 mod 2^64
            (18_446_744_073_709_551_616.0, 0),
            (past_u64, 4096),
            (1e300, 0), // 1e300 is a multiple of 2^944, so a multiple of 2^64
        ];
        for seed in [0, 3, 2_147_483_648, 4_294_967_295] {
            for (time, index) in rows {
                let got = read(params(Shape::Hold, 1.0, seed), &[time])?;
                assert_eq!(got, [reference(u64::try_from(seed).map_err(|e| Error::Call { message: e.to_string() })?, index)], "seed {seed} time {time:e}");
            }
        }
        // and the published value for the index 2^63
        assert_eq!(read(params(Shape::Hold, 1.0, 7), &[9_223_372_036_854_775_808.0])?, [value_of(0x63DC_0BBB_DDD4_0FA8)]);
        Ok(())
    }

    #[test]
    fn linear_and_smooth_join_far_lattice_values_too() -> Result<(), Error> {
        // position 2^52 - 0.5 is the last one with a fraction: halfway from index 2^52 - 1 to index 2^52
        let time = 4_503_599_627_370_495.5;
        let expected = f64::midpoint(reference(5, (1 << 52) - 1), reference(5, 1 << 52));
        for shape in [Shape::Linear, Shape::Smooth] {
            let got = read(params(shape, 1.0, 5), &[time])?[0];
            assert!((got - expected).abs() <= 1e-15, "{shape:?}: {got} vs {expected}");
        }
        Ok(())
    }

    #[test]
    fn rate_is_values_a_second() -> Result<(), Error> {
        for shape in [Shape::Hold, Shape::Linear, Shape::Smooth] {
            // times and positions are dyadic, so rate * t is exact
            let slow = read(params(shape, 0.5, 5), &[0.5, 1.5, 7.25])?;
            let fast = read(params(shape, 8.0, 5), &[0.03125, 0.09375, 0.453_125])?;
            assert_eq!(slow, fast, "{shape:?}");
        }
        Ok(())
    }

    #[test]
    fn different_seeds_give_different_noise_at_every_position() -> Result<(), Error> {
        let times: Vec<f64> = (0..50).map(|k| f64::from(k) + 0.5).collect();
        for (first, second) in [(0, 1), (0, 2_147_483_648), (1, 4_294_967_295)] {
            let (a, b) = (read(params(Shape::Hold, 1.0, first), &times)?, read(params(Shape::Hold, 1.0, second), &times)?);
            assert!(a.iter().zip(&b).all(|(x, y)| x != y), "seeds {first} and {second}");
        }
        Ok(())
    }

    #[test]
    fn a_time_before_0_reads_as_0() -> Result<(), Error> {
        for shape in [Shape::Hold, Shape::Smooth] {
            let got = read(params(shape, 4.0, 3), &[-5.0, -1e-300, -0.0, 0.0])?;
            assert!(got.iter().all(|value| *value == got[3]), "{shape:?}: {got:?}");
        }
        Ok(())
    }

    #[test]
    fn a_time_too_far_for_the_position_is_an_error_naming_it_and_a_slow_rate_still_reads() -> Result<(), Error> {
        let refused = read(params(Shape::Smooth, 1000.0, 0), &[1.0, 1.797e308]).map_err(|e| e.to_string());
        assert!(matches!(&refused, Err(text) if text.contains("1.797e308")), "{refused:?}");
        // 0.001 * 1.797e308 is finite; it is a multiple of 2^64, so the index is 0
        assert_eq!(read(params(Shape::Hold, 0.001, 0), &[1.797e308])?, [reference(0, 0)]);
        Ok(())
    }

    #[test]
    fn through_the_registry_it_is_checked_recorded_and_described() -> Result<(), Error> {
        let thing = Registry::builtin().get("noise")?;
        assert_eq!((thing.kind(), thing.returns()), (Kind::Source, Returns::Signal));
        assert!(thing.summary().starts_with("Seeded random noise between `low` and `high`"), "{}", thing.summary());
        assert_eq!(thing.inputs(), []);
        let made = thing.call(&Call { values: json!({"shape": "hold", "rate": 1, "seed": 0}), ..Call::default() })?;
        let Output::Signal(made) = made else {
            return Err(Error::Call { message: format!("not a signal: {made:?}") });
        };
        assert_eq!(made.sample(&[0.5])?, [value_of(PUBLISHED[0].1[0])]);
        let recipe = made.recipe().ok_or(Error::Signal { message: "no recipe".into() })?;
        assert_eq!((recipe.name.as_str(), recipe.values.clone()), ("noise", json!({"shape": "hold", "rate": 1.0, "seed": 0, "low": 0.0, "high": 1.0})));
        for (id, values, field) in [
            ("a seed past the largest u32", json!({"seed": 4_294_967_296_i64}), "seed"),
            ("a negative seed", json!({"seed": -1}), "seed"),
            ("a rate of 0", json!({"rate": 0}), "rate"),
            ("a rate past 1000", json!({"rate": 1000.5}), "rate"),
            ("a shape it does not have", json!({"shape": "wobbly"}), "shape"),
            ("a low past the range", json!({"low": 1e7}), "low"),
            ("a seed that is not whole", json!({"seed": 1.5}), "seed"),
        ] {
            let refused = thing.call(&Call { values, ..Call::default() });
            assert!(
                matches!(&refused, Err(Error::Params { shaper, problems, .. })
                    if shaper == "noise" && problems.iter().any(|problem| problem.path == field)),
                "{id}: {refused:?}"
            );
        }
        Ok(())
    }

    mod properties {
        use super::*;

        fn shape() -> impl Strategy<Value = Shape> {
            prop_oneof![Just(Shape::Hold), Just(Shape::Linear), Just(Shape::Smooth)]
        }

        proptest! {
            #[test]
            fn the_output_never_leaves_low_and_high_whichever_way_round_and_does_not_depend_on_the_batch(
                shape in shape(),
                rate in 0.001_f64..=1000.0,
                seed in 0_i64..=4_294_967_295,
                low in -1.0e6_f64..=1.0e6,
                high in -1.0e6_f64..=1.0e6,
                times in prop::collection::vec(prop_oneof![0.0_f64..=100.0, 0.0_f64..=1e300], 1..12),
            ) {
                let signal = lib::noise(noise::Params { shape, rate, seed, low, high })
                    .map_err(|error| TestCaseError::fail(error.to_string()))?;
                let together = signal.sample(&times).map_err(|error| TestCaseError::fail(error.to_string()))?;
                for (time, value) in times.iter().zip(&together) {
                    prop_assert!(low.min(high) <= *value && *value <= low.max(high), "{} not in {}..{}", value, low, high);
                    let alone = signal.sample(&[*time]).map_err(|error| TestCaseError::fail(error.to_string()))?;
                    prop_assert_eq!(alone[0], *value);
                }
            }

            #[test]
            fn smooth_and_linear_reach_the_next_lattice_value_as_the_position_does(
                seed in 0_i64..=4_294_967_295,
                k in 0_u32..1_000_000,
            ) {
                // one ulp under position k + 1 the line is within a few ulps of the value at k + 1, which hold reads at k + 1
                let next = f64::from(k) + 1.0;
                let nearly = f64::from_bits(next.to_bits() - 1);
                let held = read(params(Shape::Hold, 1.0, seed), &[next]).map_err(|error| TestCaseError::fail(error.to_string()))?[0];
                for shape in [Shape::Linear, Shape::Smooth] {
                    let got = read(params(shape, 1.0, seed), &[nearly]).map_err(|error| TestCaseError::fail(error.to_string()))?[0];
                    prop_assert!((got - held).abs() <= 1e-9, "{:?}: {} vs {}", shape, got, held);
                }
            }
        }
    }
}
