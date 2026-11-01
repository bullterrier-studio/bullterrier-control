//! Saving a graph as a patch, through the public face: a saved chain is the TOML written by hand, what cannot be saved
//! is refused at its path with why, names made for the signals above are free of every other, a signal used twice is
//! one node, the edges of numbers, text and keys come back as the same bits, and a property: a random graph of things
//! saved, written and read again builds signals that sample as the same bits as the graph. Every expected text, path
//! and name is worked out by hand; the oracle of the property is the graph itself.

use std::convert::Infallible;

use proptest::prelude::*;
use serde_json::{Value, json};

use super::Patch;
use super::PatchError;
use crate::Error;
use crate::FromValues;
use crate::Interpolation;
use crate::Rendering;
use crate::Signal;
use crate::builtin::constant;
use crate::builtin::follow;
use crate::builtin::slew;
use crate::easing::Ease;
use crate::formats::Event;
use crate::formats::EventOptions;
use crate::formats::FormatError;
use crate::formats::Grid;
use crate::formats::Series;
use crate::formats::SeriesOptions;
use crate::formats::Unit;
use crate::inputs::Input;
use crate::inputs::InputKind;
use crate::inputs::Keyframe;
use crate::lib;
use crate::maximum;
use crate::minimum;
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

// ---------------------------------------------------------------- the things a saved graph uses

/// A decimal field from `min` to `max`.
fn decimal(name: &str, (min, max): (f64, f64)) -> Param {
    Param {
        name: name.to_owned(),
        kind: ParamKind::Float,
        default: 0.0.into(),
        doc: format!("The {name}."),
        unit: Some(ParamUnit::Ratio),
        min: Some(min),
        max: Some(max),
        advanced: false,
        cyclic: false,
    }
}

/// A whole number field over every `i64` (2^63 is the decimal nearest to either end).
fn whole(name: &str) -> Param {
    Param {
        kind: ParamKind::Whole,
        default: 0.into(),
        unit: Some(ParamUnit::Count),
        ..decimal(name, (-9_223_372_036_854_775_808.0, 9_223_372_036_854_775_808.0))
    }
}

/// A text field, empty by default.
fn text(name: &str) -> Param {
    Param {
        kind: ParamKind::Text,
        default: "".into(),
        unit: None,
        min: None,
        max: None,
        ..decimal(name, (0.0, 0.0))
    }
}

/// The thing `name` of `kind` defined as `identity`, with `params` and `inputs`, whose body is `run`.
fn thing(
    (name, kind, identity): (&str, Kind, &str),
    params: Vec<Param>,
    inputs: &[(InputKind, bool)],
    run: impl Fn(&Call) -> Result<Output, Error> + Send + Sync + 'static,
) -> Result<Thing, Box<dyn std::error::Error>> {
    let declared = Declared {
        name: name.to_owned(),
        kind,
        identity: identity.to_owned(),
        table: ParamTable::new(params).map_err(|error| error.to_string())?,
        inputs: inputs.to_vec(),
        returns: Returns::Signal,
    };
    Ok(Thing::custom(About::default(), declared, run)?)
}

/// The number `name` of a call's values (each is there: the table read them).
fn number(call: &Call, name: &str) -> f64 {
    call.values[name].as_f64().unwrap_or(f64::NAN)
}

/// The input `kind` of a call, if it was given.
fn input(call: &Call, kind: InputKind) -> Option<&Input> {
    call.inputs.get(kind.name())
}

/// A signal whose value at each time is `at` of it.
fn of_time(at: impl Fn(f64) -> f64 + Send + Sync + 'static) -> Output {
    Output::Signal(Signal::from_fn(move |times, out| {
        for (value, time) in out.iter_mut().zip(times) {
            *value = at(*time);
        }
        Ok::<(), Infallible>(())
    }))
}

