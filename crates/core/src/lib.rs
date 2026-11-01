//! The numeric core of bullterrier-control: a [`Signal`] (a value readable at any time in seconds, a graph of nodes
//! that combine), the shapers implemented in Rust and the primitives they share, and the plain-data [`formats`] (Unit,
//! Event, Series, Grid, Bundle) that every package of the ecosystem reads and writes.
//! A thing's parameters are declared as a [`params`] table (in Rust by `#[derive(Params)]`), which holds every rule
//! they follow.
//!
//! Pure Rust, no Python: the `bullterrier-control-python` crate binds it to the package. Every built-in source, shaper
//! and operation declares its parameters in Rust ([`builtin`]); [`parameters`] gives a built-in's table, and [`shape`]
//! shapes a signal with a shaper the core implements and values it reads through that table, so a value is checked the
//! same way wherever it comes from. A shaper is also a method of [`Signal`] taking its typed parameters
//! ([`Signal::slew`]), and any [`Shaper`] goes into a chain by [`Signal::pipe`]. The [`registry`] holds every source,
//! shaper and operation by name ([`Registry::builtin`]: those the core runs), with the rules for getting in and the
//! one checked door through which each is called.

#![forbid(unsafe_code)]

// the derives write `::bullterrier_control::..` paths, which resolve inside this crate too
extern crate self as bullterrier_control;

pub mod builtin;
pub mod cadence;
pub mod easing;
mod error;
pub mod formats;
pub mod hashing;
pub mod inputs;
pub mod params;
pub mod patch;
pub mod registry;
pub mod rounding;
pub mod scaling;
mod signal;

pub use error::Error;
pub use params::{Choice, Params};
pub use patch::Patch;
pub use registry::{Operation, Registry, Shaper, Source};
pub use signal::{FromValues, Interpolation, MAX_DEPTH, Origin, Rendering, Signal, maximum, minimum};

/// The signal the shaper called `name` of [`Registry::builtin`] makes of `input` with `params` (an object of the
/// shaper's parameters by name; one left out takes its default): its [`Thing::call`](registry::Thing::call).
///
/// ```
/// use bullterrier_control::{FromValues, Interpolation, Signal, shape};
/// use serde_json::json;
///
/// let step = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
/// let ramp = shape("slew", &step, &json!({"rise": 0.5, "fall": 0.5, "rate": 10.0}))?;
/// assert_eq!(ramp.sample(&[0.0, 0.1, 0.2])?, [0.0, 0.2, 0.4]);
///
/// let refused = shape("slew", &step, &json!({"rise": "slow"})).map_err(|error| error.to_string());
/// assert_eq!(refused.err().as_deref(), Some(r#"the parameters of "slew" do not fit: rise: must be a number (got "slow")"#));
/// # Ok::<(), bullterrier_control::Error>(())
/// ```
///
/// # Errors
/// As [`Registry::get`] and [`Thing::call`](registry::Thing::call): [`Error::UnknownName`] if the core runs nothing
/// of that name, [`Error::Call`] if it is not a shaper (it takes no signal), [`Error::Params`] listing every rule
/// `params` breaks (a field of another type, out of its range, or one the shaper does not have), and [`Error::Depth`]
/// if the graph would be deeper than [`MAX_DEPTH`].
pub fn shape(name: &str, input: &Signal, params: &serde_json::Value) -> Result<Signal, Error> {
    Registry::builtin().get(name)?.signal(registry::Call {
        signals: vec![input.clone()],
        values: params.clone(),
        ..registry::Call::default()
    })
}

