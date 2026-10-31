//! A random number that is a pure function of a seed and an index: readable at any index, in any order, the same on
//! every platform.
//!
//! ```
//! use bullterrier_control::hashing::uniform;
//!
//! // output 0 of SplitMix64 from state 0 is 0xE220A8397B1DCDAF; the value is its top 53 bits over 2^53
//! assert_eq!(uniform(0, 0), (0xE220_A839_7B1D_CDAF_u64 >> 11) as f64 / 9_007_199_254_740_992.0);
//! assert_eq!(uniform(0, 0), uniform(0, 0)); // the same arguments, the same number
//! assert!((0.0..1.0).contains(&uniform(7, 1_000_000)));
//! ```
//!
//! Promises:
//! - `uniform(seed, index)` is output number `index` (from 0) of `SplitMix64` (Sebastiano Vigna, public domain,
//!   `prng.di.unimi.it/splitmix64.c`) started at state `seed`, as its top 53 bits scaled to `[0, 1)`. Integers only,
//!   wrapping at 2^64, so no float operation can differ between machines: the values are equal everywhere, not merely
//!   close.
//! - The value at an index never depends on which other indices are read, or on earlier reads.
//! - Distinct seeds below 2^32 give unrelated streams (a seed is one step of the stream, which the finaliser
//!   scrambles).

// SplitMix64's constants: the state step (2^64 divided by the golden ratio, rounded to odd) and the finaliser's two
// multipliers and shifts, as published
const STEP: u64 = 0x9E37_79B9_7F4A_7C15;
const FIRST_MULTIPLIER: u64 = 0xBF58_476D_1CE4_E5B9;
const SECOND_MULTIPLIER: u64 = 0x94D0_49BB_1331_11EB;
/// 64 - 53: the bits dropped, so that what is left fits a float's significand exactly.
const DROPPED_BITS: u32 = 11;
/// 2^-53.
const UNIT: f64 = 1.0 / 9_007_199_254_740_992.0;

/// The value at `index` of the stream of `seed`: an `f64` in `[0, 1)`.
///
/// ```
/// use bullterrier_control::hashing::uniform;
///
/// let draws: Vec<f64> = (0..3).map(|index| uniform(42, index)).collect();
/// assert!(draws.iter().all(|draw| (0.0..1.0).contains(draw)));
/// assert_ne!(draws[0], uniform(43, 0)); // another seed, another stream
/// ```
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "the value has 53 significant bits, which an f64 holds exactly"
)]
pub fn uniform(seed: u64, index: u64) -> f64 {
    let mut state = seed.wrapping_add(index.wrapping_add(1).wrapping_mul(STEP));
    state = (state ^ (state >> 30)).wrapping_mul(FIRST_MULTIPLIER);
    state = (state ^ (state >> 27)).wrapping_mul(SECOND_MULTIPLIER);
    state ^= state >> 31;
    (state >> DROPPED_BITS) as f64 * UNIT
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the values are promised to be equal everywhere, not merely close"
)]
mod tests {
    use proptest::prelude::*;

    use super::uniform;

    /// The value a published 64-bit output stands for: its top 53 bits over 2^53, worked out here with plain integer
    /// division and a float division, not with the module's constants.
    #[allow(clippy::cast_precision_loss, reason = "53 significant bits are exact in an f64")]
    fn value_of(output: u64) -> f64 {
        (output / 2048) as f64 / 9_007_199_254_740_992.0
    }

    // The first outputs of SplitMix64 from state 0, as published with the algorithm and in its common test vectors
    // (the first values `next()` returns in Vigna's C code for `x = 0`)
    const SEED_0: [u64; 4] = [
        0xE220_A839_7B1D_CDAF,
        0x6E78_9E6A_A1B9_65F4,
        0x06C4_5D18_8009_454F,
        0xF88B_B8A8_724C_81EC,
    ];
    // ... and from state 1234567, the vector the rand_core crate tests its SplitMix64 against
    const SEED_1234567: [u64; 5] = [
        6_457_827_717_110_365_317,
        3_203_168_211_198_807_973,
        9_817_491_932_198_370_423,
        4_593_380_528_125_082_431,
        16_408_922_859_458_223_821,
    ];

