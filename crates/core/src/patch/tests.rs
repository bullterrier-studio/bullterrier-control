//! A patch through its public face: reading (every rule, with the path it names), references, building (the numbers,
//! by hand), building only some nodes, what the bundle must hold, the TOML text, the schema, and a property that a
//! patch builds what the same calls by hand build. Every expected path and message is worked out by hand from the
//! Python package's patch (`patch.py`, its tests) and the rules of the modules it uses; none is computed by the code
//! under test. The things are made here (`Thing::custom`), each with a body whose numbers are easy to work out, beside
//! the built-ins `slew` and `follow`.

use std::convert::Infallible;

use proptest::prelude::*;
use serde_json::{Value, json};

use super::{Built, Patch, PatchError, patch_schema};
use crate::Error;
use crate::Signal;
use crate::formats::Bundle;
use crate::formats::Event;
use crate::formats::FormatError;
use crate::formats::Problem;
use crate::formats::Series;
use crate::formats::SeriesOptions;
use crate::formats::Unit;
use crate::inputs::Input;
use crate::inputs::InputKind;
use crate::params::Kind as ParamKind;
use crate::params::Param;
use crate::params::ParamTable;
use crate::params::ParamUnit;
use crate::registry::About;
use crate::registry::Call;
use crate::registry::Declared;
use crate::registry::Kind;
use crate::registry::Output;
use crate::registry::Registry;
use crate::registry::Returns;
use crate::registry::Thing;

type Outcome = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------- the things a test patch uses

/// A decimal field from `min` to `max`, measured as a ratio.
fn decimal(name: &str, default: f64, (min, max): (f64, f64)) -> Param {
    Param {
        name: name.to_owned(),
        kind: ParamKind::Float,
        default: default.into(),
        doc: format!("The {name}."),
        unit: Some(ParamUnit::Ratio),
        min: Some(min),
        max: Some(max),
        advanced: false,
        cyclic: false,
    }
}

/// A choice of `names`, the first by default.
fn choice(name: &str, names: &[&str]) -> Param {
    Param {
        name: name.to_owned(),
        kind: ParamKind::Choice(names.iter().map(|name| (*name).to_owned()).collect()),
        default: names[0].into(),
        doc: format!("The {name}."),
        unit: None,
        min: None,
        max: None,
        advanced: false,
        cyclic: false,
    }
}

/// The thing `name` of `kind` with `params`, `inputs` and `returns`, whose body is `run`.
fn thing(
    (name, kind, returns): (&str, Kind, Returns),
    params: Vec<Param>,
    inputs: &[(InputKind, bool)],
    run: impl Fn(&Call) -> Result<Output, Error> + Send + Sync + 'static,
) -> Result<Thing, Box<dyn std::error::Error>> {
    let declared = Declared {
        name: name.to_owned(),
        kind,
        identity: format!("tests.{name}"),
        shown: None,
        table: ParamTable::new(params).map_err(|error| error.to_string())?,
        inputs: inputs.to_vec(),
        returns,
    };
    let about = About {
        doc: format!("The test thing {name}."),
        ..About::default()
    };
    Ok(Thing::custom(about, declared, run)?)
}

/// The number `name` of a call's values (each is there: the table read them).
fn number(call: &Call, name: &str) -> f64 {
    call.values[name].as_f64().unwrap_or(f64::NAN)
}

/// The events of a call's `events` input.
fn events_of(call: &Call) -> Vec<Event> {
    match call.inputs.get("events") {
        Some(Input::Events(events)) => events.clone(),
        _ => Vec::new(),
    }
}

/// The registry the tests read patches with: the built-ins `slew` and `follow`, and these things.
/// - `level` (source, `value` -10..10): the constant `value`.
/// - `pick` (operation, events required, `threshold` 0..1): the events of a strength at or above `threshold`.
/// - `hits` (source, events required, `gain` 0..10, `mode` 'hold' or 'once', not read): `gain` times the strength
///   of the latest event at or before the time, 0 before the first.
/// - `scale` (operation, series required, `gain` 0..10): the series with every value times `gain`.
/// - `beat` (source, grid optional, `per` 'second' or 'beat'): the time in seconds, or in beats through the grid;
///   `per = 'beat'` without a grid is refused when it is called.
/// - `keys` (source, points required), `cycle` (source, values required): the constant 1.
/// - `broken` (source): refused whenever it is called.
/// - `gain` (shaper, `by` 0..10): its signal times `by`.
#[expect(clippy::too_many_lines, reason = "one thing a few lines each")]
fn registry() -> Result<Registry, Box<dyn std::error::Error>> {
    use InputKind::{Events, Grid as GridInput, Points, Series as SeriesInput, Values};
    let mut registry = Registry::new();
    for name in ["slew", "follow"] {
        registry.register(Registry::builtin().get(name)?.clone())?;
    }
    let level = thing(
        ("level", Kind::Source, Returns::Signal),
        vec![decimal("value", 0.0, (-10.0, 10.0))],
        &[],
        |call| Ok(Output::Signal(Signal::constant(number(call, "value"))?)),
    )?;
    let pick = thing(
        ("pick", Kind::Operation, Returns::Events),
        vec![decimal("threshold", 0.0, (0.0, 1.0))],
        &[(Events, true)],
        |call| {
            let threshold = number(call, "threshold");
            Ok(Output::Events(
                events_of(call)
                    .into_iter()
                    .filter(|event| event.strength() >= threshold)
                    .collect(),
            ))
        },
    )?;
    let hits = thing(
        ("hits", Kind::Source, Returns::Signal),
        vec![decimal("gain", 1.0, (0.0, 10.0)), choice("mode", &["hold", "once"])],
        &[(Events, true)],
        |call| {
            let (gain, events) = (number(call, "gain"), events_of(call));
            Ok(Output::Signal(Signal::from_fn(move |times, out| {
                for (value, time) in out.iter_mut().zip(times) {
                    let latest = events.iter().rev().find(|event| event.t() <= *time);
                    *value = latest.map_or(0.0, |event| gain * event.strength());
                }
                Ok::<(), Infallible>(())
            })))
        },
    )?;
    let scale = thing(
        ("scale", Kind::Operation, Returns::Series),
        vec![decimal("gain", 1.0, (0.0, 10.0))],
        &[(SeriesInput, true)],
        |call| {
            let Some(Input::Series(series)) = call.inputs.get("series") else {
                return Err(Error::Call {
                    message: "scale takes a series".to_owned(),
                });
            };
            let values = series
                .values()
                .iter()
                .map(|value| value * number(call, "gain"))
                .collect();
            let options = SeriesOptions {
                start: series.start(),
                ..SeriesOptions::default()
            };
            let scaled =
                Series::new(series.rate(), values, Unit::Ratio, options).map_err(|source| Error::Format { source })?;
            Ok(Output::Series(scaled))
        },
    )?;
    let beat = thing(
        ("beat", Kind::Source, Returns::Signal),
        vec![choice("per", &["second", "beat"])],
        &[(GridInput, false)],
        |call| {
            let grid = match call.inputs.get("grid") {
                Some(Input::Grid(grid)) => Some(grid.clone()),
                _ => None,
            };
            match (call.values["per"].as_str(), grid) {
                (Some("beat"), None) => Err(Error::Params {
                    shaper: "beat".to_owned(),
                    problems: vec![Problem {
                        path: "per".to_owned(),
                        rule: "'beat' needs a grid".to_owned(),
                    }],
                    omitted: 0,
                }),
                (Some("beat"), Some(grid)) => Ok(Output::Signal(Signal::from_fn(move |times, out| {
                    out.copy_from_slice(&grid.beat_at(times)?);
                    Ok::<(), FormatError>(())
                }))),
                _ => Ok(Output::Signal(Signal::from_fn(|times, out| {
                    out.copy_from_slice(times);
                    Ok::<(), Infallible>(())
                }))),
            }
        },
    )?;
    let keys = thing(
        ("keys", Kind::Source, Returns::Signal),
        vec![],
        &[(Points, true)],
        |_| Ok(Output::Signal(Signal::constant(1.0)?)),
    )?;
    let cycle = thing(
        ("cycle", Kind::Source, Returns::Signal),
        vec![],
        &[(Values, true)],
        |_| Ok(Output::Signal(Signal::constant(1.0)?)),
    )?;
    let broken = thing(("broken", Kind::Source, Returns::Signal), vec![], &[], |_| {
        Err(Error::Input {
            problems: vec![Problem {
                path: "depth".to_owned(),
                rule: "is too deep".to_owned(),
            }],
            omitted: 0,
        })
    })?;
    let gain = thing(
        ("gain", Kind::Shaper, Returns::Signal),
        vec![decimal("by", 1.0, (0.0, 10.0))],
        &[],
        |call| match call.signals.as_slice() {
            [input] => Ok(Output::Signal(input * number(call, "by"))),
            _ => Err(Error::Call {
                message: "gain takes a signal".to_owned(),
            }),
        },
    )?;
    for made in [level, pick, hits, scale, beat, keys, cycle, broken, gain] {
        registry.register(made)?;
    }
    Ok(registry)
}

