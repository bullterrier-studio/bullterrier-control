//! The units of a series: the format's seven, read back from their names.

use bullterrier_control::formats::Unit;
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
