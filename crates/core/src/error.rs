//! What can go wrong when the core runs a shaper.

use std::fmt;

use crate::formats::{Problem, write_problems};

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
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownShaper { name } => write!(formatter, "the Rust core has no shaper called {name:?}"),
            Self::Params { shaper, problems } => {
                write!(formatter, "the parameters of {shaper:?} do not fit: ")?;
                write_problems(formatter, problems, 0)
            }
        }
    }
}

impl std::error::Error for Error {}
