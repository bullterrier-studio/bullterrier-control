//! The built-ins, one module per file in `src/builtin/`, and their tables; the build script writes both.
//!
//! A built-in file declares `Params`, its parameters: a struct with `#[derive(Params)]`, `pub` fields, each with a doc
//! comment (its description) and `#[param(..)]` (its default, unit and range), and a `#[derive(Choice)]` enum for each
//! field that picks among names. That declaration is the only one: the table, the defaults, the description and the
//! checks all come from it. A rule between fields (`high` is not below `low`) is a `fn rule(&Params) -> Vec<Problem>`
//! named by `#[param(rule = rule)]` on the struct; the table runs it once every field is valid.
//!
//! The file also implements one of [`Source`](crate::Source), [`Shaper`](crate::Shaper) and
//! [`Operation`](crate::Operation) for its `Params` (`impl Shaper for Params`, its `NAME` the file's name, which the
//! build checks; a file without one does not build): it is in [`Registry::builtin`](crate::Registry::builtin) under
//! that name, with the file's leading `//!` lines as its doc (what `help()` shows) and the doc of `Params` as its
//! parameters'; a source is also a function of [`lib`](crate::lib). A shaper may also be a method of
//! [`Signal`](crate::Signal) taking its typed
//! `Params`: [`Signal::pipe`](crate::Signal::pipe) of them, so it is checked, named and recorded as every door makes
//! it. A shaper that needs history builds its signal as a [`Signal::rendered`](crate::Signal::rendered) over its
//! private causal recurrence `run` (`slew` is the model).
//! Built-ins never refer to one another: what two of them share goes in a module of the crate named for its concept.

include!(concat!(env!("OUT_DIR"), "/builtin.rs"));

