//! What can go wrong when the core runs a shaper.

use std::fmt;

use crate::formats::{Problem, write_problems};
use crate::params::ParamError;

/// Why a shaper could not run, or a built-in's parameters could not be given. Each is a programming error of the
/// caller, never a user's mistake: the Python package validates whatever a user gives before it reaches the core.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The core has no shaper (or, asked for its parameters, no built-in) of this name.
    UnknownShaper {
        /// The name asked for.
        name: String,
    },
    /// The parameters are not the built-in's: one is unknown, of the wrong type or out of its range.
    Params {
        /// The built-in whose parameters these were meant to be.
        shaper: String,
        /// What is wrong with them: each broken rule under the name of its field, the first 100 at most.
        problems: Vec<Problem>,
        /// How many more problems there were than `problems` holds.
        omitted: usize,
    },
}

impl Error {
    /// The error for `shaper` whose parameters broke the rules `error` lists.
    pub(crate) fn params(shaper: &str, error: ParamError) -> Self {
        let omitted = error.omitted();
        Self::Params {
            shaper: shaper.to_owned(),
            problems: error.problems,
            omitted,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownShaper { name } => write!(formatter, "the Rust core has no shaper called {name:?}"),
            Self::Params {
                shaper,
                problems,
                omitted,
            } => {
                write!(formatter, "the parameters of {shaper:?} do not fit: ")?;
                write_problems(formatter, problems, *omitted)
            }
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Error;
    use crate::params::{Kind, Param, ParamTable};

    #[test]
    fn a_broken_declaration_reports_its_exact_total_too() -> Result<(), String> {
        // 150 fields without a description: one broken rule each
        let fields: Vec<Param> = (0..150)
            .map(|index| Param {
                name: format!("field_{index:03}"),
                kind: Kind::Text,
                default: json!("x"),
                doc: String::new(),
                unit: None,
                min: None,
                max: None,
                advanced: false,
                cyclic: false,
            })
            .collect();
        let declaration = ParamTable::new(fields)
            .err()
            .ok_or_else(|| "accepted fields without a description".to_owned())?;
        let text = Error::params("odd", declaration).to_string();
        assert_eq!(text.matches("needs a description").count(), 100, "{text}");
        assert!(text.ends_with("; ... and 50 more"), "{text}");
        Ok(())
    }
}