/// The registry the graphs are saved for: the built-ins `constant`, `slew` and `follow`, and these things, each body
/// reading every field of its data so that a field lost on the way changes what it gives.
/// - `level` (source, `value` -1e6..1e6): the constant `value`.
/// - `wide` (source, `x` any finite decimal, `n` any whole number, `word` text, and decimals under keys a TOML file
///   must quote: `two words`, `a.b`, `ünï`): the constant `x`.
/// - `hits` (source, events): at a time, the sum over the events at or before it of strength, duration (or 0.5),
///   pitch (or 0.25) and the length of the label (or 0).
/// - `curve` (source, a series): the series read linearly.
/// - `beats` (source, a grid): the beat position at the time, plus 1000 times the meter and 100 times the first
///   downbeat.
/// - `keys` (source, keyframes): the sum of `t` and `value` over the keyframes at or before the time.
/// - `cycle` (source, values): the time times the count of values, plus their sum.
/// - `gain` (shaper, `by` -10..10): its signal times `by`.
#[expect(clippy::too_many_lines, reason = "one thing a few lines each")]
fn registry() -> Result<Registry, Box<dyn std::error::Error>> {
    let mut registry = Registry::new();
    for name in ["constant", "slew", "follow"] {
        registry.register(Registry::builtin().get(name)?.clone())?;
    }
    let level = thing(
        ("level", Kind::Source, "tests.level"),
        vec![decimal("value", (-1e6, 1e6))],
        &[],
        |call| Ok(Output::Signal(Signal::constant(number(call, "value"))?)),
    )?;
    let wide = thing(
        ("wide", Kind::Source, "tests.wide"),
        vec![
            decimal("x", (-f64::MAX, f64::MAX)),
            whole("n"),
            text("word"),
            decimal("two words", (-1.0, 1.0)),
            decimal("a.b", (-1.0, 1.0)),
            decimal("ünï", (-1.0, 1.0)),
        ],
        &[],
        |call| Ok(Output::Signal(Signal::constant(number(call, "x"))?)),
    )?;
    let hits = thing(
        ("hits", Kind::Source, "tests.hits"),
        vec![],
        &[(InputKind::Events, true)],
        |call| {
            let Some(Input::Events(events)) = input(call, InputKind::Events).cloned() else {
                return Err(Error::Call {
                    message: "no events".into(),
                });
            };
            Ok(of_time(move |time| {
                events
                    .iter()
                    .filter(|event| event.t() <= time)
                    .map(|event| {
                        let label = event.label().map_or(0, |label| label.chars().count());
                        event.strength()
                            + event.duration().unwrap_or(0.5)
                            + event.pitch().unwrap_or(0.25)
                            + f64::from(u32::try_from(label).unwrap_or(u32::MAX))
                    })
                    .sum()
            }))
        },
    )?;
    let curve = thing(
        ("curve", Kind::Source, "tests.curve"),
        vec![],
        &[(InputKind::Series, true)],
        |call| match input(call, InputKind::Series) {
            Some(Input::Series(series)) => Ok(Output::Signal(Signal::from_series(series, Interpolation::Linear))),
            _ => Err(Error::Call {
                message: "no series".into(),
            }),
        },
    )?;
    let beats = thing(
        ("beats", Kind::Source, "tests.beats"),
        vec![],
        &[(InputKind::Grid, true)],
        |call| {
            let Some(Input::Grid(grid)) = input(call, InputKind::Grid).cloned() else {
                return Err(Error::Call {
                    message: "no grid".into(),
                });
            };
            let offset = 1000.0 * f64::from(grid.meter()) + 100.0 * f64::from(grid.first_downbeat());
            Ok(Output::Signal(Signal::from_fn(move |times, out| {
                for (value, beat) in out.iter_mut().zip(grid.beat_at(times)?) {
                    *value = beat + offset;
                }
                Ok::<(), FormatError>(())
            })))
        },
    )?;
    let keys = thing(
        ("keys", Kind::Source, "tests.keys"),
        vec![],
        &[(InputKind::Points, true)],
        |call| {
            let Some(Input::Points(points)) = input(call, InputKind::Points).cloned() else {
                return Err(Error::Call {
                    message: "no points".into(),
                });
            };
            Ok(of_time(move |time| {
                points
                    .iter()
                    .filter(|point| point.t <= time)
                    .map(|point| point.t + point.value)
                    .sum()
            }))
        },
    )?;
    let cycle = thing(
        ("cycle", Kind::Source, "tests.cycle"),
        vec![],
        &[(InputKind::Values, true)],
        |call| {
            let Some(Input::Values(values)) = input(call, InputKind::Values).cloned() else {
                return Err(Error::Call {
                    message: "no values".into(),
                });
            };
            let count = f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX));
            let sum: f64 = values.iter().sum();
            Ok(of_time(move |time| time * count + sum))
        },
    )?;
    let gain = thing(
        ("gain", Kind::Shaper, "tests.gain"),
        vec![decimal("by", (-10.0, 10.0))],
        &[],
        |call| match call.signals.as_slice() {
            [signal] => Ok(Output::Signal(signal * number(call, "by"))),
            _ => Err(Error::Call {
                message: "gain takes a signal".into(),
            }),
        },
    )?;
    for made in [level, wide, hits, curve, beats, keys, cycle, gain] {
        registry.register(made)?;
    }
    Ok(registry)
}

/// The signal `name` of `registry` makes of `signals`, `values` and `inputs`.
fn call(
    registry: &Registry,
    name: &str,
    signals: &[&Signal],
    values: Value,
    inputs: Vec<Input>,
) -> Result<Signal, Box<dyn std::error::Error>> {
    let call = Call {
        signals: signals.iter().map(|signal| (*signal).clone()).collect(),
        inputs: inputs
            .into_iter()
            .map(|input| (input.kind().name().to_owned(), input))
            .collect(),
        values,
    };
    match registry.get(name)?.call(&call)? {
        Output::Signal(signal) => Ok(signal),
        _ => Err("a signal".into()),
    }
}

/// `level` of `value`.
fn level(registry: &Registry, value: f64) -> Result<Signal, Box<dyn std::error::Error>> {
    call(registry, "level", &[], json!({"value": value}), vec![])
}

/// `signal` through `gain` by `by`.
fn gain(registry: &Registry, signal: &Signal, by: f64) -> Result<Signal, Box<dyn std::error::Error>> {
    call(registry, "gain", &[signal], json!({"by": by}), vec![])
}

