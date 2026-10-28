//! What can go wrong when the core runs a shaper.

use std::fmt;

/// Why a shaper could not run. Each is a programming error of the caller, never a user's mistake: the Python package
/// validates whatever a user gives before it reaches the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The core implements no shaper of this name.
    UnknownShaper {
        /// The name asked for.
        name: String,
    },
    /// The parameters are not the shaper's: one is missing, unknown, or of the wrong type.
    Params {
        /// The shaper whose parameters these were meant to be.
        shaper: String,
        /// What is wrong with them, naming the field.
        message: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownShaper { name } => write!(formatter, "the Rust core has no shaper called {name:?}"),
            Self::Params { shaper, message } => {
                write!(formatter, "the parameters of {shaper:?} do not fit: {message}")
            }
        }
    }
}

impl std::error::Error for Error {}
