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

/// The data does not follow the format: every rule it breaks, in field order. Never empty.
///
/// Each problem is a mistake in the data the caller gave (a file's content, a call's arguments), not a bug in the core.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FormatError {
    /// The rules broken, in the order of the fields they are about.
    pub problems: Vec<Problem>,
}

impl FormatError {
    pub(crate) fn new(problems: Vec<Problem>) -> Self {
        Self { problems }
    }

    /// An error with the single rule `rule` broken at `path`.
    pub(crate) fn at(path: &str, rule: impl Into<String>) -> Self {
        Self::new(vec![Problem {
            path: path.to_owned(),
            rule: rule.into(),
        }])
    }

    /// The error a deserializer stopped with: one problem, at the path `serde_path_to_error` followed.
    pub(crate) fn from_serde<E: fmt::Display>(error: &serde_path_to_error::Error<E>) -> Self {
        let message = error.inner().to_string();
        let path = error.path().to_string();
        // serde follows a path to the value that failed to read; a repeated key fails at the object holding it, so the
        // key is taken from the message
        let repeated = message
            .strip_prefix("duplicate field `")
            .and_then(|rest| rest.split('`').next())
            .map(str::to_owned);
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
        Ok(())
    }
}

impl std::error::Error for FormatError {}