/// Whether a thing's `NAME` is its file's name: what the generated registration asserts at compile time.
const fn named(name: &str, file: &str) -> bool {
    let (name, file) = (name.as_bytes(), file.as_bytes());
    if name.len() != file.len() {
        return false;
    }
    let mut at = 0;
    while at < name.len() {
        if name[at] != file[at] {
            return false;
        }
        at += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::lfo;
    use crate::Params as _;
    use crate::Registry;
    use crate::params::{Kind as ParamKind, ParamUnit};

    fn described(name: &str) -> Result<serde_json::Value, String> {
        let thing = Registry::builtin().get(name).map_err(|error| error.to_string())?;
        Ok(thing.table().describe())
    }

    /// The names of the built-in files.
    fn files() -> Result<Vec<String>, std::io::Error> {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("builtin");
        let mut names = Vec::new();
        for entry in std::fs::read_dir(folder)? {
            let path = entry?.path();
            if path.extension().is_some_and(|extension| extension == "rs")
                && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
            {
                names.push(name.to_owned());
            }
        }
        Ok(names)
    }

    /// What does not match between the built-in `files` and the things `registered`: a file no thing is registered
    /// for, and a thing with no file.
    fn unmatched(files: &[String], registered: &[&str]) -> Vec<String> {
        let unregistered = files
            .iter()
            .filter(|file| !registered.contains(&file.as_str()))
            .map(|file| format!("the file {file}.rs is not registered"));
        let fileless = registered
            .iter()
            .filter(|name| !files.iter().any(|file| file == *name))
            .map(|name| format!("{name} is registered without a file"));
        unregistered.chain(fileless).collect()
    }

    #[test]
    fn a_thing_is_registered_for_each_built_in_file_and_for_nothing_else() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = Registry::new();
        super::register(&mut registry)?;
        let registered: Vec<&str> = registry.things().map(crate::registry::Thing::name).collect();
        let files = files()?;
        assert!(files.iter().any(|file| file == "constant"), "{files:?}");
        assert_eq!(unmatched(&files, &registered), Vec::<String>::new());
        // the core's registry is that one
        let builtin: Vec<&str> = Registry::builtin().things().map(crate::registry::Thing::name).collect();
        assert_eq!(builtin, registered);
        Ok(())
    }

    #[test]
    fn the_match_of_files_and_things_names_a_file_not_registered_and_a_thing_without_a_file() {
        let files = ["clip".to_owned(), "slew".to_owned()];
        // (row, the things registered, what does not match)
        let rows: [(&str, &[&str], &[&str]); 4] = [
            ("the same", &["clip", "slew"], &[]),
            (
                "a file not registered",
                &["clip"],
                &["the file slew.rs is not registered"],
            ),
            (
                "a thing without a file",
                &["clip", "drift", "slew"],
                &["drift is registered without a file"],
            ),
            (
                "both",
                &["clip", "drift"],
                &[
                    "the file slew.rs is not registered",
                    "drift is registered without a file",
                ],
            ),
        ];
        for (row, registered, expected) in rows {
            assert_eq!(unmatched(&files, registered), expected, "{row}");
        }
    }

    #[test]
    fn slew_describes_its_fields_as_the_python_model_did() -> Result<(), String> {
        assert_eq!(
            described("slew")?,
            json!({
                "rise": {"type": "number", "default": 0.1, "title": "Rise", "unit": "seconds",
                    "description": "Seconds the output takes to climb by 1; larger is slower. 0 follows a rise at once.",
                    "minimum": 0.0, "maximum": 3600.0, "advanced": false, "cyclic": false},
                "fall": {"type": "number", "default": 0.1, "title": "Fall", "unit": "seconds",
                    "description": "Seconds the output takes to fall by 1; larger is slower. 0 follows a fall at once.",
                    "minimum": 0.0, "maximum": 3600.0, "advanced": false, "cyclic": false},
                "rate": {"type": "number", "default": 1000.0, "title": "Rate", "unit": "hertz",
                    "description": "Internal samples per second; the output is exact to one sample. Raising it is more exact and slower.",
                    "minimum": 10.0, "maximum": 48000.0, "advanced": true, "cyclic": false},
            })
        );
        Ok(())
    }

    #[test]
    fn lfo_describes_its_choices_and_its_cyclic_phase() -> Result<(), String> {
        let lfo = described("lfo")?;
        assert_eq!(
            lfo["shape"],
            json!({"type": "string", "enum": ["sine", "triangle", "square", "saw", "saw-down"], "default": "sine",
                "title": "Shape", "unit": null, "advanced": false, "cyclic": false,
                "description": "The wave: 'sine' (smooth), 'triangle' (straight up and down), 'square' (high, then low), 'saw' (rises, then jumps back) or 'saw-down' (falls, then jumps back)."})
        );
        assert_eq!(
            lfo["per"],
            json!({"type": "string", "enum": ["second", "beat"], "default": "second",
                "title": "Per", "unit": null, "advanced": false, "cyclic": false,
                "description": "What `rate` counts in: 'second' (free-running) or 'beat' (follows the tempo of the `grid`, which it needs)."})
        );
        assert_eq!(
            lfo["phase"],
            json!({"type": "number", "default": 0.0, "title": "Phase", "unit": "cycles",
                "description": "Where in the cycle time 0 falls, as a share of a cycle, 0 to 1: 0.25 starts a quarter of the way in. Any number is accepted and wraps into 0 to 1 (1.25 is 0.25), so a phase that keeps turning needs no wrap.",
                "minimum": 0.0, "maximum": 1.0, "advanced": false, "cyclic": true})
        );
        let names: Vec<&str> = lfo
            .as_object()
            .map(|fields| fields.keys().map(String::as_str).collect())
            .unwrap_or_default();
        assert_eq!(names, ["duty", "high", "low", "per", "phase", "rate", "shape"]);
        Ok(())
    }

    #[test]
    fn noise_describes_its_seed_as_a_count_up_to_the_largest_u32() -> Result<(), String> {
        let noise = described("noise")?;
        assert_eq!(
            noise["seed"],
            json!({"type": "integer", "default": 0, "title": "Seed", "unit": "count",
                "description": "Which noise, 0 to 4294967295: the same seed gives the same values on every machine, read in any order; another seed gives other values.",
                "minimum": 0, "maximum": 4_294_967_295_u64, "advanced": false, "cyclic": false})
        );
        assert_eq!(
            noise["rate"],
            json!({"type": "number", "default": 4.0, "title": "Rate", "unit": "hertz",
                "description": "Random values per second: a new one every 1 / rate seconds. Raising it makes the noise busier.",
                "minimum": 0.001, "maximum": 1000.0, "advanced": false, "cyclic": false})
        );
        assert_eq!(noise["shape"]["enum"], json!(["hold", "linear", "smooth"]));
        assert_eq!(noise["shape"]["default"], json!("smooth"));
        Ok(())
    }

    #[test]
    fn lfo_given_nothing_is_a_one_hertz_sine_from_0_to_1() -> Result<(), String> {
        let expected = lfo::Params {
            shape: lfo::Shape::Sine,
            rate: 1.0,
            per: crate::cadence::Per::Second,
            phase: 0.0,
            low: 0.0,
            high: 1.0,
            duty: 0.5,
        };
        assert_eq!(lfo::Params::default(), expected);
        assert_eq!(
            lfo::Params::parse(&json!({})).map_err(|error| error.to_string())?,
            expected
        );
        let wrapped =
            lfo::Params::parse(&json!({"shape": "saw-down", "phase": 1.25})).map_err(|error| error.to_string())?;
        assert_eq!((wrapped.shape, wrapped.phase), (lfo::Shape::SawDown, 0.25));
        Ok(())
    }

    #[test]
    fn gate_lists_its_sampling_rate_as_advanced_and_nothing_else() -> Result<(), String> {
        let gate = described("gate")?;
        let advanced: Vec<&str> = ["open", "close", "rate"]
            .into_iter()
            .filter(|field| gate[*field]["advanced"] == json!(true))
            .collect();
        assert_eq!(advanced, ["rate"]);
        assert_eq!(
            (gate["open"]["default"].clone(), gate["close"]["default"].clone()),
            (json!(0.5), json!(0.4))
        );
        assert_eq!(gate["rate"]["unit"], json!("hertz"));
        Ok(())
    }

    /// (built-in, field, kind, default, unit, min, max, flags, choices): every field of every built-in, written out from
    /// the Python models these tables were ported from (`advanced` and `cyclic` as flags).
    #[expect(clippy::too_many_lines, reason = "one row per field of the 21 built-ins")]
    fn declared_fields() -> Vec<Row> {
        const RATIO: (Option<&str>, f64, f64) = (Some("ratio"), -1e6, 1e6);
        let seconds = |max: f64| (Some("seconds"), 0.0, max);
        let fraction = (Some("fraction"), 0.0, 1.0);
        let hertz = (Some("hertz"), 10.0, 48000.0);
        let count = |min: f64, max: f64| (Some("count"), min, max);
        let number = |built_in,
                      field,
                      kind,
                      default: serde_json::Value,
                      (unit, min, max): (Option<&'static str>, f64, f64),
                      flags| {
            Row {
                built_in,
                field,
                kind,
                default,
                unit,
                range: Some((min, max)),
                flags,
                choices: &[],
            }
        };
        let other = |built_in, field, kind, default: serde_json::Value, choices| Row {
            built_in,
            field,
            kind,
            default,
            unit: None,
            range: None,
            flags: "",
            choices,
        };
        vec![
            number("beats", "division", "whole", json!(1), count(1.0, 64.0), ""),
            number("clip", "low", "float", json!(0.0), RATIO, ""),
            number("clip", "high", "float", json!(1.0), RATIO, ""),
            number("constant", "value", "float", json!(0.0), RATIO, ""),
            number("delay", "lag", "float", json!(0.1), seconds(3600.0), ""),
            other("envelope", "label", "text", json!(""), &[]),
            number("envelope", "attack", "float", json!(0.0), seconds(60.0), ""),
            number("envelope", "decay", "float", json!(0.25), seconds(3600.0), ""),
            number("envelope", "sustain", "float", json!(0.0), fraction, ""),
            number("envelope", "release", "float", json!(0.0), seconds(3600.0), ""),
            other(
                "envelope",
                "curve",
                "choice",
                json!("exponential"),
                &["exponential", "linear"],
            ),
            other("envelope", "mix", "choice", json!("max"), &["max", "sum"]),
            number("every", "n", "whole", json!(2), count(1.0, 1000.0), ""),
            number("every", "offset", "whole", json!(0), count(0.0, 999.0), ""),
            number("follow", "attack", "float", json!(0.01), seconds(3600.0), ""),
            number("follow", "release", "float", json!(0.25), seconds(3600.0), ""),
            number("follow", "rate", "float", json!(1000.0), hertz, "advanced"),
            number("gate", "open", "float", json!(0.5), RATIO, ""),
            number("gate", "close", "float", json!(0.4), RATIO, ""),
            number("gate", "rate", "float", json!(1000.0), hertz, "advanced"),
            other("latch", "label", "text", json!(""), &[]),
            number("latch", "before", "float", json!(0.0), fraction, ""),
            number("latch", "after", "float", json!(1.0), fraction, ""),
            other(
                "lfo",
                "shape",
                "choice",
                json!("sine"),
                &["sine", "triangle", "square", "saw", "saw-down"],
            ),
            number("lfo", "rate", "float", json!(1.0), (Some("cycles"), 0.0, 1000.0), ""),
            other("lfo", "per", "choice", json!("second"), &["second", "beat"]),
            number(
                "lfo",
                "phase",
                "float",
                json!(0.0),
                (Some("cycles"), 0.0, 1.0),
                "cyclic",
            ),
            number("lfo", "low", "float", json!(0.0), RATIO, ""),
            number("lfo", "high", "float", json!(1.0), RATIO, ""),
            number("lfo", "duty", "float", json!(0.5), fraction, ""),
            other(
                "noise",
                "shape",
                "choice",
                json!("smooth"),
                &["hold", "linear", "smooth"],
            ),
            number("noise", "rate", "float", json!(4.0), (Some("hertz"), 0.001, 1000.0), ""),
            number("noise", "seed", "whole", json!(0), count(0.0, 4_294_967_295.0), ""),
            number("noise", "low", "float", json!(0.0), RATIO, ""),
            number("noise", "high", "float", json!(1.0), RATIO, ""),
            number(
                "normalise",
                "lower",
                "float",
                json!(0.05),
                (Some("fraction"), 0.0, 0.99),
                "",
            ),
            number(
                "normalise",
                "upper",
                "float",
                json!(0.98),
                (Some("fraction"), 0.01, 1.0),
                "",
            ),
            number("quantize", "division", "whole", json!(1), count(1.0, 64.0), ""),
            number("remap", "low", "float", json!(0.0), RATIO, ""),
            number("remap", "high", "float", json!(1.0), RATIO, ""),
            other("remap", "ease", "text", json!("linear"), &[]),
            other("select", "label", "text", json!(""), &[]),
            number("select", "threshold", "float", json!(0.0), fraction, ""),
            number(
                "shift",
                "offset",
                "float",
                json!(0.0),
                (Some("seconds"), -3600.0, 3600.0),
                "",
            ),
            number("slew", "rise", "float", json!(0.1), seconds(3600.0), ""),
            number("slew", "fall", "float", json!(0.1), seconds(3600.0), ""),
            number("slew", "rate", "float", json!(1000.0), hertz, "advanced"),
            number("steps", "rate", "float", json!(4.0), (Some("count"), 0.001, 1000.0), ""),
            other("steps", "per", "choice", json!("second"), &["second", "beat"]),
            number("thin", "gap", "float", json!(0.1), seconds(3600.0), ""),
        ]
    }

    /// One field's declaration, flattened for comparison.
    #[derive(Debug, PartialEq)]
    struct Row {
        built_in: &'static str,
        field: &'static str,
        kind: &'static str,
        default: serde_json::Value,
        unit: Option<&'static str>,
        range: Option<(f64, f64)>,
        flags: &'static str,
        choices: &'static [&'static str],
    }

    #[test]
    fn every_field_of_every_built_in_is_declared_as_the_python_models_did() {
        let expected: Vec<_> = declared_fields()
            .into_iter()
            .map(|row| {
                (
                    (row.built_in, row.field, row.kind, row.default),
                    (row.unit, row.range, row.flags, row.choices.to_vec()),
                )
            })
            .collect();
        // the built-ins ported from Python, each with all its fields; a later one is not written out here
        let ported = |name: &str| expected.iter().any(|((built_in, ..), _)| *built_in == name);
        let mut declared = Vec::new();
        for thing in Registry::builtin().things().filter(|thing| ported(thing.name())) {
            let name = thing.name();
            for param in thing.table().params() {
                let (kind, choices) = match &param.kind {
                    ParamKind::Float => ("float", Vec::new()),
                    ParamKind::Whole => ("whole", Vec::new()),
                    ParamKind::Switch => ("switch", Vec::new()),
                    ParamKind::Text => ("text", Vec::new()),
                    ParamKind::Choice(names) => ("choice", names.iter().map(String::as_str).collect()),
                };
                let flags = match (param.advanced, param.cyclic) {
                    (false, false) => "",
                    (true, false) => "advanced",
                    (false, true) => "cyclic",
                    (true, true) => "advanced cyclic",
                };
                declared.push((
                    (name, param.name.as_str(), kind, param.default.clone()),
                    (
                        param.unit.map(ParamUnit::as_str),
                        param.min.zip(param.max),
                        flags,
                        choices,
                    ),
                ));
            }
        }
        assert_eq!(declared, expected);
    }
}