/// The names [`registry`] holds, as an unknown name's message lists them (sorted).
const AVAILABLE: &str = "beat, broken, cycle, follow, gain, hits, keys, level, pick, scale, slew";

// ---------------------------------------------------------------- patches and bundles

/// The entries of a patch with the header and `nodes`, in order.
fn patch_of(nodes: &[(&str, Value)]) -> Vec<(String, Value)> {
    let mut entries = vec![
        ("format".to_owned(), json!("bullterrier-control/patch")),
        ("version".to_owned(), json!(1)),
    ];
    entries.extend(nodes.iter().map(|(name, node)| ((*name).to_owned(), node.clone())));
    entries
}

/// The patch in `entries`, read with [`registry`].
fn parse(entries: &[(String, Value)]) -> Result<Patch, PatchError> {
    let registry = registry().map_err(|error| PatchError::Toml {
        detail: error.to_string(),
    })?;
    Patch::parse_with(&registry, entries)
}

/// The patch in the TOML `text`, read with [`registry`].
fn from_toml(text: &str) -> Result<Patch, PatchError> {
    let registry = registry().map_err(|error| PatchError::Toml {
        detail: error.to_string(),
    })?;
    Patch::from_toml_with(&registry, text)
}

/// The problems of a refusal to read, as (path, rule); a note in place of them when the result is no such refusal.
fn problems(result: &Result<Patch, PatchError>) -> Vec<(String, String)> {
    match result {
        Err(PatchError::Invalid { problems, .. }) => problems
            .iter()
            .map(|problem| (problem.path.clone(), problem.rule.clone()))
            .collect(),
        other => vec![("not refused for problems".to_owned(), format!("{other:?}"))],
    }
}

fn kicks() -> Value {
    json!([
        {"t": 0.5, "strength": 0.9},
        {"t": 0.7, "strength": 0.45},
        {"t": 1.0, "strength": 0.8},
        {"t": 1.05, "strength": 0.95},
        {"t": 2.0, "strength": 0.7},
    ])
}

fn grid_data() -> Value {
    json!({"beats": [0.0, 0.5, 1.1, 1.8], "meter": 4, "first_downbeat": 0})
}

/// Three kicks (the middle one weak), a series `bass` from 0 to 1 over its first 0.1 s, and a grid.
fn bundle() -> Result<Bundle, FormatError> {
    let mut data = json!({
        "format": "bullterrier-control/bundle",
        "version": 1,
        "events": {"kick": [{"t": 0.5, "strength": 1.0}, {"t": 1.0, "strength": 0.6}, {"t": 1.5, "strength": 1.0}]},
        "series": {"bass": {"rate": 10, "values": [0.0, 1.0], "unit": "ratio"}},
    });
    data["grid"] = grid_data();
    Bundle::from_json(&data.to_string())
}

/// The bundle of [`bundle`] without its grid.
fn bundle_without_grid() -> Result<Bundle, FormatError> {
    Bundle::from_json(
        r#"{"format": "bullterrier-control/bundle", "version": 1,
            "events": {"kick": [{"t": 0.5}]}, "series": {"bass": {"rate": 10, "values": [0.0], "unit": "ratio"}}}"#,
    )
}

/// The signal the node `name` of `built` gave.
fn signal<'a>(built: &'a Built, name: &str) -> Result<&'a Signal, String> {
    match built.get(name) {
        Some(Output::Signal(signal)) => Ok(signal),
        other => Err(format!("{name} gave no signal: {other:?}")),
    }
}

/// Whether `got` and `want` are equal to within 1e-12 each.
fn close(got: &[f64], want: &[f64]) -> bool {
    got.len() == want.len() && got.iter().zip(want).all(|(got, want)| (got - want).abs() <= 1e-12)
}

fn names(built: &Built) -> Vec<&str> {
    built.iter().map(|(name, _)| name).collect()
}

// ---------------------------------------------------------------- building: the numbers, by hand

#[test]
fn nodes_feed_the_nodes_below_and_all_come_back_in_file_order() -> Outcome {
    let patch = parse(&patch_of(&[
        ("strong", json!({"use": "pick", "events": kicks(), "threshold": 0.5})),
        ("flash", json!({"use": "hits", "events": "strong", "gain": 0.5})),
    ]))?;
    let built = patch.build(None)?;
    assert_eq!(names(&built), ["strong", "flash"]);
    let Some(Output::Events(strong)) = built.get("strong") else {
        return Err("strong gave no events".into());
    };
    // the 0.45 kick at 0.7 s is below the threshold
    assert_eq!(strong.iter().map(Event::t).collect::<Vec<_>>(), [0.5, 1.0, 1.05, 2.0]);
    // half the latest strong kick: none at 0.4 s; 0.9 from 0.5 s, still at 0.7 s; 0.8 at 1.0; 0.95 at 1.06; 0.7 after
    let values = signal(&built, "flash")?.sample(&[0.4, 0.5, 0.7, 1.0, 1.06, 3.0])?;
    assert!(close(&values, &[0.0, 0.45, 0.45, 0.4, 0.475, 0.35]), "{values:?}");
    Ok(())
}

#[test]
fn the_events_of_a_bundle_are_named_by_their_path_in_it() -> Outcome {
    let patch = parse(&patch_of(&[
        (
            "strong",
            json!({"use": "pick", "events": "events.kick", "threshold": 0.7}),
        ),
        ("glow", json!({"use": "hits", "events": "strong"})),
    ]))?;
    // the 0.6 kick at 1.0 s is below the threshold: at 1.0 s the first kick still holds
    let values = signal(&patch.build(&bundle()?)?, "glow")?.sample(&[0.4, 0.5, 1.0, 1.5])?;
    assert_eq!(values, [0.0, 1.0, 1.0, 1.0]);
    Ok(())
}

#[test]
fn a_series_of_a_bundle_is_read_linearly_where_a_signal_is_wanted() -> Outcome {
    let patch = parse(&patch_of(&[(
        "doubled",
        json!({"use": "gain", "signal": "series.bass", "by": 2}),
    )]))?;
    // 0 at 0 s and 1 at 0.1 s: halfway at 0.05 s (a held series would read 0 there), held after the last sample
    let values = signal(&patch.build(&bundle()?)?, "doubled")?.sample(&[0.0, 0.05, 0.08, 1.0])?;
    assert!(close(&values, &[0.0, 1.0, 1.6, 2.0]), "{values:?}");
    Ok(())
}

