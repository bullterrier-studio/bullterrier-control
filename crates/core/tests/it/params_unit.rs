//! A parameter unit is read back from its name.

use bullterrier_control::params::ParamUnit;

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
