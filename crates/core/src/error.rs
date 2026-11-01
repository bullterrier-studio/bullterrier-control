//! What can go wrong when the core reads a signal or runs a shaper.

use std::fmt;
use std::sync::Arc;

use crate::formats::{FormatError, Problem, write_problems};
use crate::params::ParamError;

/// Why a signal could not be read, a shaper could not run, a built-in's parameters or a source's inputs could not be
/// given, or a thing of the [`registry`](crate::registry) could not be registered, found or called. Each but
/// [`Error::Node`], [`Error::Input`] and [`Error::UnknownName`] is a programming error of the caller, never a user's
/// mistake: the Python package validates whatever else a user gives before it reaches the core.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Error {
    /// A text names no easing curve; the message lists the valid forms.
    Ease {
        /// What is wrong with the text and what would be right.
        message: String,
    },
    /// A signal cannot be read as asked: a time or a value that is not a finite number, or one too far for a position to
    /// be a number.
    Signal {
        /// What cannot be read and why, naming the time where there is one.
        message: String,
    },
    /// The parameters are not the thing's: one is unknown, of the wrong type or out of its range.
    Params {
        /// The name of the thing whose parameters these were meant to be.
        shaper: String,
        /// What is wrong with them: each broken rule under the name of its field, the first 100 at most.
        problems: Vec<Problem>,
        /// How many more problems there were than `problems` holds.
        omitted: usize,
    },
    /// The data given as an input ([`inputs`](crate::inputs): keyframes, events, a grid, values, a series) breaks
    /// its rules: a mistake in the data, which the bindings report as the user's.
    Input {
        /// Each broken rule under its path from the input's name (`points[1].ease`), the first 100 at most.
        problems: Vec<Problem>,
        /// How many more problems there were than `problems` holds.
        omitted: usize,
    },
    /// Data for a signal breaks the rules of its format (the values of `Signal::from_values` do not make a series):
    /// the problems are the [`source`](std::error::Error::source)'s, and the message is its message.
    Format {
        /// The rules broken, each naming its field.
        source: FormatError,
    },
    /// A callback of a signal (`Signal::from_fn`) failed with an error that is not a core [`Error`] (that one is
    /// returned as it is): its error is the [`source`](std::error::Error::source), and the message is its message.
    Node {
        /// The callback's own error.
        source: Arc<dyn std::error::Error + Send + Sync>,
    },
    /// A signal would be deeper than [`MAX_DEPTH`](crate::MAX_DEPTH) nodes, or callbacks (a `Signal::from_fn`, a
    /// recurrence, a thing's body) would nest deeper than a thread's stack is sure to hold, each reading a signal or
    /// calling a thing whose own callback does (the message says how deep they may). It is the shape of what the caller built, not a wrong argument: an operator that makes
    /// a graph too deep reports this on every read.
    Depth {
        /// How deep the graph would be and what to do instead.
        message: String,
    },
    /// A thing cannot be registered: its declaration breaks a rule of the [`registry`](crate::registry), or its name
    /// is taken by another thing.
    Registration {
        /// The thing, by kind and name, and the rule it breaks.
        message: String,
    },
    /// No thing (or, asked for a built-in's parameters, no built-in) of this name: the one error for a name not
    /// known, wherever it is asked for.
    UnknownName {
        /// The name asked for.
        name: String,
        /// The names that are registered, sorted.
        available: Vec<String>,
    },
    /// A thing was called as it cannot be: with another number of signals than its kind takes, an input it does not
    /// declare or of another kind than its name says, or without an input it requires; or it gave another kind of
    /// result than it declares.
    Call {
        /// The thing, by kind and name, and what was wrong with the call.
        message: String,
    },
    /// A thing's body gave a result of its kind that breaks what such a result promises: events out of time order.
    Output {
        /// The thing, by kind and name, and what is wrong with what it gave.
        message: String,
    },
}

impl Error {
    /// The error a callback (`Signal::from_fn`, `retimed`, `rendered`) failed with, as its reader gets it: a core
    /// `Error` as it is, so a callback can refuse with the kind it means; any other error as the source of an
    /// [`Error::Node`].
    pub(crate) fn from_callback<E: std::error::Error + Send + Sync + 'static>(failure: E) -> Self {
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(failure);
        match boxed.downcast::<Self>() {
            Ok(core) => *core,
            Err(other) => Self::Node {
                source: Arc::from(other),
            },
        }
    }

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
            Self::Ease { message }
            | Self::Signal { message }
            | Self::Depth { message }
            | Self::Registration { message }
            | Self::Call { message }
            | Self::Output { message } => formatter.write_str(message),
            Self::UnknownName { name, available } => write!(
                formatter,
                "unknown source, shaper or operation {}; available: {}",
                crate::registry::quoted(name),
                available.join(", ")
            ),
            Self::Params {
                shaper,
                problems,
                omitted,
            } => {
                write!(formatter, "the parameters of {shaper:?} do not fit: ")?;
                write_problems(formatter, problems, *omitted)
            }
            Self::Input { problems, omitted } => write_problems(formatter, problems, *omitted),
            Self::Format { source } => write!(formatter, "{source}"),
            Self::Node { source } => write!(formatter, "{source}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Format { source } => Some(source),
            Self::Node { source } => Some(source.as_ref()),
            _ => None,
        }
    }
}

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