#[test]
fn a_node_that_gives_a_series_is_read_linearly_too() -> Outcome {
    let ramp = json!({"rate": 10, "values": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10], "unit": "ratio"});
    let patch = parse(&patch_of(&[
        ("scaled", json!({"use": "scale", "series": ramp, "gain": 0.1})),
        ("followed", json!({"use": "gain", "signal": "scaled"})),
    ]))?;
    // sample i is i / 10 at i / 10 s: 0.25 s is halfway between samples 2 and 3
    let values = signal(&patch.build(None)?, "followed")?.sample(&[0.2, 0.25])?;
    assert!(close(&values, &[0.2, 0.25]), "{values:?}");
    let returns: Vec<(&str, Returns)> = patch.nodes().map(|(name, node)| (name, node.returns())).collect();
    assert_eq!(returns, [("scaled", Returns::Series), ("followed", Returns::Signal)]);
    Ok(())
}

#[test]
fn the_grid_of_a_bundle_is_named_grid_and_one_written_inline_reads_alike() -> Outcome {
    let named = parse(&patch_of(&[(
        "pulse",
        json!({"use": "beat", "per": "beat", "grid": "grid"}),
    )]))?;
    let inline = parse(&patch_of(&[
        ("pulse", json!({"use": "beat", "per": "beat", "grid": grid_data()})),
        ("level", json!({"use": "level", "value": 1})), // a TOML integer for a decimal
    ]))?;
    // beats at 0, 0.5, 1.1, 1.8 s: 0.25 s is half the first beat, 0.8 s half the second
    for built in [named.build(&bundle()?)?, inline.build(None)?] {
        let values = signal(&built, "pulse")?.sample(&[0.0, 0.25, 0.8])?;
        assert!(close(&values, &[0.0, 0.5, 1.5]), "{values:?}");
    }
    assert_eq!(signal(&inline.build(None)?, "level")?.at(3.0)?, 1.0);
    Ok(())
}

#[test]
fn each_node_says_what_it_uses_and_gives_without_building_anything() -> Outcome {
    let patch = parse(&patch_of(&[
        ("strong", json!({"use": "pick", "events": "events.kick"})),
        ("glow", json!({"use": "hits", "events": "strong"})),
        ("slow", json!({"use": "slew", "signal": "glow", "rise": 1})),
        ("ramp", json!({"use": "scale", "series": "series.bass"})),
    ]))?;
    let said: Vec<(&str, &str, Returns)> = patch
        .nodes()
        .map(|(name, node)| (name, node.uses(), node.returns()))
        .collect();
    assert_eq!(
        said,
        [
            ("strong", "pick", Returns::Events),
            ("glow", "hits", Returns::Signal),
            ("slow", "slew", Returns::Signal),
            ("ramp", "scale", Returns::Series),
        ]
    );
    Ok(())
}

#[test]
fn each_node_says_what_it_is_built_from_and_with_which_values() -> Outcome {
    use super::{Given, Reference};
    type Row<'a> = (&'a str, Option<Reference>, Vec<(InputKind, Given)>, Value);
    let patch = parse(&patch_of(&[
        (
            "strong",
            json!({"use": "pick", "events": "events.kick", "threshold": 0.5}),
        ),
        ("glow", json!({"use": "hits", "events": "strong"})),
        ("slow", json!({"use": "slew", "signal": "glow", "rise": 1})),
        ("ramp", json!({"use": "scale", "series": "series.bass", "gain": 2.5})),
        ("pulse", json!({"use": "beat", "grid": "grid"})),
        ("own", json!({"use": "beat", "grid": grid_data(), "per": "beat"})),
        ("cycle", json!({"use": "cycle", "values": [1, 2.5]})),
        ("plain", json!({"use": "level"})),
    ]))?;
    let read = |name: &str| patch.nodes().find(|(found, _)| *found == name).map(|(_, node)| node);
    let node = |name: &str| read(name).ok_or(format!("no node {name}"));
    let reference = |name: &str| Given::Reference(Reference::Events(name.to_owned()));
    // (node, its signal, its inputs, its values)
    let rows: Vec<Row> = vec![
        (
            "strong",
            None,
            vec![(InputKind::Events, reference("kick"))],
            json!({"threshold": 0.5}),
        ),
        (
            "glow",
            None,
            vec![(
                InputKind::Events,
                Given::Reference(Reference::Node("strong".to_owned())),
            )],
            json!({}),
        ),
        (
            "slow",
            Some(Reference::Node("glow".to_owned())),
            vec![],
            json!({"rise": 1}),
        ),
        (
            "ramp",
            None,
            vec![(
                InputKind::Series,
                Given::Reference(Reference::Series("bass".to_owned())),
            )],
            json!({"gain": 2.5}),
        ),
        (
            "pulse",
            None,
            vec![(InputKind::Grid, Given::Reference(Reference::Grid))],
            json!({}),
        ),
        (
            "cycle",
            None,
            vec![(InputKind::Values, Given::Data(Input::Values(vec![1.0, 2.5])))],
            json!({}),
        ),
        ("plain", None, vec![], json!({})),
    ];
    for (name, signal, inputs, values) in rows {
        let node = node(name)?;
        assert_eq!(node.signal(), signal.as_ref(), "{name}: signal");
        assert_eq!(
            node.inputs().collect::<Vec<_>>(),
            inputs.iter().map(|(k, g)| (*k, g)).collect::<Vec<_>>(),
            "{name}: inputs"
        );
        assert_eq!(Value::Object(node.values().clone()), values, "{name}: values");
    }
    // data written inline is data, checked as its kind
    let own = node("own")?;
    let inputs: Vec<_> = own.inputs().collect();
    assert!(
        matches!(inputs.as_slice(), [(InputKind::Grid, Given::Data(Input::Grid(grid)))] if grid.beats() == [0.0, 0.5, 1.1, 1.8])
    );
    assert_eq!(own.values().get("per"), Some(&json!("beat")));
    // a reference is shown as a patch writes it
    let shown: Vec<String> = [
        Reference::Node("glow".to_owned()),
        Reference::Events("kick".to_owned()),
        Reference::Series("bass".to_owned()),
        Reference::Grid,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    assert_eq!(shown, ["glow", "events.kick", "series.bass", "grid"]);
    Ok(())
}

#[test]
fn building_twice_gives_the_same_values() -> Outcome {
    let patch = parse(&patch_of(&[(
        "glow",
        json!({"use": "hits", "events": kicks(), "gain": 0.25}),
    )]))?;
    let times = [0.4, 0.5, 0.9, 3.0];
    assert_eq!(
        signal(&patch.build(None)?, "glow")?.sample(&times)?,
        signal(&patch.build(None)?, "glow")?.sample(&times)?
    );
    Ok(())
}

#[test]
fn a_patch_with_no_node_is_valid_and_builds_nothing() -> Outcome {
    let built = parse(&patch_of(&[]))?.build(None)?;
    assert!(built.is_empty());
    assert_eq!(built.len(), 0);
    Ok(())
}

// ---------------------------------------------------------------- building only some nodes

fn needs_a_bundle_and_not() -> Vec<(String, Value)> {
    patch_of(&[
        ("strong", json!({"use": "pick", "events": "events.kick"})),
        ("glow", json!({"use": "hits", "events": "strong"})),
        ("wave", json!({"use": "level", "value": 0.25})),
        ("never", json!({"use": "broken"})),
    ])
}

