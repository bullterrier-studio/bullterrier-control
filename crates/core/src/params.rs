//! What a source, shaper or operation takes: its parameters, declared once as a [`ParamTable`] and checked there.
//!
//! In Rust a table is declared by deriving [`Params`] on a struct; anything else (the Python package, for a plugin's
//! parameters) builds one from a list of [`Param`]s. Either way the table holds every rule:
//! - **Declaration** ([`ParamTable::new`], the one place these rules live): a field is a number (a decimal or a whole
//!   number), a switch, text or a choice of names ([`Kind`]); it has a valid default and a description; a number has a
//!   unit ([`ParamUnit`]) and a finite range (`min` and `max`) that holds its default; any other field takes no unit
//!   or range; a choice offers at least one name, each once; a cyclic field (a phase) is a decimal in cycles whose
//!   range is one cycle wide (`max == min + 1`); every field has a name, no two share one, and none is named after a
//!   unit or with a discouraged name (`strength`, `intensity`, `factor`, `val`, `k`: say what it is instead).
//! - **Values** ([`ParamTable::parse`]): an object of values by name, read strictly; a field left out takes its
//!   default; every broken rule is listed, the first 100 kept ([`ParamError`]).
//! - **Description** ([`ParamTable::describe`]): the fields as a consumer reads them, in the shape of a JSON Schema's
//!   `properties`.
//!
//! ```
//! use bullterrier_control::{Choice, Params};
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Choice)]
//! enum Shape {
//!     Sine,
//!     #[choice(name = "saw-down")]
//!     SawDown,
//! }
//!
//! #[derive(Clone, Debug, PartialEq, Params)]
//! struct Wave {
//!     /// How far the wave moves the value.
//!     #[param(default = 0.5, unit = Fraction, min = 0, max = 1)]
//!     depth: f64,
//!     /// The wave: 'sine' or 'saw-down'.
//!     #[param(default = Shape::Sine)]
//!     shape: Shape,
//!     /// Where in the cycle it starts.
//!     #[param(default = 0.0, unit = Cycles, min = 0, max = 1, advanced, cyclic)]
//!     phase: f64,
//! }
//!
//! # fn main() -> Result<(), bullterrier_control::params::ParamError> {
//! let wave = Wave::parse(&serde_json::json!({"shape": "saw-down", "phase": 1.25}))?;
//! assert_eq!(wave, Wave { depth: 0.5, shape: Shape::SawDown, phase: 0.25 });
//! assert!(Wave::parse(&serde_json::json!({"depth": 2})).is_err());
//! # Ok(())
//! # }
//! ```

mod check;
mod table;
mod unit;

use std::fmt;

pub use bullterrier_control_macros::{Choice, Params};
pub use table::{Kind, Param, ParamTable, Rule, Values};
pub use unit::ParamUnit;

use crate::formats::{Problem, Problems, write_problems};

/// A parameter declaration or a set of values breaks the table's rules: each broken rule as a [`Problem`] whose path
/// is the field's name (empty for the whole object). Never empty.
///
/// At most 100 are kept in [`problems`](Self::problems) (the first ones); [`omitted`](Self::omitted) counts the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParamError {
    /// The rules broken, in the order of the fields they are about (unknown fields last): the first 100 at most.
    pub problems: Vec<Problem>,
    omitted: usize,
}

impl ParamError {
    /// How many more problems there were than [`problems`](Self::problems) holds.
    #[must_use]
    pub fn omitted(&self) -> usize {
        self.omitted
    }

    /// The error for `problems`, or `None` when there is none.
    pub(crate) fn from_problems(problems: Problems) -> Option<Self> {
        (!problems.is_empty()).then(|| {
            let (problems, omitted) = problems.into_parts();
            Self { problems, omitted }
        })
    }

    /// The error with the single rule `rule` broken at `path`.
    pub(crate) fn at(path: &str, rule: String) -> Self {
        Self {
            problems: vec![Problem {
                path: path.to_owned(),
                rule,
            }],
            omitted: 0,
        }
    }
}

impl fmt::Display for ParamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_problems(formatter, &self.problems, self.omitted)
    }
}

impl std::error::Error for ParamError {}

