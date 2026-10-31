//! The numeric core of bullterrier-control: the shapers implemented in Rust and the primitives they share, and the
//! plain-data [`formats`] (Unit, Event, Series, Grid, Bundle) that every package of the ecosystem reads and writes.
//! A thing's parameters are declared as a [`params`] table (in Rust by `#[derive(Params)]`), which holds every rule
//! they follow.
//!
//! Pure Rust, no Python: the `bullterrier-control-python` crate binds it to the package. A shaper's parameters arrive
//! already validated by the Python package, which declares them; the core trusts them and does not check their ranges
//! again.

#![forbid(unsafe_code)]

// the derives write `::bullterrier_control::..` paths, which resolve inside this crate too
extern crate self as bullterrier_control;

mod builtin;
mod error;
pub mod formats;
pub mod params;

pub use error::Error;
pub use params::{Choice, Params};

/// Run the shaper called `name` over `inputs` (its input signal sampled at the shaper's rate from time 0) with `params`
/// (an object with exactly the shaper's parameters); a new series of the same length, output `i` depending on inputs
/// `0..=i` only.
///
/// # Errors
/// [`Error::UnknownShaper`] if the core implements no shaper of that name, and [`Error::Params`] if `params` misses
/// one of its parameters, has one it does not know, or one of the wrong type.
pub fn render(name: &str, inputs: &[f64], params: &serde_json::Value) -> Result<Vec<f64>, Error> {
    let (_, run) = builtin::SHAPERS
        .iter()
        .find(|(shaper, _)| *shaper == name)
        .ok_or_else(|| Error::UnknownShaper { name: name.to_owned() })?;
    run(inputs, params)
}

/// The names of the shapers the core implements, sorted.
#[must_use]
pub fn shapers() -> Vec<&'static str> {
    builtin::SHAPERS.iter().map(|(name, _)| *name).collect()
}

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
        assert_eq!(
            result,
            Err(Error::UnknownShaper {
                name: "no_such_shaper".to_owned()
            })
        );
    }

    #[test]
    fn parameters_that_drift_from_the_shapers_are_refused_naming_the_field() {
        let mut missing = slew_params();
        missing.as_object_mut().map(|fields| fields.remove("fall"));
        let mut extra = slew_params();
        extra
            .as_object_mut()
            .map(|fields| fields.insert("depth".to_owned(), json!(1.0)));
        for (params, field) in [
            (missing, "fall"),
            (extra, "depth"),
            (json!({"rise": "slow", "fall": 0.5, "rate": 10.0}), "string"),
        ] {
            let result = render("slew", &[0.0], &params);
            assert!(
                matches!(&result, Err(error @ Error::Params { shaper, .. }) if shaper == "slew" && error.to_string().contains(field)),
                "{field}: {result:?}"
            );
        }
    }
}
