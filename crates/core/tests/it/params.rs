//! The derives' promises: a struct declares its table, defaults and values from its attributes and doc comments.

use serde_json::json;

use crate::common::problems::check_problems;
use bullterrier_control::params::{Choice, Kind, Param, ParamError, ParamTable, ParamUnit, Params};

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
