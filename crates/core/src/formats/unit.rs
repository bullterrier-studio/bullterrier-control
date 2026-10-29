//! What a [`Series`](super::Series)' values measure.

use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::error::FormatError;

/// What a series' values measure. `Fraction` values lie in 0..=1 and the series says how that was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Unit {
    /// Values in 0..=1; the series' `scaling` says how that range was reached.
    Fraction,
    /// Decibels.
    Decibels,
    /// Hertz.
    Hertz,
    /// MIDI note numbers (fractions allowed).
    Midi,
    /// A ratio of two quantities.
    Ratio,
    /// Seconds.
    Seconds,
    /// Beats.
    Beats,
}

impl Unit {
    /// Every unit, in the order `docs/formats.md` lists them.
    pub const ALL: [Self; 7] = [
        Self::Fraction,
        Self::Decibels,
        Self::Hertz,
        Self::Midi,
        Self::Ratio,
        Self::Seconds,
        Self::Beats,
    ];

    /// The name a file spells the unit with.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fraction => "fraction",
            Self::Decibels => "decibels",
            Self::Hertz => "hertz",
            Self::Midi => "midi",
            Self::Ratio => "ratio",
            Self::Seconds => "seconds",
            Self::Beats => "beats",
        }
    }

    /// The unit called `name`, or the reason there is none.
    fn named(name: &str) -> Result<Self, String> {
        Self::ALL.into_iter().find(|unit| unit.as_str() == name).ok_or_else(|| {
            let known: Vec<&str> = Self::ALL.iter().map(|unit| unit.as_str()).collect();
            format!("unknown unit {name:?}: expected one of {}", known.join(", "))
        })
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Unit {
    type Err = FormatError;

    fn from_str(name: &str) -> Result<Self, FormatError> {
        Self::named(name).map_err(|rule| FormatError::at("unit", rule))
    }
}

impl Serialize for Unit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Unit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::named(&name).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_units_are_the_seven_of_the_format_in_its_order() {
        let names: Vec<&str> = Unit::ALL.iter().map(|unit| unit.as_str()).collect();
        assert_eq!(
            names,
            ["fraction", "decibels", "hertz", "midi", "ratio", "seconds", "beats"]
        );
    }

    #[test]
    fn every_unit_survives_its_name_and_its_json() {
        for unit in Unit::ALL {
            assert_eq!(unit.to_string().parse::<Unit>(), Ok(unit), "{unit}");
            assert_eq!(serde_json::to_value(unit).ok(), Some(json!(unit.as_str())), "{unit}");
            assert_eq!(
                serde_json::from_value::<Unit>(json!(unit.as_str())).ok(),
                Some(unit),
                "{unit}"
            );
        }
    }

    #[test]
    fn only_the_exact_lowercase_names_are_units() {
        for (name, ok) in [
            ("hertz", true),
            ("Hertz", false),
            ("HERTZ", false),
            (" hertz", false),
            ("hz", false),
            ("", false),
            ("percent", false),
        ] {
            let parsed = name.parse::<Unit>();
            assert_eq!(parsed.is_ok(), ok, "{name:?}: {parsed:?}");
            if let Err(error) = parsed {
                assert_eq!(error.problems.len(), 1, "{name:?}");
                assert_eq!(error.problems[0].path, "unit", "{name:?}");
                assert!(
                    error.problems[0].rule.contains("fraction, decibels"),
                    "{name:?}: {error}"
                );
            }
        }
    }

    #[test]
    fn json_that_is_not_a_unit_name_is_refused() {
        for bad in [json!("Hertz"), json!(5), json!(null), json!(true), json!(["hertz"])] {
            assert!(serde_json::from_value::<Unit>(bad.clone()).is_err(), "{bad}");
        }
    }
}