    #[test]
    fn the_published_splitmix64_outputs_are_reproduced_bit_for_bit() {
        for (seed, outputs) in [(0, &SEED_0[..]), (1_234_567, &SEED_1234567[..])] {
            for (index, &output) in (0..).zip(outputs) {
                assert_eq!(uniform(seed, index), value_of(output), "seed {seed} index {index}");
            }
        }
    }

    // outputs of Vigna's C algorithm written out in plain Python integers (an independent implementation), at the
    // start, an ordinary index, the far end of the 64-bit range, and the last index (where index + 1 wraps to 0)
    #[test]
    fn other_seeds_and_far_indices_take_the_reference_values() {
        for (seed, index, output) in [
            (7, 0, 0x63CB_E1E4_5932_0DD7),
            (7, 1, 0x044C_3CD7_F43C_661C),
            (7, 2, 0xE698_4080_BAB1_2A02),
            (7, 999, 0x9793_FB91_046F_FFEE),
            (7, 1 << 63, 0x63DC_0BBB_DDD4_0FA8),
            (7, u64::MAX, 0x12AE_3023_7B17_DF14),
            (4_294_967_295, 0, 0x73B1_3BA2_AFF1_81C0),
            (4_294_967_295, 1, 0x6120_4305_1340_D3B4),
            (4_294_967_295, 999, 0x511C_6D42_DB6B_0EF4),
            (4_294_967_295, 1 << 63, 0x0C8D_95B8_EB77_3EB5),
            (4_294_967_295, u64::MAX, 0x8B32_C408_E8C2_C97C),
        ] {
            assert_eq!(uniform(seed, index), value_of(output), "seed {seed} index {index}");
        }
    }

    #[test]
    fn different_seeds_give_different_values_at_each_of_a_thousand_indices() {
        for index in 0..1000 {
            assert_ne!(uniform(0, index), uniform(1, index), "index {index}");
        }
    }

    #[test]
    fn a_hundred_thousand_values_are_distinct_and_have_the_mean_and_variance_of_a_uniform_distribution() {
        let mut values: Vec<f64> = (0..100_000).map(|index| uniform(3, index)).collect();
        let count = 100_000.0;
        let mean = values.iter().sum::<f64>() / count;
        let variance = values.iter().map(|value| (value - mean) * (value - mean)).sum::<f64>() / count;
        // a standard error of the mean is 0.0009 here; of the variance about 0.0004 (six of them are allowed)
        assert!((mean - 0.5).abs() < 0.006, "{mean}");
        assert!((variance - 1.0 / 12.0).abs() < 0.003, "{variance}");
        values.sort_by(f64::total_cmp);
        values.dedup();
        assert_eq!(values.len(), 100_000);
    }

    /// The Pearson correlation of two equally long samples.
    fn correlation(first: &[f64], second: &[f64]) -> f64 {
        let n = f64::from(u32::try_from(first.len()).unwrap_or(u32::MAX));
        let (mean_first, mean_second) = (first.iter().sum::<f64>() / n, second.iter().sum::<f64>() / n);
        let (mut covariance, mut spread_first, mut spread_second) = (0.0, 0.0, 0.0);
        for (a, b) in first.iter().zip(second) {
            let (da, db) = (a - mean_first, b - mean_second);
            (covariance, spread_first, spread_second) =
                (covariance + da * db, spread_first + da * da, spread_second + db * db);
        }
        covariance / (spread_first * spread_second).sqrt()
    }

    #[test]
    fn neighbouring_seeds_give_uncorrelated_values() {
        let stream = |seed| (0..100_000).map(|index| uniform(seed, index)).collect::<Vec<f64>>();
        for (seed, other) in [(0, 1), (0, 2), (1, 4_294_967_295)] {
            let correlated = correlation(&stream(seed), &stream(other));
            // a standard error of 0.0032 for this many samples: six of them
            assert!(correlated.abs() < 0.019, "seeds {seed} and {other}: {correlated}");
        }
    }

    proptest! {
        #[test]
        fn a_value_is_in_zero_to_one_and_depends_on_its_arguments_alone(seed: u64, index: u64, other: u64) {
            let value = uniform(seed, index);
            prop_assert!((0.0..1.0).contains(&value), "{value}");
            // reading another index in between changes nothing
            let _ = uniform(seed, other);
            prop_assert_eq!(uniform(seed, index), value);
        }
    }
}
