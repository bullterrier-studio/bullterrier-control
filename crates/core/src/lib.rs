//! The numeric core of bullterrier-control: the shapers implemented in Rust and the primitives they share, and the
//! plain-data [`formats`] (Unit, Event, Series, Grid, Bundle) that every package of the ecosystem reads and writes.
//! A thing's parameters are declared as a [`params`] table (in Rust by `#[derive(Params)]`), which holds every rule
//! they follow.
//!
//! Pure Rust, no Python: the `bullterrier-control-python` crate binds it to the package. Every built-in source, shaper
//! and operation declares its parameters in Rust ([`builtin`]); [`parameters`] gives a built-in's table and [`render`]
//! runs a shaper with values it reads through that table, so a value is checked the same way wherever it comes from.

#![forbid(unsafe_code)]

// the derives write `::bullterrier_control::..` paths, which resolve inside this crate too
extern crate self as bullterrier_control;

pub mod builtin;
pub mod cadence;
pub mod easing;
mod error;
pub mod formats;
pub mod hashing;
pub mod params;
pub mod rounding;
pub mod scaling;
mod signal;

pub use error::Error;
pub use params::{Choice, Params};
pub use signal::Signal;

/// Run the shaper called `name` over `inputs` (its input signal sampled at the shaper's rate from time 0) with `params`
/// (an object of the shaper's parameters by name; one left out takes its default); a new series of the same length,
/// output `i` depending on inputs `0..=i` only.
///
/// ```
/// use serde_json::json;
///
/// let out = bullterrier_control::render("slew", &[0.0, 1.0, 1.0], &json!({"rise": 0.5, "fall": 0.5, "rate": 10.0}))?;
/// assert_eq!(out.len(), 3);
///
/// let refused = bullterrier_control::render("slew", &[0.0], &json!({"rise": "slow"})).map_err(|error| error.to_string());
/// assert_eq!(refused, Err(r#"the parameters of "slew" do not fit: rise: must be a number (got "slow")"#.to_owned()));
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
///
/// # Errors
/// [`Error::UnknownShaper`] if the core implements no shaper of that name, and [`Error::Params`] listing every rule
/// `params` breaks (a field of another type, out of its range, or one the shaper does not have).
pub fn render(name: &str, inputs: &[f64], params: &serde_json::Value) -> Result<Vec<f64>, Error> {
    let (_, run) = builtin::SHAPERS
        .iter()
        .find(|(shaper, _)| *shaper == name)
        .ok_or_else(|| Error::UnknownShaper { name: name.to_owned() })?;
    run(inputs, params)
}

/// The parameter table of the built-in called `name`: a source, shaper or operation, whether or not the core runs it.
///
/// ```
/// let table = bullterrier_control::parameters("slew")?;
/// let names: Vec<&str> = table.params().iter().map(|param| param.name.as_str()).collect();
/// assert_eq!(names, ["rise", "fall", "rate"]);
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
///
/// # Errors
/// [`Error::UnknownShaper`] if there is no built-in of that name (the name is the one asked for, whatever its kind),
/// and [`Error::Params`] if its declaration breaks a rule (a bug in the core, which its tests rule out).
pub fn parameters(name: &str) -> Result<&'static params::ParamTable, Error> {
    let (_, table) = builtin::PARAMS
        .iter()
        .find(|(built_in, _)| *built_in == name)
        .ok_or_else(|| Error::UnknownShaper { name: name.to_owned() })?;
    table().map_err(|error| Error::params(name, error))
}

/// The names of the built-in sources, shapers and operations, sorted: each has a table ([`parameters`]).
#[must_use]
pub fn builtins() -> Vec<&'static str> {
    builtin::PARAMS.iter().map(|(name, _)| *name).collect()
}

/// The names of the shapers the core implements, sorted.
#[must_use]
pub fn shapers() -> Vec<&'static str> {
    builtin::SHAPERS.iter().map(|(name, _)| *name).collect()
}

#[cfg(test)]
mod builtin_table;

#[cfg(test)]
mod tests {
    use super::{Error, render, shapers};
    use serde_json::json;

    fn slew_params() -> serde_json::Value {
        json!({"rise": 0.5, "fall": 0.5, "rate": 10.0})
    }

    #[test]
    fn a_shaper_runs_by_its_name_into_a_series_of_the_inputs_length() -> Result<(), Error> {
        let out = render("slew", &[0.0, 1.0, 1.0], &slew_params())?;
        assert_eq!(out.len(), 3);
        assert!((out[1] - 0.2).abs() <= 1e-12, "{out:?}");
        Ok(())
    }

    #[test]
    fn the_shapers_are_named_sorted() {
        let names = shapers();
        assert!(names.contains(&"slew"), "{names:?}");
        assert!(names.is_sorted(), "{names:?}");
    }

    #[test]
    fn an_unknown_shaper_is_refused_by_name() {
        let result = render("no_such_shaper", &[0.0], &slew_params());
        assert!(
            matches!(&result, Err(Error::UnknownShaper { name }) if name == "no_such_shaper"),
            "{result:?}"
        );
    }

    #[test]
    fn a_refusal_lists_the_first_100_problems_and_counts_the_rest() {
        let mut given = slew_params();
        if let Some(fields) = given.as_object_mut() {
            for index in 0..150 {
                fields.insert(format!("extra_{index:03}"), json!(index));
            }
        }
        let result = render("slew", &[0.0], &given);
        let Err(error) = &result else {
            assert!(result.is_err(), "accepted 150 unknown fields");
            return;
        };
        let text = error.to_string();
        assert_eq!(text.matches(": unknown field").count(), 100, "{text}");
        assert!(text.ends_with("; ... and 50 more"), "{text}");
        assert!(
            matches!(error, Error::Params { problems, .. } if problems.len() == 100),
            "{error:?}"
        );
    }

    #[test]
    fn parameters_that_break_the_rules_are_refused_naming_each_field() {
        let mut missing = slew_params();
        missing.as_object_mut().map(|fields| fields.remove("fall"));
        let mut extra = slew_params();
        extra
            .as_object_mut()
            .map(|fields| fields.insert("depth".to_owned(), json!(1.0)));
        // (given, the fields named, in order)
        for (given, fields) in [
            (json!({"rise": "slow", "rate": 10.0}), vec!["rise"]),
            (extra, vec!["depth"]),
            (
                json!({"rise": 4000.0, "fall": -1, "rate": 5.0}),
                vec!["rise", "fall", "rate"],
            ),
            (
                json!({"rise": 0.5, "fall": true, "depth": 1, "speed": 2}),
                vec!["fall", "depth", "speed"],
            ),
            (json!(3), vec![""]),
        ] {
            let result = render("slew", &[0.0], &given);
            let named = match &result {
                Err(Error::Params { shaper, problems, .. }) if shaper == "slew" => {
                    problems.iter().map(|problem| problem.path.clone()).collect()
                }
                other => vec![format!("not a parameter error: {other:?}")],
            };
            assert_eq!(named, fields, "{given}");
        }
        // a field left out takes its default
        assert!(render("slew", &[0.0], &missing).is_ok());
    }
}