/// A struct of parameters with its table: what `#[derive(Params)]` writes (with `Default`, from each field's
/// `default`). Each field is `#[param(default = .., unit = .., min = .., max = .., advanced, cyclic)]` (a number needs
/// `unit`, `min` and `max`; a unit is a [`ParamUnit`] variant) and its doc comment is the field's description; its
/// type is a [`ParamType`]. A rule between fields is `#[param(rule = path)]` on the struct: `path` is a
/// `fn(&Self) -> Vec<Problem>` that [`ParamTable::parse`] runs once every field is valid (see [`ParamTable::with_rule`]).
pub trait Params: Default + Clone + fmt::Debug + PartialEq {
    /// The table of the struct's fields, built on the first call and shared after.
    ///
    /// # Errors
    /// The [`ParamError`] of [`ParamTable::new`] if the declaration breaks a rule: every call returns it, so a test
    /// that reads the table (or parses once) catches it before any value is given.
    fn table() -> Result<&'static ParamTable, ParamError>;

    /// The struct holding `values`, which [`table`](Self::table)`.parse` returned. Infallible: a field `values` does
    /// not hold as its kind (values of another table) takes its default, so the result is always valid.
    #[must_use]
    fn from_values(values: &Values) -> Self;

    /// The struct for `given`, an object of values by name, read by the table's [`ParamTable::parse`].
    ///
    /// # Errors
    /// A [`ParamError`] listing every rule `given` breaks, or the declaration's own error (see
    /// [`table`](Self::table)).
    fn parse(given: &serde_json::Value) -> Result<Self, ParamError> {
        Ok(Self::from_values(&Self::table()?.parse(given)?))
    }

    /// The struct's values as an object by field name, each in the form [`ParamType::to_json`] gives: what
    /// [`parse`](Self::parse) reads back.
    ///
    /// ```
    /// use bullterrier_control::Params;
    /// use bullterrier_control::builtin::slew;
    ///
    /// let value = slew::Params { rise: 0.5, ..Default::default() }.to_value();
    /// assert_eq!(value, serde_json::json!({"rise": 0.5, "fall": 0.1, "rate": 1000.0}));
    /// ```
    #[must_use]
    fn to_value(&self) -> serde_json::Value;

    /// The struct as its table reads its values ([`parse`](Self::parse) of [`to_value`](Self::to_value)): the same
    /// struct, a cyclic field wrapped into its cycle, if every field and the rule between them hold. A struct built in
    /// code is checked by the same rules as values given by name.
    ///
    /// ```
    /// use bullterrier_control::Params;
    /// use bullterrier_control::builtin::slew;
    ///
    /// let fine = slew::Params { rise: 0.5, ..Default::default() };
    /// assert_eq!(fine.check(), Ok(fine));
    /// let refused = slew::Params { rise: -1.0, ..Default::default() }.check();
    /// assert!(refused.is_err_and(|error| error.problems[0].path == "rise"));
    /// ```
    ///
    /// # Errors
    /// A [`ParamError`] listing every rule the values break (a value that is not finite is not a number), or the
    /// declaration's own error.
    fn check(&self) -> Result<Self, ParamError> {
        Self::parse(&self.to_value())
    }
}

/// An enum whose unit variants are the choices of a parameter, by name: what `#[derive(Choice)]` writes. A variant is
/// named in snake case (`SawDown` is `saw_down`) unless `#[choice(name = "saw-down")]` names it.
pub trait Choice: Sized + 'static {
    /// Every choice's name, in declaration order.
    const NAMES: &'static [&'static str];

    /// The choice called `name`, if there is one.
    fn from_name(name: &str) -> Option<Self>;

    /// The name of this choice: one of [`NAMES`](Self::NAMES).
    fn name(&self) -> &'static str;
}

/// A Rust type a parameter field may have, with the [`Kind`] it is: `f64` a decimal, `i64` a whole number, `bool` a
/// switch, `String` text, and any [`Choice`] a choice.
pub trait ParamType: Sized {
    /// The kind of field a value of this type is.
    fn kind() -> Kind;

    /// The value as JSON: the form [`Param::default`] holds.
    fn to_json(&self) -> serde_json::Value;

    /// The value a JSON value of this kind holds, as [`ParamTable::parse`] leaves it; `None` for any other.
    fn from_json(value: &serde_json::Value) -> Option<Self>;
}

impl ParamType for f64 {
    fn kind() -> Kind {
        Kind::Float
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::Value::from(*self)
    }

    fn from_json(value: &serde_json::Value) -> Option<Self> {
        value.as_f64()
    }
}

impl ParamType for i64 {
    fn kind() -> Kind {
        Kind::Whole
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::Value::from(*self)
    }

    fn from_json(value: &serde_json::Value) -> Option<Self> {
        value.as_i64()
    }
}

impl ParamType for bool {
    fn kind() -> Kind {
        Kind::Switch
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::Value::from(*self)
    }

    fn from_json(value: &serde_json::Value) -> Option<Self> {
        value.as_bool()
    }
}

