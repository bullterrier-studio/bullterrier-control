//! What can be wrong with the data of a format.

use std::fmt;

/// One broken rule: where it is and what the rule says, with the offending value shown as JSON
/// (`"must be >= 0 (got -0.1)"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The field the rule is about, as the file spells it: `t`, `values[2]`, or empty for the whole document.
    pub path: String,
    /// The rule that is broken, with the value that breaks it where it is a value.
    pub rule: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            formatter.write_str(&self.rule)
        } else {
            write!(formatter, "{}: {}", self.path, self.rule)
        }
    }
}

/// How many problems a [`FormatError`] keeps; the rest are only counted ([`FormatError::omitted`]), so data with millions
/// of broken values is refused in a bounded amount of memory.
const MAX_PROBLEMS: usize = 100;

/// The problems found while checking data, kept up to `MAX_PROBLEMS`: the first ones are stored, the others only counted
/// (nothing is allocated for them). A format collects its problems here, and [`Problems::into_error`] turns them into the
/// [`FormatError`] to return.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Problems {
    stored: Vec<Problem>,
    omitted: usize,
}

impl Problems {
    /// No problems yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `problem`: kept while there is room, else only counted.
    pub fn push(&mut self, problem: Problem) {
        self.push_with(|| problem);
    }

    /// Record the problem `make` builds, which is not called (nothing is allocated) once the list is full.
    pub(crate) fn push_with(&mut self, make: impl FnOnce() -> Problem) {
        if self.stored.len() < MAX_PROBLEMS {
            self.stored.push(make());
        } else {
            self.omitted += 1;
        }
    }

    /// Record all the problems of `error`, kept or not, after those already here.
    pub fn absorb(&mut self, error: &FormatError) {
        for problem in &error.problems {
            self.push(problem.clone());
        }
        self.omitted += error.omitted;
    }

    /// How many problems were recorded, kept or not.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stored.len() + self.omitted
    }

    /// Whether no problem was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The error for the problems recorded, or `None` when there is none.
    #[must_use]
    pub fn into_error(self) -> Option<FormatError> {
        (!self.is_empty()).then(|| FormatError::new(self))
    }
}

/// The data does not follow the format: the rules it breaks, in field order. Never empty.
///
/// At most 100 are kept in [`problems`](Self::problems) (the first ones); [`omitted`](Self::omitted) counts the rest.
/// Each problem is a mistake in the data the caller gave (a file's content, a call's arguments), not a bug in the core.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FormatError {
    /// The rules broken, in the order of the fields they are about: the first 100 at most.
    pub problems: Vec<Problem>,
    omitted: usize,
}

impl FormatError {
    /// How many more problems there were than [`problems`](Self::problems) holds.
    #[must_use]
    pub fn omitted(&self) -> usize {
        self.omitted
    }

    /// The error for `problems`, which hold at least one.
    pub(crate) fn new(problems: Problems) -> Self {
        Self {
            problems: problems.stored,
            omitted: problems.omitted,
        }
    }

    /// An error with the single rule `rule` broken at `path`.
    pub(crate) fn at(path: &str, rule: impl Into<String>) -> Self {
        let mut problems = Problems::new();
        problems.push(Problem {
            path: path.to_owned(),
            rule: rule.into(),
        });
        Self::new(problems)
    }

    /// The error a deserializer stopped with: one problem, at the path `serde_path_to_error` followed.
    pub(crate) fn from_serde<E: fmt::Display>(error: &serde_path_to_error::Error<E>) -> Self {
        let message = error.inner().to_string();
        let path = error.path().to_string();
        // serde follows a path to the value that failed to read; a repeated key fails at the object holding it, so the
        // key is taken from the message
        let repeated = message
            .strip_prefix("duplicate field `")
            // the key may hold a backtick itself: it ends at the last one (a JSON reader adds " at line .. column .." after)
            .and_then(|rest| rest.rfind('`').map(|end| rest[..end].to_owned()));
        match (repeated, path.as_str()) {
            (Some(key), ".") => Self::at(&key, message),
            (Some(key), parent) => Self::at(&format!("{parent}.{key}"), message),
            // a text that is not JSON fails before any field is reached: no path is known
            (None, "." | "?") => Self::at("", message),
            (None, _) => Self::at(&path, message),
        }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, problem) in self.problems.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            write!(formatter, "{problem}")?;
        }
        match self.omitted {
            0 => Ok(()),
            omitted => write!(formatter, "; ... and {omitted} more"),
        }
    }
}

impl std::error::Error for FormatError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(index: usize) -> Problem {
        Problem {
            path: format!("values[{index}]"),
            rule: "is wrong".to_owned(),
        }
    }

    fn collected(count: usize) -> Problems {
        let mut problems = Problems::new();
        for index in 0..count {
            problems.push(problem(index));
        }
        problems
    }

    #[test]
    fn nothing_recorded_is_no_error() {
        assert!(Problems::new().is_empty());
        assert_eq!(Problems::new().into_error(), None);
    }

    #[test]
    fn up_to_the_cap_every_problem_is_kept_and_listed() {
        for count in [1, 3, 100] {
            assert_eq!(collected(count).len(), count);
            let Some(error) = collected(count).into_error() else {
                continue;
            };
            assert_eq!(error.problems.len(), count);
            assert_eq!(error.omitted(), 0, "{count}");
            assert!(!error.to_string().contains("more"), "{count}: {error}");
        }
        let Some(error) = collected(3).into_error() else {
            return;
        };
        assert_eq!(
            error.to_string(),
            "values[0]: is wrong; values[1]: is wrong; values[2]: is wrong"
        );
    }

    #[test]
    fn past_the_cap_the_first_problems_are_kept_and_the_rest_counted() {
        for (count, omitted) in [(101, 1), (150, 50), (1_000, 900)] {
            let problems = collected(count);
            assert_eq!(problems.len(), count);
            let Some(error) = problems.into_error() else {
                continue;
            };
            assert_eq!(error.problems, (0..100).map(problem).collect::<Vec<_>>());
            assert_eq!(error.omitted(), omitted);
            assert!(
                error.to_string().ends_with(&format!("; ... and {omitted} more")),
                "{error}"
            );
        }
    }

    #[test]
    fn a_problem_that_is_not_kept_is_not_even_built() {
        let mut problems = collected(100);
        problems.push_with(|| Problem {
            path: "never built".to_owned(),
            rule: String::new(),
        });
        let mut built = false;
        problems.push_with(|| {
            built = true;
            problem(0)
        });
        assert!(!built);
        assert_eq!(problems.len(), 102);
    }

    #[test]
    fn an_error_absorbed_adds_what_it_kept_and_what_it_omitted() {
        let mut inner = collected(250);
        inner.push(problem(250));
        assert_eq!(inner.len(), 251);
        let Some(inner) = inner.into_error() else {
            return;
        };
        let mut problems = collected(10);
        problems.absorb(&inner);
        assert_eq!(problems.len(), 10 + 251);
        let Some(error) = problems.into_error() else {
            return;
        };
        assert_eq!(error.problems.len(), 100);
        assert_eq!(error.omitted(), 161);
    }
}
