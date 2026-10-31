//! The built-ins, one module per file in `src/builtin/`, and their tables; the build script writes both.
//!
//! A built-in file declares `Params`, its parameters: a struct with `#[derive(Params)]`, `pub` fields, each with a doc
//! comment (its description) and `#[param(..)]` (its default, unit and range), and a `#[derive(Choice)]` enum for each
//! field that picks among names. That declaration is the only one: the table, the defaults, the description and the
//! checks all come from it. A file that also defines `pub(crate) fn run(inputs: &[f64], params: &Params) -> Vec<f64>`,
//! the causal recurrence over the input sampled at the shaper's rate, is a shaper the core runs; one without it only
//! declares (its source or operation still runs in Python). Built-ins never refer to one another: what two of them
//! share goes in a module of the crate named for its concept.

use crate::Error;
use crate::Params as _;
use crate::params::{ParamError, ParamTable};

/// The function that gives one built-in's parameter table.
pub(crate) type Table = fn() -> Result<&'static ParamTable, ParamError>;

/// The function that runs one shaper: reads its parameters, then runs it over the inputs.
pub(crate) type Run = fn(&[f64], &serde_json::Value) -> Result<Vec<f64>, Error>;

include!(concat!(env!("OUT_DIR"), "/builtin.rs"));

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{PARAMS, SHAPERS, lfo};
    use crate::Params as _;

    /// The names the Python package registers, sorted: sources, shapers and operations.
    const NAMES: [&str; 21] = [
        "beats",
        "clip",
        "constant",
        "delay",
        "envelope",
        "every",
        "follow",
        "gate",
        "hold",
        "keyframes",
        "latch",
        "lfo",
        "noise",
        "normalise",
        "quantize",
        "remap",
        "select",
        "shift",
        "slew",
        "steps",
        "thin",
    ];

    fn described(name: &str) -> Result<serde_json::Value, String> {
        let (_, table) = PARAMS
            .iter()
            .find(|(built_in, _)| *built_in == name)
            .ok_or_else(|| format!("no built-in called {name}"))?;
        Ok(table().map_err(|error| error.to_string())?.describe())
    }

    #[test]
    fn every_built_in_declares_a_table_that_builds() {
        let names: Vec<&str> = PARAMS.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, NAMES);
        for (name, table) in PARAMS {
            assert!(table().is_ok(), "{name}: {:?}", table().err());
        }
    }

    #[test]
    fn only_the_built_ins_with_a_run_function_are_shapers() {
        let shapers: Vec<&str> = SHAPERS.iter().map(|(name, _)| *name).collect();
        assert_eq!(shapers, ["follow", "slew"]);
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
            per: lfo::Per::Second,
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
}