impl ParamType for String {
    fn kind() -> Kind {
        Kind::Text
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::Value::from(self.as_str())
    }

    fn from_json(value: &serde_json::Value) -> Option<Self> {
        value.as_str().map(str::to_owned)
    }
}

impl<T: Choice> ParamType for T {
    fn kind() -> Kind {
        Kind::Choice(T::NAMES.iter().map(|name| (*name).to_owned()).collect())
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::Value::from(self.name())
    }

    fn from_json(value: &serde_json::Value) -> Option<Self> {
        value.as_str().and_then(T::from_name)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Choice, Kind, Param, ParamError, ParamTable, ParamUnit, Params};
    use crate::formats::check_problems;

    #[derive(Clone, Copy, Debug, PartialEq, Choice)]
    enum Shape {
        Sine,
        SawUp,
        #[choice(name = "saw-down")]
        SawDown,
    }

    #[derive(Clone, Debug, PartialEq, Params)]
    struct Sample {
        /// How far it moves,
        /// over two lines.
        #[param(default = 0.25, unit = Fraction, min = 0, max = 1)]
        depth: f64,
        /// Which noise.
        #[param(default = 7, unit = Count, min = 0, max = 4294967295)]
        seed: i64,
        /// The wave.
        ///
        /// A second paragraph.
        #[param(default = Shape::SawUp)]
        shape: Shape,
        /// Whether it runs.
        #[param(default = true, advanced)]
        on: bool,
        /// A label.
        #[param(default = "kick")]
        label: String,
        /// Where it starts.
        #[param(default = 0.0, unit = Cycles, min = -0.5, max = 0.5, cyclic)]
        centre: f64,
    }

    #[derive(Clone, Debug, PartialEq, Params)]
    struct Nothing;

    #[derive(Clone, Debug, PartialEq, Params)]
    struct Keyword {
        /// The kind of wave.
        #[param(default = "sine")]
        r#type: String,
    }

    #[derive(Clone, Debug, PartialEq, Params)]
    struct Broken {
        #[param(default = 2.0, unit = Ratio, min = 0, max = 1)]
        strength: f64,
    }

    fn declared(name: &str, kind: Kind, default: serde_json::Value, doc: &str) -> Param {
        Param {
            name: name.to_owned(),
            kind,
            default,
            doc: doc.to_owned(),
            unit: None,
            min: None,
            max: None,
            advanced: false,
            cyclic: false,
        }
    }

    #[test]
    fn a_derived_default_holds_each_declared_default() {
        assert_eq!(
            Sample::default(),
            Sample {
                depth: 0.25,
                seed: 7,
                shape: Shape::SawUp,
                on: true,
                label: "kick".to_owned(),
                centre: 0.0,
            }
        );
    }

    #[test]
    fn a_derived_table_declares_each_field_from_its_attribute_and_doc_comment() -> Result<(), ParamError> {
        let names = ["sine", "saw_up", "saw-down"].map(str::to_owned).to_vec();
        let expected = vec![
            Param {
                unit: Some(ParamUnit::Fraction),
                min: Some(0.0),
                max: Some(1.0),
                ..declared("depth", Kind::Float, json!(0.25), "How far it moves, over two lines.")
            },
            Param {
                unit: Some(ParamUnit::Count),
                min: Some(0.0),
                max: Some(4_294_967_295.0),
                ..declared("seed", Kind::Whole, json!(7), "Which noise.")
            },
            declared(
                "shape",
                Kind::Choice(names),
                json!("saw_up"),
                "The wave.\n\nA second paragraph.",
            ),
            Param {
                advanced: true,
                ..declared("on", Kind::Switch, json!(true), "Whether it runs.")
            },
            declared("label", Kind::Text, json!("kick"), "A label."),
            Param {
                unit: Some(ParamUnit::Cycles),
                min: Some(-0.5),
                max: Some(0.5),
                cyclic: true,
                ..declared("centre", Kind::Float, json!(0.0), "Where it starts.")
            },
        ];
        assert_eq!(Sample::table()?.params(), expected.as_slice());
        assert!(std::ptr::eq(Sample::table()?, Sample::table()?), "built once");
        Ok(())
    }

    #[test]
    fn a_raw_identifier_field_is_declared_without_its_prefix() -> Result<(), ParamError> {
        let names: Vec<&str> = Keyword::table()?
            .params()
            .iter()
            .map(|param| param.name.as_str())
            .collect();
        assert_eq!(names, ["type"]);
        assert_eq!(
            Keyword::parse(&json!({"type": "saw"}))?,
            Keyword {
                r#type: "saw".to_owned()
            }
        );
        Ok(())
    }

