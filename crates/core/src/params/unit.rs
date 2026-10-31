//! What a parameter's number is measured in.

use std::fmt;
use std::str::FromStr;

use super::ParamError;

/// What a number parameter is measured in. Not the formats' [`Unit`](crate::formats::Unit), which says what a
/// series' values measure: the two share some names and nothing else. A value in `Beats` is turned into seconds through
/// a grid by whatever reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParamUnit {
    /// Seconds.
    Seconds,
    /// Beats of a grid.
    Beats,
    /// Hertz: times a second.
    Hertz,
    /// Cycles: turns of something periodic (a phase is a position in one).
    Cycles,
    /// A share of a whole, 0 to 1.
    Fraction,
    /// Decibels.
    Decibels,
    /// A ratio of two quantities, or a plain multiplier.
    Ratio,
    /// Semitones.
    Semitones,
    /// A count of things.
    Count,
}

impl ParamUnit {
    /// Every unit, in the order messages list them.
    pub const ALL: [Self; 9] = [
        Self::Seconds,
        Self::Beats,
        Self::Hertz,
        Self::Cycles,
        Self::Fraction,
        Self::Decibels,
        Self::Ratio,
        Self::Semitones,
        Self::Count,
    ];

    /// The unit's name, as a description spells it (`seconds`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Seconds => "seconds",
            Self::Beats => "beats",
            Self::Hertz => "hertz",
            Self::Cycles => "cycles",
            Self::Fraction => "fraction",
            Self::Decibels => "decibels",
            Self::Ratio => "ratio",
            Self::Semitones => "semitones",
            Self::Count => "count",
        }
    }

    /// Every unit's name, joined by `, `.
    pub(crate) fn listed() -> String {
        Self::ALL.map(Self::as_str).join(", ")
    }
}

impl fmt::Display for ParamUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ParamUnit {
    type Err = ParamError;

    /// The unit called `name`, or a [`ParamError`] at the path `unit` listing the units there are.
    fn from_str(name: &str) -> Result<Self, ParamError> {
        Self::ALL.into_iter().find(|unit| unit.as_str() == name).ok_or_else(|| {
            ParamError::at(
                "unit",
                format!("unknown unit {name:?}: expected one of {}", Self::listed()),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ParamUnit;

    #[test]
    fn every_unit_is_read_back_from_its_name_and_no_other_name_is_a_unit() {
        let names = [
            "seconds",
            "beats",
            "hertz",
            "cycles",
            "fraction",
            "decibels",
            "ratio",
            "semitones",
            "count",
        ];
        assert_eq!(ParamUnit::ALL.map(ParamUnit::as_str), names);
        for unit in ParamUnit::ALL {
            assert_eq!(unit.as_str().parse::<ParamUnit>(), Ok(unit));
            assert_eq!(unit.to_string(), unit.as_str());
        }
        for wrong in ["percent", "Seconds", "midi", ""] {
            let refused = wrong.parse::<ParamUnit>();
            assert!(
                matches!(&refused, Err(error) if error.problems[0].path == "unit" && error.to_string().contains("expected one of seconds, beats")),
                "{wrong}: {refused:?}"
            );
        }
    }
}
