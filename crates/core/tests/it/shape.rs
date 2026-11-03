//! The `shape` door: a shaper of the built-ins by its name.

use bullterrier_control::{Error, FromValues, Interpolation, Signal, shape};
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
    let names: Vec<&str> = bullterrier_control::Registry::builtin()
        .things()
        .map(bullterrier_control::registry::Thing::name)
        .collect();
    assert!(names.contains(&"slew"), "{names:?}");
    let unknown = format!(
        "unknown source, shaper or operation 'no_such_shaper'; available: {}",
        names.join(", ")
    );
    for refused in [
        render("no_such_shaper", &slew_params()).map(|_| ()),
        bullterrier_control::Registry::builtin()
            .get("no_such_shaper")
            .map(|_| ()),
    ] {
        assert_eq!(refused.map_err(|error| error.to_string()), Err(unknown.clone()));
    }
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