#[test]
fn building_some_nodes_builds_only_those_and_what_they_use() -> Outcome {
    let patch = parse(&needs_a_bundle_and_not())?;
    // `strong` needs a bundle and `never` fails when built: neither is built for `wave`
    let built = patch.build_only(None, &["wave"])?;
    assert_eq!(names(&built), ["wave"]);
    assert_eq!(signal(&built, "wave")?.at(1.0)?, 0.25);
    // what `glow` uses is built (it needs the bundle), but only `glow` comes back
    assert_eq!(names(&patch.build_only(&bundle()?, &["glow"])?), ["glow"]);
    let refused = patch
        .build_only(None, &["glow"])
        .map(|_| ())
        .map_err(|error| error.to_string());
    assert_eq!(
        refused,
        Err("strong.events: 'events.kick' is a path into a bundle, and none was given".to_owned())
    );
    Ok(())
}

#[test]
fn the_nodes_asked_for_come_back_in_file_order() -> Outcome {
    let built = parse(&needs_a_bundle_and_not())?.build_only(&bundle()?, &["wave", "strong"])?;
    assert_eq!(names(&built), ["strong", "wave"]);
    Ok(())
}

#[test]
fn asking_for_a_node_the_patch_lacks_names_it_and_what_the_patch_has() -> Outcome {
    let refused = parse(&needs_a_bundle_and_not())?.build_only(None, &["wave", "nosuch"]);
    let Err(error @ PatchError::UnknownNode { .. }) = refused else {
        return Err(format!("not an unknown node: {refused:?}").into());
    };
    assert_eq!(
        error.to_string(),
        "unknown node 'nosuch'; the patch has: strong, glow, wave, never"
    );
    Ok(())
}

// ---------------------------------------------------------------- building: what the bundle must hold

#[test]
fn a_reference_the_bundle_cannot_serve_is_refused_at_its_node_and_key() -> Outcome {
    // (row, node, the bundle given, the refusal)
    let rows: [(&str, Value, Option<Bundle>, &str); 4] = [
        (
            "no bundle given",
            json!({"use": "hits", "events": "events.kick"}),
            None,
            "glow.events: 'events.kick' is a path into a bundle, and none was given",
        ),
        (
            "a name the bundle has no events for",
            json!({"use": "hits", "events": "events.snare"}),
            Some(bundle()?),
            "glow.events: the bundle has no events 'snare' (it has: kick)",
        ),
        (
            "a name the bundle has no series for",
            json!({"use": "gain", "signal": "series.treble"}),
            Some(bundle()?),
            "glow.signal: the bundle has no series 'treble' (it has: bass)",
        ),
        (
            "a bundle without a grid where a grid is named",
            json!({"use": "beat", "per": "beat", "grid": "grid"}),
            Some(bundle_without_grid()?),
            "glow.grid: the bundle has no grid",
        ),
    ];
    for (row, node, given, expected) in rows {
        let refused = parse(&patch_of(&[("glow", node)]))?.build(given.as_ref());
        let Err(error @ PatchError::Bundle { .. }) = refused else {
            return Err(format!("{row}: not refused for the bundle: {refused:?}").into());
        };
        assert_eq!(error.to_string(), expected, "{row}");
    }
    Ok(())
}

#[test]
fn an_error_a_thing_gives_when_built_is_kept_and_names_the_node() -> Outcome {
    // `per = 'beat'` needs a grid: the thing says so when called, so reading cannot refuse it (the rule is its own)
    let rows = [
        ("low", json!({"use": "broken"}), "low: depth: is too deep"),
        (
            "a",
            json!({"use": "beat", "per": "beat"}),
            r#"a: the parameters of "beat" do not fit: per: 'beat' needs a grid"#,
        ),
    ];
    for (name, node, expected) in rows {
        let refused = parse(&patch_of(&[(name, node)]))?.build(None);
        let Err(error @ PatchError::Node { .. }) = &refused else {
            return Err(format!("{name}: not a thing's error: {refused:?}").into());
        };
        assert_eq!(error.to_string(), expected);
        let kept = match error {
            PatchError::Node {
                node,
                source: Error::Input { problems, .. } | Error::Params { problems, .. },
            } => (node.as_str(), problems[0].path.as_str()),
            _ => ("", ""),
        };
        assert_eq!(kept, (name, if name == "low" { "depth" } else { "per" }));
    }
    Ok(())
}

// ---------------------------------------------------------------- reading: every rule, with the path it names

fn level() -> Value {
    json!({"use": "level"})
}

/// A row of [`a_broken_patch_lists_every_problem_at_its_node_and_key`]: (row, the entries, the problems).
type Bad<'a> = (&'a str, Vec<(String, Value)>, Vec<(&'a str, &'a str)>);

