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
