//! A signal that holds one value at every time.

use crate::{Error, Signal, Source};

/// The value a constant signal holds.
#[derive(Clone, Debug, PartialEq, crate::Params)]
pub struct Params {
    /// The value the signal holds at every time; negative values are fine.
    #[param(default = 0.0, unit = Ratio, min = -1000000.0, max = 1000000.0)]
    pub value: f64,
}

impl Source for Params {
    const NAME: &'static str = "constant";

    /// The signal that is `value` at every time. The body only: [`lib::constant`](crate::lib::constant) checks the
    /// value against its range, names and records it.
    ///
    /// ```
    /// use bullterrier_control::builtin::constant;
    /// use bullterrier_control::{lib, Source};
    ///
    /// let level = constant::Params { value: 0.75 };
    /// assert_eq!(level.source()?.sample(&[0.0, 1.5, 3600.0])?, [0.75, 0.75, 0.75]);
    /// // the door refuses a value beyond its documented range
    /// assert!(lib::constant(constant::Params { value: 1e9 }).is_err());
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`Error::Signal`] if `value` is not finite (a door has refused that before this is called).
    fn source(&self) -> Result<Signal, Error> {
        Signal::constant(self.value)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::registry::{Call, Output};
    use crate::{Registry, builtin::constant, lib};

    /// What a row of the table expects of a call of `constant`.
    enum Expect {
        /// The signal holds this at every time.
        Holds(f64),
        /// The call is refused, naming this parameter.
        Refuses(&'static str),
    }
    use Expect::{Holds, Refuses};

    /// The rows are those of the Python tests (`tests/builtin/test_constant.py`), then the edges of the range.
    fn rows() -> Vec<(&'static str, Value, Expect)> {
        vec![
            ("a value is held at every time", json!({"value": 0.75}), Holds(0.75)),
            ("the default is zero", json!({}), Holds(0.0)),
            ("a negative value is fine", json!({"value": -3}), Holds(-3.0)),
            ("the top of the range is allowed", json!({"value": 1e6}), Holds(1e6)),
            ("the bottom of the range is allowed", json!({"value": -1e6}), Holds(-1e6)),
            ("outside the documented range is refused", json!({"value": 1e9}), Refuses("value")),
            ("just above the range is refused", json!({"value": 1_000_000.5}), Refuses("value")),
            ("just below the range is refused", json!({"value": -1_000_000.5}), Refuses("value")),
            ("text is not a value", json!({"value": "x"}), Refuses("value")),
            ("true is not a value", json!({"value": true}), Refuses("value")),
            ("a parameter it does not have is refused", json!({"valu": 1}), Refuses("valu")),
        ]
    }

    const TIMES: [f64; 3] = [0.0, 1.5, 3600.0];

    #[test]
    fn through_the_registry_a_constant_holds_its_value_and_a_bad_value_is_refused_naming_it() -> Result<(), Error> {
        let thing = Registry::builtin().get("constant")?;
        for (id, values, expected) in rows() {
            let made = thing.call(&Call { values, ..Call::default() });
            match (expected, made) {
                (Holds(value), Ok(Output::Signal(signal))) => assert_eq!(signal.sample(&TIMES)?, [value; 3], "{id}"),
                (Refuses(name), Err(Error::Params { shaper, problems, .. })) => {
                    assert_eq!(shaper, "constant", "{id}");
                    assert_eq!(problems.iter().map(|problem| problem.path.as_str()).collect::<Vec<_>>(), [name], "{id}");
                }
                (_, other) => {
                    return Err(Error::Call {
                        message: format!("{id}: not what the row expects: {other:?}"),
                    });
                }
            }
        }
        Ok(())
    }

    #[test]
    fn through_the_typed_door_it_holds_the_same_and_refuses_the_same() -> Result<(), Error> {
        assert_eq!(lib::constant(constant::Params { value: 0.75 })?.sample(&TIMES)?, [0.75; 3]);
        assert_eq!(lib::constant(constant::Params::default())?.sample(&TIMES)?, [0.0; 3]);
        assert_eq!(lib::constant(constant::Params { value: -3.0 })?.at(2.0)?, -3.0);
        for value in [1e9, f64::NAN, f64::INFINITY] {
            let refused = lib::constant(constant::Params { value });
            assert!(
                matches!(&refused, Err(Error::Params { shaper, problems, .. })
                    if shaper == "constant" && problems.len() == 1 && problems[0].path == "value"),
                "{value}: {refused:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn it_is_recorded_by_name_and_values_whichever_door_made_it() -> Result<(), Error> {
        let by_name = Registry::builtin().get("constant")?.call(&Call { values: json!({"value": 0.75}), ..Call::default() })?;
        let Output::Signal(by_name) = by_name else {
            return Err(Error::Call { message: format!("not a signal: {by_name:?}") });
        };
        let typed = lib::constant(constant::Params { value: 0.75 })?;
        let defaulted = lib::constant(constant::Params::default())?;
        for (signal, value) in [(&by_name, 0.75), (&typed, 0.75), (&defaulted, 0.0)] {
            let recipe = signal.recipe().ok_or(Error::Signal { message: "no recipe".into() })?;
            assert_eq!(recipe.name, "constant");
            assert_eq!(recipe.values, json!({ "value": value }));
            assert!(recipe.inputs.is_empty() && recipe.signal.is_none());
            assert!(recipe.made_by(Registry::builtin().get("constant")?));
        }
        Ok(())
    }

    #[test]
    fn it_has_a_name_a_kind_and_its_words() -> Result<(), Error> {
        let thing = Registry::builtin().get("constant")?;
        assert_eq!(thing.kind(), crate::registry::Kind::Source);
        assert_eq!(thing.returns(), crate::registry::Returns::Signal);
        assert_eq!(thing.summary(), "A signal that holds one value at every time.");
        assert_eq!(thing.inputs(), []);
        Ok(())
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            #[test]
            fn any_value_in_range_is_held_at_every_time_and_any_beyond_it_is_refused(
                value in -1_000_000.0_f64..=1_000_000.0,
                beyond in 1_000_000.001_f64..=1e300,
                times in prop::collection::vec(0.0_f64..=1e6, 0..20),
            ) {
                let held = lib::constant(constant::Params { value }).map_err(|error| TestCaseError::fail(error.to_string()))?;
                let sampled = held.sample(&times).map_err(|error| TestCaseError::fail(error.to_string()))?;
                prop_assert_eq!(sampled, vec![value; times.len()]);
                prop_assert!(lib::constant(constant::Params { value: beyond }).is_err(), "{beyond} accepted");
                prop_assert!(lib::constant(constant::Params { value: -beyond }).is_err(), "-{beyond} accepted");
            }
        }
    }
}
