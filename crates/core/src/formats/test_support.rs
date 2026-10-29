//! Test helpers shared by the formats' tests.

use super::error::FormatError;

/// The problems wanted: each a path and a short part of its rule.
pub(crate) type Expected = &'static [(&'static str, &'static str)];

/// Assert that `result` is accepted when `expected` is empty, else refused with exactly those paths, in that order,
/// each rule containing the given part.
pub(crate) fn check_against<T: std::fmt::Debug>(id: &str, result: Result<T, FormatError>, expected: Expected) {
    match result {
        Ok(value) => assert!(expected.is_empty(), "{id}: accepted {value:?}, wanted {expected:?}"),
        Err(error) => {
            assert!(!expected.is_empty(), "{id}: refused with {error}");
            let got: Vec<(&str, &str)> = error
                .problems
                .iter()
                .map(|p| (p.path.as_str(), p.rule.as_str()))
                .collect();
            let paths: Vec<&str> = got.iter().map(|(path, _)| *path).collect();
            let wanted: Vec<&str> = expected.iter().map(|(path, _)| *path).collect();
            assert_eq!(paths, wanted, "{id}: {error}");
            for ((path, rule), (_, part)) in got.iter().zip(expected) {
                assert!(rule.contains(part), "{id}: {path}: {rule:?} lacks {part:?}");
            }
        }
    }
}