#[test]
#[expect(clippy::too_many_lines, reason = "one row per rule, good and bad")]
fn a_broken_patch_lists_every_problem_at_its_node_and_key() {
    let header = |format: Option<Value>, version: Option<Value>| {
        let mut entries: Vec<(String, Value)> = Vec::new();
        entries.extend(format.map(|format| ("format".to_owned(), format)));
        entries.extend(version.map(|version| ("version".to_owned(), version)));
        entries.push(("level".to_owned(), level()));
        entries
    };
    let format = || Some(json!("bullterrier-control/patch"));
    let below = "is a node below this one; a node uses only the nodes above it";
    let node_name = "is not a node name: lower case letters, digits and _, starting with a letter, and not one of \
                     format, version, events, series, grid";
    let (later, itself) = (format!("'b' {below}"), format!("'a' {below}"));
    let rows: Vec<Bad<'_>> = vec![
        (
            "a good patch",
            patch_of(&[("level", level()), ("glow", json!({"use": "gain", "signal": "level"}))]),
            vec![],
        ),
        (
            "an unknown source",
            patch_of(&[("a", json!({"use": "nosuch"}))]),
            vec![(
                "a.use",
                "unknown source, shaper or operation 'nosuch'; available: beat, broken, cycle, follow, gain, hits, keys, level, pick, scale, slew",
            )],
        ),
        (
            "a use that is not text",
            patch_of(&[("a", json!({"use": 3}))]),
            vec![(
                "a.use",
                "must be text, the name of a source, shaper or operation (got 3)",
            )],
        ),
        (
            "no use",
            patch_of(&[("a", json!({"value": 1}))]),
            vec![("a.use", "is required: the name of a source, shaper or operation")],
        ),
        (
            "a node that is not a table",
            patch_of(&[("glow", json!(5))]),
            vec![("glow", "must be a node: a table with `use`")],
        ),
        (
            "a signal given to a source",
            patch_of(&[("level", level()), ("a", json!({"use": "level", "signal": "level"}))]),
            vec![("a.signal", "source 'level' takes no signal")],
        ),
        (
            "a shaper without its signal",
            patch_of(&[("a", json!({"use": "slew"}))]),
            vec![("a.signal", "is required: a shaper takes a signal")],
        ),
        (
            "a signal that is not a reference",
            patch_of(&[("a", json!({"use": "slew", "signal": 1}))]),
            vec![("a.signal", "must be a reference: a node above, or series.<name>")],
        ),
        (
            "a reference to a later node",
            patch_of(&[
                ("a", json!({"use": "hits", "events": "b"})),
                ("b", json!({"use": "pick", "events": "events.kick"})),
            ]),
            vec![("a.events", &later)],
        ),
        (
            "a reference to the node itself",
            patch_of(&[("a", json!({"use": "hits", "events": "a"}))]),
            vec![("a.events", &itself)],
        ),
        (
            "a reference to a node that does not exist",
            patch_of(&[("level", level()), ("a", json!({"use": "hits", "events": "nothing"}))]),
            vec![(
                "a.events",
                "'nothing' is no node above this one (nodes above: level) and no bundle path (events.<name>, series.<name>, grid)",
            )],
        ),
        (
            "a reference with no node above",
            patch_of(&[("a", json!({"use": "hits", "events": "nothing"}))]),
            vec![(
                "a.events",
                "'nothing' is no node above this one (nodes above: none) and no bundle path (events.<name>, series.<name>, grid)",
            )],
        ),
        (
            "a node where events are wanted that gives a signal",
            patch_of(&[("level", level()), ("a", json!({"use": "hits", "events": "level"}))]),
            vec![("a.events", "'level' gives signal, but events takes events")],
        ),
        (
            "a node where a signal is wanted that gives events",
            patch_of(&[
                ("strong", json!({"use": "pick", "events": "events.kick"})),
                ("a", json!({"use": "slew", "signal": "strong"})),
            ]),
            vec![("a.signal", "'strong' gives events, but signal takes signal or series")],
        ),
        (
            "bundle events where a grid is wanted",
            patch_of(&[("a", json!({"use": "beat", "grid": "events.kick"}))]),
            vec![("a.grid", "'events.kick' gives events, but grid takes grid")],
        ),
        (
            "a bundle series where a grid is wanted",
            patch_of(&[("a", json!({"use": "beat", "grid": "series.bass"}))]),
            vec![("a.grid", "'series.bass' gives series, but grid takes grid")],
        ),
        (
            "a bundle grid where events are wanted",
            patch_of(&[("a", json!({"use": "hits", "events": "grid"}))]),
            vec![("a.events", "'grid' gives grid, but events takes events")],
        ),
        (
            "a bundle series where events are wanted",
            patch_of(&[("a", json!({"use": "hits", "events": "series.bass"}))]),
            vec![("a.events", "'series.bass' gives series, but events takes events")],
        ),
        (
            "a bundle series where a signal is wanted",
            patch_of(&[("a", json!({"use": "slew", "signal": "series.bass"}))]),
            vec![],
        ),
        (
            "a bundle name that no bundle could hold",
            patch_of(&[("a", json!({"use": "hits", "events": "events.Kick"}))]),
            vec![(
                "a.events",
                "'events.Kick' is not a path into a bundle: events.<name>, the name in lower case",
            )],
        ),
        (
            "a bundle path with no name",
            patch_of(&[("a", json!({"use": "hits", "events": "events."}))]),
            vec![(
                "a.events",
                "'events.' is not a path into a bundle: events.<name>, the name in lower case",
            )],
        ),
        (
            "a bundle path with no dot",
            patch_of(&[("a", json!({"use": "scale", "series": "series"}))]),
            vec![(
                "a.series",
                "'series' is not a path into a bundle: series.<name>, the name in lower case",
            )],
        ),
        (
            "a name after grid",
            patch_of(&[("a", json!({"use": "beat", "grid": "grid.x"}))]),
            vec![(
                "a.grid",
                "'grid.x' is no node above this one (nodes above: none) and no bundle path (events.<name>, series.<name>, grid)",
            )],
        ),
        (
            "an input the thing does not take",
            patch_of(&[("a", json!({"use": "level", "events": "events.kick"}))]),
            vec![(
                "a.events",
                "is neither an input nor a parameter of source 'level' (it takes value)",
            )],
        ),
        (
            "a key a thing of nothing does not take",
            patch_of(&[("a", json!({"use": "broken", "depth": 1}))]),
            vec![(
                "a.depth",
                "is neither an input nor a parameter of source 'broken' (it takes nothing)",
            )],
        ),
        (
            "a required input left out",
            patch_of(&[("a", json!({"use": "hits"}))]),
            vec![("a.events", "is required: 'hits' takes events")],
        ),
        (
            "an optional input left out",
            patch_of(&[("a", json!({"use": "beat"}))]),
            vec![],
        ),
        (
            "a key that is neither input nor parameter",
            patch_of(&[("glow", json!({"use": "hits", "events": kicks(), "target": "radius"}))]),
            vec![(
                "glow.target",
                "is neither an input nor a parameter of source 'hits' (it takes events, gain, mode)",
            )],
        ),
        (
            "a parameter beyond its range",
            patch_of(&[("glow", json!({"use": "hits", "events": kicks(), "gain": -1}))]),
            vec![("glow.gain", "must be >= 0.0 (got -1)")],
        ),
        (
            "a bool for a number",
            patch_of(&[("glow", json!({"use": "hits", "events": kicks(), "gain": true}))]),
            vec![("glow.gain", "must be a number (got true)")],
        ),
        (
            "a text for a number",
            patch_of(&[("glow", json!({"use": "hits", "events": kicks(), "gain": "0.25"}))]),
            vec![("glow.gain", r#"must be a number (got "0.25")"#)],
        ),
        (
            "a choice not offered",
            patch_of(&[("glow", json!({"use": "hits", "events": kicks(), "mode": "wobbly"}))]),
            vec![("glow.mode", r#"must be one of "hold", "once" (got "wobbly")"#)],
        ),
        (
            "inline events with a bad strength",
            patch_of(&[(
                "glow",
                json!({"use": "hits", "events": [{"t": 0}, {"t": 1, "strength": 5}]}),
            )]),
            vec![("glow.events[1].strength", "must be in [0, 1] (got 5.0)")],
        ),
        (
            "inline keyframes that do not increase",
            patch_of(&[(
                "a",
                json!({"use": "keys", "points": [{"t": 1, "value": 0}, {"t": 1, "value": 1}]}),
            )]),
            vec![("a.points[1].t", "1.0 is not after the previous t = 1.0")],
        ),
        (
            "a node named grid",
            patch_of(&[("grid", level())]),
            vec![("grid", node_name)],
        ),
        (
            "a node named events",
            patch_of(&[("events", level())]),
            vec![("events", node_name)],
        ),
        (
            "a node named series",
            patch_of(&[("series", level())]),
            vec![("series", node_name)],
        ),
        (
            "a node name in capitals",
            patch_of(&[("Glow", level())]),
            vec![("Glow", node_name)],
        ),
        (
            "a node name starting with a digit",
            patch_of(&[("1st", level())]),
            vec![("1st", node_name)],
        ),
        (
            "a node name beyond ASCII",
            patch_of(&[("é", level())]),
            vec![("é", node_name)],
        ),
        (
            "a node named twice",
            patch_of(&[("a", level()), ("a", level())]),
            vec![("a", "is the name of a node above: each node has a name of its own")],
        ),
        (
            "a top-level value that is not a node",
            patch_of(&[("drives", json!("glow_radius")), ("level", level())]),
            vec![("drives", "must be a node: a table with `use`")],
        ),
        (
            "no format",
            header(None, Some(json!(1))),
            vec![("format", "is required: 'bullterrier-control/patch'")],
        ),
        (
            "another format",
            header(Some(json!("bullterrier-control/bundle")), Some(json!(1))),
            vec![(
                "format",
                "must be 'bullterrier-control/patch' (got 'bullterrier-control/bundle')",
            )],
        ),
        (
            "no version",
            header(format(), None),
            vec![("version", "is required: 1")],
        ),
        (
            "a version this reader does not know",
            header(format(), Some(json!(2))),
            vec![(
                "version",
                "must be the whole number 1, the one version this reader knows (got 2)",
            )],
        ),
        (
            "a version written as a decimal",
            header(format(), Some(json!(1.0))),
            vec![(
                "version",
                "must be the whole number 1, the one version this reader knows (got 1.0)",
            )],
        ),
        (
            "a version that is true",
            header(format(), Some(json!(true))),
            vec![(
                "version",
                "must be the whole number 1, the one version this reader knows (got True)",
            )],
        ),
    ];
    for (row, entries, expected) in rows {
        let result = parse(&entries);
        let got = if result.is_ok() { vec![] } else { problems(&result) };
        let expected: Vec<(String, String)> = expected
            .iter()
            .map(|(path, rule)| ((*path).to_owned(), (*rule).to_owned()))
            .collect();
        assert_eq!(got, expected, "{row}");
    }
}

#[test]
fn a_reference_where_only_data_goes_is_read_as_data_and_refused() {
    // keyframes and values are written inline: a text there is data of the wrong type (the inputs' own rule)
    for (row, node, path) in [
        ("keyframes", json!({"use": "keys", "points": "events.kick"}), "a.points"),
        ("values", json!({"use": "cycle", "values": "kicks"}), "a.values"),
    ] {
        let found = problems(&parse(&patch_of(&[("a", node)])));
        assert!(
            found.len() == 1 && found[0].0 == path && found[0].1.starts_with("invalid type: string"),
            "{row}: {found:?}"
        );
    }
}

#[test]
fn every_problem_is_listed_not_only_the_first() {
    let found = problems(&parse(&patch_of(&[
        ("a", json!({"use": "nosuch"})),
        ("b", json!({"use": "hits", "events": "events.Kick", "gain": -1})),
        ("Loud", level()),
    ])));
    let paths: Vec<&str> = found.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(paths, ["a.use", "b.events", "b.gain", "Loud"]);
}

#[test]
fn past_100_problems_the_first_are_listed_and_the_rest_counted() {
    // (row, nodes, problems kept, problems counted): one unknown name, then what the inputs' check keeps of 250
    // broken values (100) and what it counts (150); or 150 unknown keys of one node
    let values = vec![json!(true); 250];
    let keys: serde_json::Map<String, Value> = (0..150)
        .map(|index| (format!("key_{index:03}"), json!(1)))
        .chain([("use".to_owned(), json!("level"))])
        .collect();
    let rows = [
        (
            "problems the inputs omit",
            vec![
                ("z", json!({"use": "nosuch"})),
                ("a", json!({"use": "cycle", "values": values})),
            ],
            151,
        ),
        ("problems of one node", vec![("a", Value::Object(keys))], 50),
    ];
    for (row, nodes, omitted) in rows {
        let refused = parse(&patch_of(&nodes));
        let Err(
            error @ PatchError::Invalid {
                problems,
                omitted: counted,
            },
        ) = &refused
        else {
            assert!(refused.is_err(), "{row}: read");
            continue;
        };
        assert_eq!((problems.len(), *counted), (100, omitted), "{row}");
        assert!(
            error.to_string().ends_with(&format!("; ... and {omitted} more")),
            "{row}"
        );
    }
}

#[test]
fn an_unknown_source_lists_what_is_available() {
    let found = problems(&parse(&patch_of(&[("a", json!({"use": "levl"}))])));
    assert!(found[0].1.ends_with(&format!("available: {AVAILABLE}")), "{found:?}");
}

#[test]
fn a_node_that_follows_a_node_with_a_broken_use_adds_no_second_complaint() {
    // what the broken node would have given is unknown, so no reference to it can be judged
    for follower in [
        json!({"use": "gain", "signal": "a"}),
        json!({"use": "hits", "events": "a"}),
        json!({"use": "scale", "series": "a"}),
    ] {
        let found = problems(&parse(&patch_of(&[
            ("a", json!({"use": "nosuch"})),
            ("b", follower.clone()),
        ])));
        let paths: Vec<&str> = found.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(paths, ["a.use"], "{follower}");
    }
}

#[test]
fn the_keys_of_a_patch_are_signals_only_never_a_target() {
    // mapping a signal onto a parameter of something else is a consumer's job (video's): a patch cannot say it
    for key in ["target", "drives", "maps_to", "range"] {
        let mut node = json!({"use": "hits", "events": kicks()});
        node[key] = json!("glow_radius");
        let found = problems(&parse(&patch_of(&[("glow", node)])));
        assert_eq!(found.len(), 1, "{key}");
        assert_eq!(found[0].0, format!("glow.{key}"));
    }
}

// ---------------------------------------------------------------- the TOML text

const GLOW: &str = r#"format = "bullterrier-control/patch"
version = 1

[strong]
use = "pick"
events = "events.kick"   # the bundle's events "kick"
threshold = 0.7

[glow]
use = "hits"
events = "strong"        # the node above
gain = 0.5
"#;

#[test]
fn a_toml_text_is_read_in_file_order_and_builds_against_a_bundle() -> Outcome {
    let built = from_toml(GLOW)?.build(&bundle()?)?;
    assert_eq!(names(&built), ["strong", "glow"]); // not sorted by name
    // half the latest kick of 0.7 or more: the 0.6 kick at 1.0 s is dropped
    assert_eq!(
        signal(&built, "glow")?.sample(&[0.4, 0.5, 1.0, 1.5])?,
        [0.0, 0.5, 0.5, 0.5]
    );
    Ok(())
}

#[test]
fn a_node_is_where_the_text_first_names_it_even_in_a_table_header_below_it() -> Outcome {
    // `[pulse.grid]` names `pulse` before `slow`; its own `[pulse]` header comes later (as Python's tomllib orders it)
    let text = r#"format = "bullterrier-control/patch"
version = 1

[pulse.grid]
beats = [0.0, 0.5, 1.1, 1.8]
meter = 4
first_downbeat = 0

[slow]
use = "slew"
signal = "pulse"
rise = 1

[pulse]
use = "beat"
per = "beat"
"#;
    let patch = from_toml(text)?;
    assert_eq!(
        patch.nodes().map(|(name, _)| name).collect::<Vec<_>>(),
        ["pulse", "slow"]
    );
    // beats at 0, 0.5, 1.1, 1.8 s: 0.25 s is half the first beat, 0.8 s half the second (the grid was read)
    let values = signal(&patch.build(None)?, "pulse")?.sample(&[0.25, 0.8])?;
    assert!(close(&values, &[0.5, 1.5]), "{values:?}");
    // a header two tables below (`[slow.x.y]`) names `slow` first, before `[slow.x]` and `[slow]` do
    let deeper = "format = 'bullterrier-control/patch'\nversion = 1\n[slow.x.y]\n[level]\nuse = 'level'\n\
                  [slow.x]\n[slow]\nuse = 'slew'\nsignal = 'level'\n";
    let paths: Vec<String> = problems(&from_toml(deeper)).into_iter().map(|(path, _)| path).collect();
    assert_eq!(paths, ["slow.signal", "slow.x"]);
    Ok(())
}

#[test]
fn a_toml_text_and_the_same_table_as_data_are_one_patch() -> Outcome {
    let entries = patch_of(&[
        (
            "strong",
            json!({"use": "pick", "events": "events.kick", "threshold": 0.7}),
        ),
        ("glow", json!({"use": "hits", "events": "strong", "gain": 0.5})),
    ]);
    assert_eq!(from_toml(GLOW)?, parse(&entries)?);
    let other = patch_of(&[
        (
            "strong",
            json!({"use": "pick", "events": "events.kick", "threshold": 0.7}),
        ),
        ("glow", json!({"use": "hits", "events": "strong", "gain": 0.25})),
    ]);
    assert_ne!(from_toml(GLOW)?, parse(&other)?);
    Ok(())
}

#[test]
fn a_text_that_is_not_toml_is_refused_with_the_line_and_column() {
    // (row, text, the start of the refusal): the parser's own words follow the position
    let rows = [
        (
            "a value left out",
            "format = \n".to_owned(),
            "not TOML (line 1, column 10: ",
        ),
        (
            "a repeated table",
            format!("{GLOW}\n[glow]\nuse = \"level\"\n"),
            "not TOML (line 14, column 2: duplicate key",
        ),
        (
            "a repeated key",
            "format = 1\nformat = 2\n".to_owned(),
            "not TOML (line 2, column 1: duplicate key",
        ),
        (
            "arrays nested deeper than the parser follows",
            format!("x = {}{}\n", "[".repeat(5000), "]".repeat(5000)),
            "not TOML (line 1, column ",
        ),
        (
            "a position counted in characters",
            "\"é\" = 1\n\"ü\" = \n".to_owned(),
            "not TOML (line 2, column 7: ",
        ),
    ];
    for (row, text, start) in rows {
        let refused = from_toml(&text).map(|_| ()).map_err(|error| error.to_string());
        assert!(
            refused.as_ref().is_err_and(|text| text.starts_with(start)),
            "{row}: {refused:?}"
        );
    }
}

/// A row of [`a_value_toml_holds_and_a_patch_cannot_is_one_problem_at_its_key`]: (row, text, the problems).
type Spoiled<'a> = (&'a str, String, Vec<(&'a str, String)>);

#[test]
fn a_value_toml_holds_and_a_patch_cannot_is_one_problem_at_its_key() {
    let node = |value: &str| {
        format!("format = 'bullterrier-control/patch'\nversion = 1\n[a]\nuse = 'level'\nvalue = {value}\n")
    };
    let digits = format!("1{}", "0".repeat(5000));
    let shown = format!("1{}...", "0".repeat(79)); // 80 characters of it
    let rows: Vec<Spoiled<'_>> = vec![
        (
            "not a number",
            node("nan"),
            vec![("a.value", "must be finite (got nan)".to_owned())],
        ),
        (
            "an infinity",
            node("-inf"),
            vec![("a.value", "must be finite (got -inf)".to_owned())],
        ),
        (
            "a decimal beyond f64",
            node("1e400"),
            vec![("a.value", "must be finite (got 1e400)".to_owned())],
        ),
        (
            "a date",
            node("1979-05-27"),
            vec![(
                "a.value",
                "is a date or time (1979-05-27), which no input or parameter takes".to_owned(),
            )],
        ),
        (
            "a whole number of thousands of digits",
            node(&digits),
            vec![(
                "a.value",
                format!("must be a whole number within 64 bits (got {shown})"),
            )],
        ),
        (
            "a whole number just beyond 64 bits",
            node("9223372036854775808"),
            vec![(
                "a.value",
                "must be a whole number within 64 bits (got 9223372036854775808)".to_owned(),
            )],
        ),
        (
            "the largest whole number of 64 bits is read (and is beyond the range)",
            node("9223372036854775807"),
            vec![("a.value", "must be <= 10.0 (got 9223372036854775807)".to_owned())],
        ),
        ("a hexadecimal whole number", node("0x0a"), vec![]),
        (
            "an infinity in inline data",
            "format = 'bullterrier-control/patch'\nversion = 1\n[a]\nuse = 'hits'\nevents = [{t = 0}, {t = inf}]\n"
                .to_owned(),
            vec![("a.events[1].t", "must be finite (got inf)".to_owned())],
        ),
        (
            "a date for the version, with a broken node",
            "format = 'bullterrier-control/patch'\nversion = 2026-10-31\n[a]\nuse = 'nosuch'\n".to_owned(),
            vec![
                (
                    "version",
                    "is a date or time (2026-10-31), which no input or parameter takes".to_owned(),
                ),
                (
                    "a.use",
                    format!("unknown source, shaper or operation 'nosuch'; available: {AVAILABLE}"),
                ),
            ],
        ),
    ];
    for (row, text, expected) in rows {
        let result = from_toml(&text);
        let got = if result.is_ok() { vec![] } else { problems(&result) };
        let expected: Vec<(String, String)> = expected
            .into_iter()
            .map(|(path, rule)| (path.to_owned(), rule))
            .collect();
        assert_eq!(got, expected, "{row}");
    }
}

#[test]
fn hostile_text_is_refused_without_trouble() -> Outcome {
    let long = "x".repeat(1_000_000);
    let loud = "X".repeat(1_000_000);
    let header = "format = 'bullterrier-control/patch'\nversion = 1\n";
    // a name of a million lower case letters is a name
    let read = from_toml(&format!("{header}[{long}]\nuse = 'level'\n"))?;
    assert_eq!(
        read.nodes().map(|(name, _)| name.len()).collect::<Vec<_>>(),
        [1_000_000]
    );
    // (row, text, the path of the one problem)
    let rows = [
        (
            "a million-character use",
            format!("{header}[a]\nuse = '{long}'\n"),
            "a.use",
        ),
        (
            "a million-character node name in capitals",
            format!("{header}[{loud}]\nuse = 'level'\n"),
            loud.as_str(),
        ),
        (
            "a million-character reference",
            format!("{header}[a]\nuse = 'hits'\nevents = '{long}'\n"),
            "a.events",
        ),
        (
            "a key beyond ASCII",
            format!("{header}[a]\nuse = 'level'\n\"dé\" = 1\n"),
            "a.dé",
        ),
    ];
    for (row, text, path) in rows {
        let found = problems(&from_toml(&text));
        assert!(
            found.len() == 1 && found[0].0 == path,
            "{row}: {} problems",
            found.len()
        );
    }
    // a reference's text is cut short in the message
    let found = problems(&from_toml(&format!("{header}[a]\nuse = 'hits'\nevents = '{long}'\n")));
    let cut = format!("'{}...' is no node above", "x".repeat(80));
    assert!(found[0].1.starts_with(&cut), "{} characters", found[0].1.len());
    Ok(())
}

#[test]
fn the_nodes_above_a_broken_reference_are_listed_cut_short() {
    // (row, how many nodes above, the names a message lists): a name of 8 characters and `, ` are 10, so 80 characters
    // hold 8 names; past them the list ends in `...`
    let rows = [
        (
            "eight names fit",
            8,
            "level_00, level_01, level_02, level_03, level_04, level_05, level_06, level_07",
        ),
        (
            "a ninth is cut",
            9,
            "level_00, level_01, level_02, level_03, level_04, level_05, level_06, level_07, ...",
        ),
    ];
    for (row, above, listed) in rows {
        let mut nodes: Vec<(String, Value)> = (0..above).map(|index| (format!("level_{index:02}"), level())).collect();
        nodes.push(("a".to_owned(), json!({"use": "hits", "events": "zz"})));
        let nodes: Vec<(&str, Value)> = nodes.iter().map(|(name, node)| (name.as_str(), node.clone())).collect();
        let found = problems(&parse(&patch_of(&nodes)));
        let expected = format!(
            "'zz' is no node above this one (nodes above: {listed}) and no bundle path (events.<name>, series.<name>, grid)"
        );
        assert_eq!(found, [("a.events".to_owned(), expected)], "{row}");
    }
    // so a patch of many nodes, each a broken reference, keeps its message small
    let nodes: Vec<(String, Value)> = (0..20_000)
        .map(|index| (format!("n{index}"), json!({"use": "hits", "events": "zz"})))
        .collect();
    let nodes: Vec<(&str, Value)> = nodes.iter().map(|(name, node)| (name.as_str(), node.clone())).collect();
    let found = problems(&parse(&patch_of(&nodes)));
    let longest = found.iter().map(|(_, rule)| rule.len()).max().unwrap_or(0);
    assert!(
        found.len() == 100 && longest < 200,
        "{} problems, the longest {longest} bytes",
        found.len()
    );
}

// ---------------------------------------------------------------- the schema

#[test]
fn the_schema_states_the_header_and_the_shape_of_a_node() {
    // the words of the Python package's `patch_schema`, by hand
    let schema = patch_schema();
    assert_eq!(schema["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert_eq!((&schema["title"], &schema["type"]), (&json!("Patch"), &json!("object")));
    assert_eq!(schema["required"], json!(["format", "version"]));
    assert_eq!(
        schema["properties"],
        json!({"format": {"const": "bullterrier-control/patch"}, "version": {"const": 1}})
    );
    assert_eq!(
        schema["patternProperties"],
        json!({"^(?!(format|version|events|series|grid)$)[a-z][a-z0-9_]*$": {"$ref": "#/$defs/node"}})
    );
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(
        schema["$defs"]["node"],
        json!({
            "type": "object",
            "required": ["use"],
            "properties": {"use": {"type": "string", "pattern": "^[a-z][a-z0-9_]*$"}, "signal": {"type": "string"}},
            "additionalProperties": {"type": ["number", "string", "boolean", "array", "object"]},
        })
    );
}

// ---------------------------------------------------------------- no Python: a patch read, built and sampled

#[test]
fn a_toml_patch_of_the_built_ins_builds_and_samples_with_rust_alone() -> Outcome {
    let text = r#"
        format = "bullterrier-control/patch"
        version = 1

        [smooth]
        use = "slew"
        signal = "series.energy"
        rise = 0.5
        fall = 0.25
        rate = 10

        [glow]
        use = "follow"
        signal = "smooth"
        attack = 0.1
        release = 0
        rate = 10
    "#;
    let bundle = Bundle::from_json(
        r#"{"format": "bullterrier-control/bundle", "version": 1,
            "series": {"energy": {"rate": 10, "values": [0, 1, 1, 1, 0, 0], "unit": "ratio"}}}"#,
    )?;
    let built = Patch::from_toml(text)?.build(&bundle)?;
    // slew at 10 samples a second: up 0.1 / 0.5 = 0.2 a sample, down 0.1 / 0.25 = 0.4: 0, 0.2, 0.4, 0.6, 0.2, 0
    let smooth = signal(&built, "smooth")?.sample(&[0.05, 0.15, 0.25, 0.35, 0.45, 0.55])?;
    assert!(close(&smooth, &[0.0, 0.2, 0.4, 0.6, 0.2, 0.0]), "{smooth:?}");
    // follow: a climb keeps e^-1 of its level (attack 0.1 s, a sample 0.1 s) and takes the rest from the input; a fall
    // (release 0) is the input. 0; 0.2 (1 - 1/e); that / e + 0.4 (1 - 1/e); that / e + 0.6 (1 - 1/e); 0.2; 0
    let glow = signal(&built, "glow")?.sample(&[-1.0, 0.05, 0.15, 0.25, 0.35, 0.45, 0.55, 9.0])?;
    let want = [
        0.0,
        0.0,
        0.126_424_111_765_711_53,
        0.299_357_055_118_389,
        0.489_399_641_444_816_24,
        0.2,
        0.0,
        0.0,
    ];
    assert!(close(&glow, &want), "{glow:?}");
    Ok(())
}

#[test]
fn a_toml_patch_makes_a_constant_and_selects_bundle_events_with_rust_alone() -> Outcome {
    let text = r#"
        format = "bullterrier-control/patch"
        version = 1

        [level]
        use = "constant"
        value = -2.5

        [strong]
        use = "select"
        events = "events.kick"
        threshold = 0.7
    "#;
    let built = Patch::from_toml(text)?.build(&bundle()?)?;
    assert_eq!(signal(&built, "level")?.sample(&[0.0, 7.0])?, [-2.5, -2.5]);
    let Some(Output::Events(strong)) = built.get("strong") else {
        return Err("select gave no events".into());
    };
    // the kicks at 0.5 s (1.0), 1.0 s (0.6) and 1.5 s (1.0): the weak one is dropped
    assert_eq!(strong.iter().map(Event::t).collect::<Vec<_>>(), [0.5, 1.5]);
    Ok(())
}

// ---------------------------------------------------------------- property: a patch builds what calls by hand build

/// A node of a generated chain: a shaper and its values (some may be out of range).
#[derive(Clone, Debug)]
enum Step {
    Slew { rise: f64, fall: f64 },
    Follow { attack: f64, release: f64 },
    Gain { by: f64 },
}

impl Step {
    fn node(&self, signal: &str) -> Value {
        match self {
            Self::Slew { rise, fall } => {
                json!({"use": "slew", "signal": signal, "rise": rise, "fall": fall, "rate": 100})
            }
            Self::Follow { attack, release } => {
                json!({"use": "follow", "signal": signal, "attack": attack, "release": release, "rate": 100})
            }
            Self::Gain { by } => json!({"use": "gain", "signal": signal, "by": by}),
        }
    }
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (-0.5..2.0f64, -0.5..2.0f64).prop_map(|(rise, fall)| Step::Slew { rise, fall }),
        (-0.5..2.0f64, -0.5..2.0f64).prop_map(|(attack, release)| Step::Follow { attack, release }),
        (-1.0..11.0f64).prop_map(|by| Step::Gain { by }),
    ]
}

/// The first node of a chain: a level, or kicks written inline.
fn first() -> impl Strategy<Value = Value> {
    prop_oneof![
        (-12.0..12.0f64).prop_map(|value| json!({"use": "level", "value": value})),
        proptest::collection::vec((0.0..3.0f64, 0.0..1.0f64), 0..5).prop_map(|kicks| {
            let events: Vec<Value> = kicks
                .iter()
                .map(|(t, strength)| json!({"t": t, "strength": strength}))
                .collect();
            json!({"use": "hits", "events": events})
        }),
    ]
}

/// What calling each node of `nodes` by hand, through the registry, gives: the last node's signal, or the first
/// refusal.
fn by_hand(registry: &Registry, nodes: &[(String, Value)]) -> Result<Signal, Error> {
    let mut made: Option<Signal> = None;
    for (_, node) in nodes {
        let thing = registry.get(node["use"].as_str().unwrap_or_default())?;
        let mut call = Call::default();
        let mut values = node.as_object().cloned().unwrap_or_default();
        values.remove("use");
        values.remove("signal");
        if let Some(events) = values.remove("events") {
            call.inputs
                .insert("events".to_owned(), InputKind::Events.check(&events)?);
        }
        call.signals.extend(made.take());
        call.values = Value::Object(values);
        made = match thing.call(&call)? {
            Output::Signal(signal) => Some(signal),
            _ => None,
        };
    }
    made.ok_or_else(|| Error::Call {
        message: "no signal".to_owned(),
    })
}

proptest! {
    #[test]
    fn a_patch_that_reads_builds_what_the_same_calls_by_hand_build(
        source in first(),
        steps in proptest::collection::vec(step(), 0..4),
        times in proptest::collection::vec(-1.0..4.0f64, 1..8),
    ) {
        let registry = registry().map_err(|error| TestCaseError::fail(error.to_string()))?;
        let mut nodes = vec![("n0".to_owned(), source)];
        for (index, step) in steps.iter().enumerate() {
            nodes.push((format!("n{}", index + 1), step.node(&format!("n{index}"))));
        }
        let last = nodes[nodes.len() - 1].0.clone();
        let entries = patch_of(&nodes.iter().map(|(name, node)| (name.as_str(), node.clone())).collect::<Vec<_>>());
        let hand = by_hand(&registry, &nodes);
        match Patch::parse_with(&registry, &entries) {
            Ok(patch) => {
                let built = patch.build(None).map_err(|error| TestCaseError::fail(error.to_string()))?;
                let Some(Output::Signal(made)) = built.get(&last) else {
                    return Err(TestCaseError::fail("the last node gave no signal"));
                };
                let hand = hand.map_err(|error| TestCaseError::fail(format!("by hand: {error}")))?;
                prop_assert_eq!(made.sample(&times).ok(), hand.sample(&times).ok());
                prop_assert_eq!(built.len(), nodes.len());
            }
            // a patch refused when read is one whose calls by hand are refused too
            Err(error) => prop_assert!(hand.is_err(), "read: {error}, but by hand it builds"),
        }
    }
}