/// The sources the core runs, as functions of their typed parameters (`lib::constant(constant::Params { .. })`), each
/// made through the [`registry`]'s one door (checked, named and recorded as a call by name is); a shaper is a
/// method of [`Signal`] ([`Signal::pipe`]), and every thing the core runs is in [`Registry::builtin`] by name.
pub mod lib {
    include!(concat!(env!("OUT_DIR"), "/lib.rs"));
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
/// [`Error::UnknownName`] listing the built-ins if there is none of that name, and [`Error::Params`] if its
/// declaration breaks a rule (a bug in the core, which its tests rule out).
pub fn parameters(name: &str) -> Result<&'static params::ParamTable, Error> {
    let (_, table) = builtin::PARAMS
        .iter()
        .find(|(built_in, _)| *built_in == name)
        .ok_or_else(|| Error::UnknownName {
            name: name.to_owned(),
            available: builtins().into_iter().map(str::to_owned).collect(),
        })?;
    table().map_err(|error| Error::params(name, error))
}

/// The names of the built-in sources, shapers and operations, sorted: each has a table ([`parameters`]).
#[must_use]
pub fn builtins() -> Vec<&'static str> {
    builtin::PARAMS.iter().map(|(name, _)| *name).collect()
}

#[cfg(test)]
mod builtin_table;
#[cfg(test)]
mod memory_cap;
#[cfg(test)]
mod scan;

#[cfg(test)]
mod tests {
    use super::{Error, FromValues, Interpolation, Signal, shape};
    use serde_json::json;

    fn slew_params() -> serde_json::Value {
        json!({"rise": 0.5, "fall": 0.5, "rate": 10.0})
    }

    /// A step from 0 to 1 at 0.1 s.
    fn step() -> Result<Signal, Error> {
        Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)
    }

    /// `shape` of `name` over [`step`] with `params`.
    fn render(name: &str, params: &serde_json::Value) -> Result<Signal, Error> {
        shape(name, &step()?, params)
    }

    #[test]
    fn a_shaper_shapes_a_signal_by_its_name() -> Result<(), Error> {
        // 0.1 s a sample, 0.2 a sample up: 0.2 at 0.1 s, 0.4 at 0.25 s (sample 2), 1 from 0.5 s
        let out = render("slew", &slew_params())?.sample(&[0.0, 0.1, 0.25, 0.5, 9.0])?;
        assert_eq!(out.len(), 5);
        assert!(
            out.iter()
                .zip([0.0, 0.2, 0.4, 1.0, 1.0])
                .all(|(got, want)| (got - want).abs() <= 1e-12),
            "{out:?}"
        );
        // follow, with an instant attack, is its input
        assert_eq!(
            render("follow", &json!({"attack": 0, "rate": 10.0}))?.sample(&[0.0, 0.1])?,
            [0.0, 1.0]
        );
        Ok(())
    }

    #[test]
    fn a_name_the_core_runs_no_shaper_of_is_refused_as_the_registry_refuses_it() {
        let unknown = "unknown source, shaper or operation 'no_such_shaper'; available: beats, constant, delay, every, follow, hold, latch, quantize, select, shift, slew, thin";
        for refused in [
            render("no_such_shaper", &slew_params()).map(|_| ()),
            crate::Registry::builtin().get("no_such_shaper").map(|_| ()),
        ] {
            assert_eq!(refused.map_err(|error| error.to_string()), Err(unknown.to_owned()));
        }
        // a built-in's table asked for by a name with none
        let table = super::parameters("no_such_thing")
            .map(|_| ())
            .map_err(|error| error.to_string());
        assert!(
            table.as_ref().is_err_and(|text| text
                .starts_with("unknown source, shaper or operation 'no_such_thing'; available: beats, clip, constant,")),
            "{table:?}"
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
        let result = render("slew", &given);
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
            let result = render("slew", &given);
            let named = match &result {
                Err(Error::Params { shaper, problems, .. }) if shaper == "slew" => {
                    problems.iter().map(|problem| problem.path.clone()).collect()
                }
                other => vec![format!("not a parameter error: {other:?}")],
            };
            assert_eq!(named, fields, "{given}");
        }
        // a field left out takes its default
        assert!(render("slew", &missing).is_ok());
    }
}