/// The names of a patch's nodes, in file order.
fn names(patch: &Patch) -> Vec<&str> {
    patch.nodes().map(|(name, _)| name).collect()
}

/// The signal `name` of `patch` built with no bundle.
fn built(patch: &Patch, name: &str) -> Result<Signal, Box<dyn std::error::Error>> {
    match patch.build(None)?.get(name) {
        Some(Output::Signal(signal)) => Ok(signal.clone()),
        _ => Err(format!("no signal {name}").into()),
    }
}

/// What a signal gives at `times`, each value as its bits (`-0.0` is not `0.0`), or the text of its refusal.
fn bits(signal: &Signal, times: &[f64]) -> Result<Vec<u64>, String> {
    signal
        .sample(times)
        .map(|values| values.iter().map(|value| value.to_bits()).collect())
        .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------- a chain, by hand

#[test]
fn a_saved_chain_of_built_ins_is_the_toml_written_by_hand() -> Outcome {
    let level = lib::constant(constant::Params { value: 0.5 })?;
    let smooth = level.slew(slew::Params {
        rise: 0.25,
        ..slew::Params::default()
    })?;
    let glow = smooth.follow(follow::Params {
        attack: 0.0,
        release: 0.5,
        rate: 2000.0,
    })?;
    // the signals above `glow` are named after it, from the top; every parameter is written, a default too
    let expected = "\
format = \"bullterrier-control/patch\"
version = 1

[glow_1]
use = \"constant\"
value = 0.5

[glow_2]
use = \"slew\"
signal = \"glow_1\"
rise = 0.25
fall = 0.1
rate = 1000.0

[glow]
use = \"follow\"
signal = \"glow_2\"
attack = 0.0
release = 0.5
rate = 2000.0
";
    let saved = Patch::from_signals(&[("glow", &glow)])?;
    assert_eq!(saved.to_toml(), expected);
    // and it builds the same signal: the same bits at every time
    let times = [-1.0, 0.0, 0.001, 0.25, 0.3, 1.0, 7.5];
    assert_eq!(
        bits(&built(&Patch::from_toml(expected)?, "glow")?, &times),
        bits(&glow, &times)
    );
    Ok(())
}

#[test]
fn a_patch_read_from_toml_is_written_as_it_reads() -> Outcome {
    let registry = registry()?;
    // references to the bundle and to nodes, inline data, parameters left out (they stay out) and a whole number
    // given for a decimal (it stays whole), written in another order than the thing declares
    let text = "format = 'bullterrier-control/patch'\nversion = 1\n\
                [smooth]\nrate = 10\nrise = 0.5\nsignal = 'series.energy'\nuse = 'slew'\n\
                [kicks]\nuse = 'hits'\nevents = 'events.kick'\n\
                [pulse]\nuse = 'beats'\ngrid = 'grid'\n\
                [steps]\nuse = 'cycle'\nvalues = [1, 0.5]\n";
    let expected = "\
format = \"bullterrier-control/patch\"
version = 1

[smooth]
use = \"slew\"
signal = \"series.energy\"
rise = 0.5
rate = 10

[kicks]
use = \"hits\"
events = \"events.kick\"

[pulse]
use = \"beats\"
grid = \"grid\"

[steps]
use = \"cycle\"
values = [1.0, 0.5]
";
    let patch = Patch::from_toml_with(&registry, text)?;
    assert_eq!(patch.to_toml(), expected);
    assert_eq!(Patch::from_toml_with(&registry, expected)?, patch);
    Ok(())
}

// ---------------------------------------------------------------- refusals

/// What a refusal row gives to save: the signals by name, and the registry they are saved for.
type Given = (Vec<(&'static str, Signal)>, Registry);

/// The problems a refusal row expects: each path, and a part of its rule.
type Expected = Vec<(&'static str, &'static str)>;

#[test]
#[expect(clippy::too_many_lines, reason = "one table, a row per refusal")]
fn what_a_patch_cannot_hold_is_refused_at_its_path_saying_why() -> Outcome {
    let registry = registry()?;
    let one = level(&registry, 1.0)?;
    let two = level(&registry, 2.0)?;
    let shaped = |signal: &Signal| gain(&registry, signal, 2.0);
    let echo = Signal::from_fn(|times, out| {
        out.copy_from_slice(times);
        Ok::<(), Infallible>(())
    });
    let rendered = Signal::rendered(&one, Rendering::default(), |inputs| {
        Ok::<_, Infallible>(inputs.to_vec())
    })?;
    let series = Series::new(10.0, vec![0.0, 1.0], Unit::Ratio, SeriesOptions::default())?;
    let values = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
    // a thing no registry here holds, and one that holds another thing under the name `gain`
    let fold = thing(("fold", Kind::Shaper, "tests.fold"), vec![], &[], |call| {
        match call.signals.as_slice() {
            [signal] => Ok(Output::Signal(signal.clone())),
            _ => Err(Error::Call {
                message: "fold takes a signal".into(),
            }),
        }
    })?;
    let Output::Signal(folded) = fold.call(&Call {
        signals: vec![one.clone()],
        ..Call::default()
    })?
    else {
        return Err("a signal".into());
    };
    let mut other = registry.clone();
    other.unregister("gain")?;
    other.register(thing(
        ("gain", Kind::Shaper, "elsewhere.gain"),
        vec![decimal("by", (-10.0, 10.0))],
        &[],
        |call| Ok(Output::Signal(call.signals[0].clone())),
    )?)?;
    let made = Registry::new();
    let mine = || registry.clone();
    let operator = "is made by an operator (+, -, *, negation, minimum or maximum), which a patch has no node for";
    let values_rule = "holds values given as they are (a constant, from_values or from_series)";
    let not_a_name = "is not a node name: lower case letters, digits and _, starting with a letter";
    // (row, what is saved, the problems: each path and a part of its rule; none when it saves)
    let rows: Vec<(&str, Given, Expected)> = vec![
        (
            "a graph of things saves",
            (vec![("glow", shaped(&one)?)], mine()),
            vec![],
        ),
        (
            "a sum given",
            (vec![("mix", &one + &two)], mine()),
            vec![("mix", operator)],
        ),
        (
            "a sum above",
            (vec![("glow", shaped(&(&one + &two))?)], mine()),
            vec![("glow.signal", operator)],
        ),
        (
            "a difference with a number",
            (vec![("glow", shaped(&(1.0 - &one))?)], mine()),
            vec![("glow.signal", operator)],
        ),
        (
            "a product",
            (vec![("glow", shaped(&(&one * &two))?)], mine()),
            vec![("glow.signal", operator)],
        ),
        (
            "a negation",
            (vec![("glow", shaped(&-&one)?)], mine()),
            vec![("glow.signal", operator)],
        ),
        (
            "the smaller of two",
            (vec![("glow", shaped(&minimum(&one, &two))?)], mine()),
            vec![("glow.signal", operator)],
        ),
        (
            "the larger of two",
            (vec![("glow", shaped(&maximum(&one, &two))?)], mine()),
            vec![("glow.signal", operator)],
        ),
        (
            "a constant made outside the registry",
            (vec![("glow", shaped(&Signal::constant(0.5)?)?)], mine()),
            vec![("glow.signal", values_rule)],
        ),
        (
            "from values",
            (vec![("glow", shaped(&values)?)], mine()),
            vec![("glow.signal", values_rule)],
        ),
        (
            "from a series",
            (
                vec![("glow", shaped(&Signal::from_series(&series, Interpolation::Linear))?)],
                mine(),
            ),
            vec![("glow.signal", values_rule)],
        ),
        (
            "a callback",
            (vec![("glow", shaped(&echo)?)], mine()),
            vec![("glow.signal", "is made by a callback (from_fn, or a Python compute)")],
        ),
        (
            "a recurrence of a callback",
            (vec![("glow", shaped(&rendered)?)], mine()),
            vec![(
                "glow.signal",
                "is rendered by a callback (rendered, or a Python Rendered(run=...))",
            )],
        ),
        (
            "two steps up",
            (vec![("glow", shaped(&shaped(&(&one + 1.0))?)?)], mine()),
            vec![("glow.signal.signal", operator)],
        ),
        (
            "above a given signal, the path starts at it",
            {
                let base = shaped(&(&one + 1.0))?;
                (vec![("glow", shaped(&base)?), ("base", base)], mine())
            },
            vec![("base.signal", operator)],
        ),
        (
            "one problem for each signal given",
            (vec![("a", &one + 1.0), ("b", shaped(&echo)?)], mine()),
            vec![("a", operator), ("b.signal", "a callback")],
        ),
        (
            "a thing the registry does not hold",
            (vec![("glow", shaped(&folded)?)], mine()),
            vec![(
                "glow.signal",
                "is made by 'fold', which the registry it is saved for does not hold",
            )],
        ),
        (
            "another thing under its name",
            (vec![("glow", shaped(&one)?)], other),
            vec![(
                "glow",
                "is made by a thing called 'gain' other than the one the registry it is saved for holds",
            )],
        ),
        (
            "a built-in, saved for a registry without it",
            (vec![("glow", lib::constant(constant::Params { value: 0.5 })?)], made),
            vec![("glow", "is made by 'constant', which the registry")],
        ),
        (
            "a name with capitals",
            (vec![("Glow", one.clone())], mine()),
            vec![("Glow", not_a_name)],
        ),
        (
            "a name a patch reserves",
            (vec![("grid", one.clone())], mine()),
            vec![("grid", "and not one of format, version, events, series, grid")],
        ),
        (
            "a name given twice",
            (vec![("a", one.clone()), ("a", two.clone())], mine()),
            vec![("a", "is given twice: each node has a name of its own")],
        ),
    ];
    for (row, (signals, registry), expected) in rows {
        let given: Vec<(&str, &Signal)> = signals.iter().map(|(name, signal)| (*name, signal)).collect();
        match (Patch::from_signals_with(&registry, &given), expected.as_slice()) {
            (Ok(_), []) => {}
            (Err(PatchError::Unsaved { problems, omitted: 0 }), expected) => {
                let found: Vec<(&str, &str)> = problems
                    .iter()
                    .map(|problem| (problem.path.as_str(), problem.rule.as_str()))
                    .collect();
                assert_eq!(found.len(), expected.len(), "{row}: {found:?}");
                for ((path, rule), (expected_path, part)) in found.iter().zip(expected) {
                    assert!(path == expected_path && rule.contains(part), "{row}: {path}: {rule}");
                }
            }
            (other, _) => return Err(format!("{row}: {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn a_refusal_says_where_and_why_in_its_message() -> Outcome {
    let registry = registry()?;
    let mix = level(&registry, 1.0)? * 2.0;
    let refused = Patch::from_signals_with(&registry, &[("mix", &mix)])
        .map(|_| ())
        .map_err(|error| error.to_string());
    assert_eq!(
        refused.err().as_deref(),
        Some(
            "cannot save as a patch: mix: is made by an operator (+, -, *, negation, minimum or maximum), which a patch \
             has no node for: make it with a source, shaper or operation"
        )
    );
    Ok(())
}

// ---------------------------------------------------------------- names and sharing

/// A naming row: what it shows, the signals by name, the nodes in file order.
type Row<'a> = (&'a str, Vec<(&'a str, &'a Signal)>, Vec<&'a str>);

#[test]
fn the_signals_above_are_named_after_the_signal_given_and_never_like_another() -> Outcome {
    let registry = registry()?;
    let chain = |value: f64| -> Result<Signal, Box<dyn std::error::Error>> {
        let level = level(&registry, value)?;
        let once = gain(&registry, &level, 2.0)?;
        gain(&registry, &once, 3.0)
    };
    let a = chain(1.0)?;
    let b = chain(2.0)?;
    let base = gain(&registry, &level(&registry, 3.0)?, 2.0)?;
    let on_base = gain(&registry, &base, 4.0)?;
    // (row, the signals by name, the nodes in file order)
    let rows: Vec<Row<'_>> = vec![
        (
            "from the top, numbered from 1",
            vec![("glow", &a)],
            vec!["glow_1", "glow_2", "glow"],
        ),
        (
            "a name given is not made",
            vec![("glow", &a), ("glow_1", &b)],
            vec!["glow_2", "glow_3", "glow", "glow_1_1", "glow_1_2", "glow_1"],
        ),
        (
            "a name given later is not made",
            vec![("glow", &a), ("glow_2", &b)],
            vec!["glow_1", "glow_3", "glow", "glow_2_1", "glow_2_2", "glow_2"],
        ),
        (
            "a given signal above is written above, under its name",
            vec![("glow", &on_base), ("base", &base)],
            vec!["base_1", "base", "glow"],
        ),
        (
            "the same signal under two names",
            vec![("a", &base), ("b", &base)],
            vec!["a_1", "a", "b"],
        ),
    ];
    for (row, given, expected) in rows {
        let saved = Patch::from_signals_with(&registry, &given)?;
        assert_eq!(names(&saved), expected, "{row}");
        // it reads back as it was written
        assert_eq!(Patch::from_toml_with(&registry, &saved.to_toml())?, saved, "{row}");
    }
    Ok(())
}

#[test]
fn a_signal_two_signals_use_is_one_node_both_refer_to() -> Outcome {
    let registry = registry()?;
    let shared = gain(&registry, &level(&registry, 0.5)?, 2.0)?;
    let glow = shared.follow(follow::Params::default())?;
    let halo = shared.slew(slew::Params::default())?;
    let saved = Patch::from_signals_with(&registry, &[("glow", &glow), ("halo", &halo)])?;
    assert_eq!(names(&saved), ["glow_1", "glow_2", "glow", "halo"]);
    let text = saved.to_toml();
    assert_eq!(text.matches("use = \"gain\"").count(), 1, "{text}");
    assert!(
        text.contains("[glow]\nuse = \"follow\"\nsignal = \"glow_2\"\n"),
        "{text}"
    );
    assert!(text.contains("[halo]\nuse = \"slew\"\nsignal = \"glow_2\"\n"), "{text}");
    let rebuilt = Patch::from_toml_with(&registry, &text)?;
    let times = [0.0, 0.1, 2.0];
    assert_eq!(bits(&built(&rebuilt, "halo")?, &times), bits(&halo, &times));
    Ok(())
}

#[test]
fn nothing_given_is_a_patch_of_no_node() -> Outcome {
    let saved = Patch::from_signals(&[])?;
    assert_eq!(saved.to_toml(), "format = \"bullterrier-control/patch\"\nversion = 1\n");
    assert_eq!(saved.nodes().count(), 0);
    Ok(())
}

// ---------------------------------------------------------------- the edges of numbers, text and keys

#[test]
fn numbers_text_and_keys_at_their_edges_come_back_as_the_same_bits() -> Outcome {
    let registry = registry()?;
    // (row, the values given to `wide`, a line the saved text holds)
    let rows: Vec<(&str, Value, &str)> = vec![
        ("negative zero", json!({"x": -0.0}), "x = -0.0\n"),
        ("the smallest subnormal", json!({"x": 5e-324}), "x = 5e-324\n"),
        ("a subnormal", json!({"x": 1e-320}), "x = 1e-320\n"),
        ("a large decimal", json!({"x": 1e308}), "x = 1e308\n"),
        (
            "the most negative decimal",
            json!({"x": -f64::MAX}),
            "x = -1.7976931348623157e308\n",
        ),
        (
            "a sum that is not a tenth's multiple",
            json!({"x": 0.1 + 0.2}),
            "x = 0.30000000000000004\n",
        ),
        ("a third", json!({"x": 1.0 / 3.0}), "x = 0.3333333333333333\n"),
        (
            "a whole number from 1e16, as a decimal",
            json!({"x": 1e16}),
            "x = 1e16\n",
        ),
        (
            "2^53 + 1 given for a decimal is the decimal it reads as",
            json!({"x": 9_007_199_254_740_993_i64}),
            "x = 9007199254740992.0\n",
        ),
        (
            "2^53 + 1 as a whole number, exactly",
            json!({"n": 9_007_199_254_740_993_i64}),
            "n = 9007199254740993\n",
        ),
        (
            "the least whole number",
            json!({"n": i64::MIN}),
            "n = -9223372036854775808\n",
        ),
        (
            "the greatest whole number",
            json!({"n": i64::MAX}),
            "n = 9223372036854775807\n",
        ),
        ("empty text", json!({"word": ""}), "word = \"\"\n"),
        (
            "unicode text as it is",
            json!({"word": "glüh 🌟 ✓"}),
            "word = \"glüh 🌟 ✓\"\n",
        ),
        (
            "quotes and a backslash",
            json!({"word": "say \"hi\" \\ 'x'"}),
            r#"word = "say \"hi\" \\ 'x'""#,
        ),
        (
            "line breaks and tabs",
            json!({"word": "a\nb\tc\r"}),
            r#"word = "a\nb\tc\r""#,
        ),
        (
            "control characters",
            json!({"word": "\u{0}\u{8}\u{c}\u{1f}\u{7f}"}),
            r#"word = "\u0000\b\f\u001F\u007F""#,
        ),
        (
            "a control character TOML takes as it is",
            json!({"word": "\u{85}"}),
            "word = \"\u{85}\"\n",
        ),
        ("a key with a space", json!({"two words": 0.5}), "\"two words\" = 0.5\n"),
        ("a key with a dot", json!({"a.b": -0.5}), "\"a.b\" = -0.5\n"),
        ("a key of unicode", json!({"ünï": 0.25}), "\"ünï\" = 0.25\n"),
    ];
    for (row, values, line) in rows {
        let made = call(&registry, "wide", &[], values, vec![])?;
        let saved = Patch::from_signals_with(&registry, &[("edge", &made)])?;
        let text = saved.to_toml();
        assert!(text.contains(line), "{row}: {text}");
        let read = Patch::from_toml_with(&registry, &text)?;
        // the values the thing was called with again: each the same bits (JSON text tells -0.0 from 0.0 and 1 from 1.0)
        let again = built(&read, "edge")?;
        let recorded = |signal: &Signal| signal.recipe().map(|recipe| recipe.values.to_string());
        assert_eq!(recorded(&again), recorded(&made), "{row}");
        assert_eq!(bits(&again, &[0.0, 1e300]), bits(&made, &[0.0, 1e300]), "{row}");
        // and the text is written again as it was
        assert_eq!(read.to_toml(), text, "{row}");
    }
    Ok(())
}

#[test]
fn data_at_its_edges_comes_back_as_it_was_given() -> Outcome {
    let registry = registry()?;
    let label = |text: &str| EventOptions {
        label: Some(text.to_owned()),
        ..EventOptions::default()
    };
    let events = vec![
        Event::new(
            0.0,
            EventOptions {
                strength: 0.0,
                ..EventOptions::default()
            },
        )?,
        Event::new(1e-320, label("quote \" back \\ tab \t line \n del \u{7f}"))?,
        Event::new(
            2.5,
            EventOptions {
                duration: Some(0.125),
                pitch: Some(127.0),
                ..label("🥁 kick")
            },
        )?,
    ];
    let series = Series::new(
        44.1,
        vec![0.0, 1.0, 0.5],
        Unit::Fraction,
        SeriesOptions {
            start: 0.75,
            scaling: Some("5th to 98th \"percentile\"".to_owned()),
        },
    )?;
    let points = vec![
        Keyframe::at(0.0, -0.0),
        Keyframe::at(0.5, 1.0).ease(Ease::Bezier {
            x1: 0.25,
            y1: -1.5,
            x2: 1.0,
            y2: 2.0 / 3.0,
        }),
        Keyframe::at(2.0, 3.0).ease(Ease::Overshoot),
    ];
    // (the thing, its input)
    let rows: Vec<(&str, Input)> = vec![
        ("hits", Input::from_events(events)),
        ("curve", Input::Series(series)),
        ("beats", Input::Grid(Grid::new(vec![0.5, 1.0, 1.4, 1.8], 3, 2)?)),
        ("keys", Input::from_points(points)?),
        ("cycle", Input::from_values(vec![-0.0, 5e-324, 1e308])?),
    ];
    for (name, given) in rows {
        let made = call(&registry, name, &[], json!({}), vec![given.clone()])?;
        let saved = Patch::from_signals_with(&registry, &[("data", &made)])?;
        let text = saved.to_toml();
        let read = Patch::from_toml_with(&registry, &text)?;
        assert_eq!(read, saved, "{name}: {text}");
        assert_eq!(read.to_toml(), text, "{name}");
        let again = built(&read, "data")?;
        let inputs = again.recipe().map(|recipe| recipe.inputs.clone());
        assert_eq!(inputs, Some(vec![(given.kind().name().to_owned(), given)]), "{name}");
        let times = [0.0, 0.5, 1.2, 2.5, 30.0];
        assert_eq!(bits(&again, &times), bits(&made, &times), "{name}");
    }
    Ok(())
}

// ---------------------------------------------------------------- the property

/// An event of a random graph: `t`, strength, duration, label and pitch.
type Hit = (f64, f64, Option<f64>, Option<String>, Option<f64>);

/// Signals by name.
type Named = Vec<(String, Signal)>;

/// A source of a random graph, with its values or data.
#[derive(Clone, Debug)]
enum First {
    Constant(f64),
    Level(f64),
    Wide(f64),
    Hits(Vec<Hit>),
    Curve(f64, f64, Vec<f64>),
    Beats(f64, Vec<f64>, u32, u32),
    Keys(Vec<(f64, f64, Ease)>),
    Cycle(Vec<f64>),
}

/// A shaper of a random graph, with its values.
#[derive(Clone, Debug)]
enum Then {
    Slew(f64, f64, f64),
    Follow(f64, f64, f64),
    Gain(f64),
}

/// Text that TOML must escape, or need not.
fn words() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "kick",
        "é snare",
        "quote\"d",
        "back\\slash",
        "tab\tline\nbreak",
        "🌟",
        "\u{7f}\u{1}",
    ])
    .prop_map(str::to_owned)
}

/// Any finite decimal: normal, subnormal or zero, of either sign.
fn any_decimal() -> impl Strategy<Value = f64> {
    use prop::num::f64::{NEGATIVE, NORMAL, POSITIVE, SUBNORMAL, ZERO};
    (NORMAL | SUBNORMAL | ZERO | POSITIVE | NEGATIVE).prop_filter("finite", |number| number.is_finite())
}

fn first() -> impl Strategy<Value = First> {
    let event = (
        0.0..20.0f64,
        0.0..=1.0f64,
        prop::option::of(1e-3..10.0f64),
        prop::option::of(words()),
        prop::option::of(0.0..=127.0f64),
    );
    let ease = prop_oneof![
        prop::sample::select(vec![
            Ease::Linear,
            Ease::Hold,
            Ease::In,
            Ease::Out,
            Ease::InOut,
            Ease::CubicIn,
            Ease::CubicOut,
            Ease::CubicInOut,
            Ease::Overshoot
        ]),
        (0.0..=1.0f64, -2.0..2.0f64, 0.0..=1.0f64, -2.0..2.0f64).prop_map(|(x1, y1, x2, y2)| Ease::Bezier {
            x1,
            y1,
            x2,
            y2
        }),
    ];
    prop_oneof![
        (-1e6..1e6f64).prop_map(First::Constant),
        (-1e6..1e6f64).prop_map(First::Level),
        any_decimal().prop_map(First::Wide),
        prop::collection::vec(event, 0..5).prop_map(First::Hits),
        (0.5..100.0f64, 0.0..5.0f64, prop::collection::vec(-1e3..1e3f64, 1..8))
            .prop_map(|(rate, start, values)| First::Curve(rate, start, values)),
        (0.0..2.0f64, prop::collection::vec(0.05..1.0f64, 1..6), 1..8u32, 0..8u32)
            .prop_map(|(first, steps, meter, downbeat)| First::Beats(first, steps, meter, downbeat)),
        prop::collection::vec((0.01..2.0f64, -10.0..10.0f64, ease), 1..5).prop_map(First::Keys),
        prop::collection::vec(-1e6..1e6f64, 1..5).prop_map(First::Cycle),
    ]
}

fn then() -> impl Strategy<Value = Then> {
    prop_oneof![
        (0.0..2.0f64, 0.0..2.0f64, 10.0..2000.0f64).prop_map(|(rise, fall, rate)| Then::Slew(rise, fall, rate)),
        (0.0..2.0f64, 0.0..2.0f64, 10.0..2000.0f64)
            .prop_map(|(attack, release, rate)| Then::Follow(attack, release, rate)),
        (-10.0..10.0f64).prop_map(Then::Gain),
    ]
}

/// The signal `first` makes, through `registry` (or, for `constant`, through its `lib` function).
fn source(registry: &Registry, first: &First) -> Result<Signal, Box<dyn std::error::Error>> {
    Ok(match first {
        First::Constant(value) => lib::constant(constant::Params { value: *value })?,
        First::Level(value) => level(registry, *value)?,
        First::Wide(x) => call(registry, "wide", &[], json!({"x": x, "word": "glüh \"x\""}), vec![])?,
        First::Hits(events) => {
            let events = events
                .iter()
                .map(|(t, strength, duration, label, pitch)| {
                    Event::new(
                        *t,
                        EventOptions {
                            strength: *strength,
                            duration: *duration,
                            label: label.clone(),
                            pitch: *pitch,
                        },
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            call(registry, "hits", &[], json!({}), vec![Input::from_events(events)])?
        }
        First::Curve(rate, start, values) => {
            let options = SeriesOptions {
                start: *start,
                ..SeriesOptions::default()
            };
            let series = Series::new(*rate, values.clone(), Unit::Ratio, options)?;
            call(registry, "curve", &[], json!({}), vec![Input::Series(series)])?
        }
        First::Beats(first, steps, meter, downbeat) => {
            let beats: Vec<f64> = std::iter::once(*first)
                .chain(steps.iter().scan(*first, |at, step| {
                    *at += step;
                    Some(*at)
                }))
                .collect();
            let count = u32::try_from(beats.len())?;
            let grid = Grid::new(beats, *meter, downbeat % meter.min(&count))?;
            call(registry, "beats", &[], json!({}), vec![Input::Grid(grid)])?
        }
        First::Keys(steps) => {
            let mut t = 0.0;
            let points = steps
                .iter()
                .map(|(step, value, ease)| {
                    let point = Keyframe::at(t, *value).ease(*ease);
                    t += step;
                    point
                })
                .collect();
            call(registry, "keys", &[], json!({}), vec![Input::from_points(points)?])?
        }
        First::Cycle(values) => call(
            registry,
            "cycle",
            &[],
            json!({}),
            vec![Input::from_values(values.clone())?],
        )?,
    })
}

/// `signal` through `then`: slew by its typed method, follow by name, gain by name.
fn shaped(registry: &Registry, signal: &Signal, then: &Then) -> Result<Signal, Box<dyn std::error::Error>> {
    Ok(match then {
        Then::Slew(rise, fall, rate) => signal.slew(slew::Params {
            rise: *rise,
            fall: *fall,
            rate: *rate,
        })?,
        Then::Follow(attack, release, rate) => call(
            registry,
            "follow",
            &[signal],
            json!({"attack": attack, "release": release, "rate": rate}),
            vec![],
        )?,
        Then::Gain(by) => gain(registry, signal, *by)?,
    })
}

/// Times to read at: before 0, within the first seconds, and far beyond what a recurrence renders.
fn times() -> impl Strategy<Value = Vec<f64>> {
    prop::collection::vec(prop_oneof![-1e300..0.0f64, 0.0..25.0f64, 1e9..1e300f64], 1..6)
}

/// A graph: chains (a source and its shapers, the last given under a name that may clash with what is made for
/// another), and branches (a shaper of a signal of a chain, given too: a signal used twice).
#[derive(Clone, Debug)]
struct Graph {
    chains: Vec<(First, Vec<Then>)>,
    branches: Vec<(usize, usize, Then)>,
}

fn graph() -> impl Strategy<Value = Graph> {
    (
        prop::collection::vec((first(), prop::collection::vec(then(), 0..4)), 1..4),
        prop::collection::vec((any::<usize>(), any::<usize>(), then()), 0..3),
    )
        .prop_map(|(chains, branches)| Graph { chains, branches })
}

/// The signals of `graph` by name, and how many calls made them (each call is one node of its patch).
fn signals(registry: &Registry, graph: &Graph) -> Result<(Named, usize), Box<dyn std::error::Error>> {
    // names that clash with the names made for the signals above the first chain's
    let chain_names = ["glow", "glow_1", "glow_2"];
    let mut made: Vec<Vec<Signal>> = Vec::new();
    let mut given = Vec::new();
    let mut calls = 0;
    for ((first, thens), name) in graph.chains.iter().zip(chain_names) {
        let mut chain = vec![source(registry, first)?];
        for then in thens {
            let last = chain[chain.len() - 1].clone();
            chain.push(shaped(registry, &last, then)?);
        }
        calls += chain.len();
        given.push((name.to_owned(), chain[chain.len() - 1].clone()));
        made.push(chain);
    }
    for (index, (chain, at, then)) in graph.branches.iter().enumerate() {
        let chain = &made[chain % made.len()];
        let branch = shaped(registry, &chain[at % chain.len()], then)?;
        calls += 1;
        given.push((format!("halo_{index}"), branch));
    }
    Ok((given, calls))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn a_saved_graph_reads_back_and_builds_signals_of_the_same_bits(graph in graph(), times in times()) {
        let check = || -> Outcome {
            let registry = registry()?;
            let (given, calls) = signals(&registry, &graph)?;
            let by_name: Vec<(&str, &Signal)> = given.iter().map(|(name, signal)| (name.as_str(), signal)).collect();
            let saved = Patch::from_signals_with(&registry, &by_name)?;
            // one node per call: a signal used twice is written once
            assert_eq!(saved.nodes().count(), calls);
            let text = saved.to_toml();
            let read = Patch::from_toml_with(&registry, &text)?;
            assert_eq!(read, saved, "{text}");
            assert_eq!(read.to_toml(), text);
            let built = read.build(None)?;
            for (name, signal) in &given {
                let Some(Output::Signal(again)) = built.get(name) else { return Err(format!("no {name}").into()) };
                assert_eq!(bits(again, &times), bits(signal, &times), "{name} in {text}");
            }
            Ok(())
        };
        check().map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}