    #[test]
    fn a_derived_struct_reads_values_through_its_table() -> Result<(), ParamError> {
        let given = json!({"depth": 1, "seed": 42, "shape": "saw-down", "on": false, "label": "snare", "centre": 0.75});
        assert_eq!(
            Sample::parse(&given)?,
            Sample {
                depth: 1.0,
                seed: 42,
                shape: Shape::SawDown,
                on: false,
                label: "snare".to_owned(),
                centre: -0.25,
            }
        );
        assert_eq!(Sample::parse(&json!({}))?, Sample::default());
        let refused = Sample::parse(&json!({"depth": true, "shape": "saw_down", "speed": 1}));
        assert!(refused.is_err(), "accepted {refused:?}");
        let Err(error) = refused else {
            return Ok(());
        };
        check_problems(
            "derived",
            &error.problems,
            &[("depth", "number"), ("shape", "one of"), ("speed", "unknown")],
        );
        Ok(())
    }

    #[test]
    fn values_of_another_table_leave_a_derived_struct_at_its_defaults() -> Result<(), ParamError> {
        let other = ParamTable::new(vec![declared("depth", Kind::Text, json!("deep"), "Not a number here.")])?;
        assert_eq!(Sample::from_values(&other.parse(&json!({}))?), Sample::default());
        Ok(())
    }

    #[test]
    fn a_struct_without_fields_takes_no_values() -> Result<(), ParamError> {
        assert_eq!(Nothing::parse(&json!({}))?, Nothing);
        assert_eq!(Nothing::table()?.params(), []);
        let refused = Nothing::parse(&json!({"rate": 1}));
        assert!(
            matches!(&refused, Err(error) if error.problems[0].path == "rate"),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_typed_struct_gives_its_values_by_field_name() {
        let sample = Sample {
            depth: 0.75,
            seed: 3,
            shape: Shape::SawDown,
            on: false,
            label: "hat".to_owned(),
            centre: -0.125,
        };
        assert_eq!(
            sample.to_value(),
            json!({"depth": 0.75, "seed": 3, "shape": "saw-down", "on": false, "label": "hat", "centre": -0.125})
        );
        assert_eq!(Keyword::default().to_value(), json!({"type": "sine"}));
        assert_eq!(Nothing.to_value(), json!({}));
    }

    #[test]
    fn a_typed_struct_is_checked_by_its_table_as_its_values_would_be() -> Result<(), ParamError> {
        // a valid struct comes back as its table reads it: a cyclic field wrapped into its cycle
        let wrapped = Sample {
            depth: 1.0,
            centre: 0.75,
            ..Sample::default()
        };
        assert_eq!(
            wrapped.check()?,
            Sample {
                depth: 1.0,
                centre: -0.25,
                ..Sample::default()
            }
        );
        // (struct, the fields refused with a part of each rule)
        for (id, given, expected) in [
            (
                "out of range both ways",
                Sample {
                    depth: 1.5,
                    seed: -1,
                    ..Sample::default()
                },
                &[("depth", "<= 1"), ("seed", ">= 0")][..],
            ),
            (
                "not finite",
                Sample {
                    depth: f64::NAN,
                    ..Sample::default()
                },
                &[("depth", "must be a number")],
            ),
        ] {
            let refused = given.check();
            assert!(refused.is_err(), "{id}: accepted {refused:?}");
            if let Err(error) = refused {
                check_problems(id, &error.problems, expected);
            }
        }
        Ok(())
    }

    #[test]
    fn a_derived_declaration_that_breaks_a_rule_is_refused_on_every_call() {
        for _ in 0..2 {
            let refused = Broken::table();
            assert!(refused.is_err(), "accepted {refused:?}");
            let Err(error) = refused else {
                return;
            };
            check_problems(
                "broken",
                &error.problems,
                &[
                    ("strength", "do not name"),
                    ("strength", "needs a description"),
                    ("strength", "the default 2.0"),
                ],
            );
        }
        assert!(Broken::parse(&json!({})).is_err());
    }

    #[test]
    fn a_derived_choice_names_each_variant_in_snake_case_unless_it_is_named() {
        assert_eq!(Shape::NAMES, ["sine", "saw_up", "saw-down"]);
        for (name, shape) in [
            ("sine", Shape::Sine),
            ("saw_up", Shape::SawUp),
            ("saw-down", Shape::SawDown),
        ] {
            assert_eq!(Shape::from_name(name), Some(shape));
            assert_eq!(shape.name(), name);
        }
        for wrong in ["Sine", "saw_down", "sawup", ""] {
            assert_eq!(Shape::from_name(wrong), None, "{wrong}");
        }
    }
}
