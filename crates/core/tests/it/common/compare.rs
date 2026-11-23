//! The one comparison of signal values across routes (two doors, two read orders, a saved patch against the graph).
//!
//! Two values are the same when they differ by at most `TOLERANCE` of the larger of 1 and their size. A billionth is far
//! below anything a signal can be heard or seen to drive (a 16-bit sample step is 1.5e-5, an 8-bit colour step 4e-3 of
//! 0..1), yet far above the last-bit noise two routes may differ by (a fused multiply-add, one `libm` call).
//! The wasm tests repeat this number in `tests/wasm/package.mjs`.

/// How far apart two values of one signal may be, relative to `max(1, |value|)`.
pub(crate) const TOLERANCE: f64 = 1e-9;

fn close(expected: f64, actual: f64) -> bool {
    if expected.is_nan() || actual.is_nan() {
        return expected.is_nan() && actual.is_nan();
    }
    expected.to_bits() == actual.to_bits()
        || (expected.is_finite()
            && actual.is_finite()
            && (expected - actual).abs() <= TOLERANCE * expected.abs().max(actual.abs()).max(1.0))
}

/// The index of the first value where `actual` is not `expected` within the tolerance, or `None` if they agree. A
/// different length is a difference at the shorter length. Compared by index, so a mismatch never builds a diff of
/// the whole array.
pub(crate) fn first_difference(expected: &[f64], actual: &[f64]) -> Option<usize> {
    if expected.len() != actual.len() {
        return Some(expected.len().min(actual.len()));
    }
    expected.iter().zip(actual).position(|(e, a)| !close(*e, *a))
}

/// Two reads of a signal, each its values or the text of its refusal: the refusals are equal, the values agree.
pub(crate) fn same_reads(expected: &Result<Vec<f64>, String>, actual: &Result<Vec<f64>, String>) -> Option<usize> {
    match (expected, actual) {
        (Ok(e), Ok(a)) => first_difference(e, a),
        _ => (expected != actual).then_some(0),
    }
}
