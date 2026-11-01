//! The registry through its public face: the rules for getting in, the checks of a call, the queries, and what a
//! thing says about itself, each expected text worked out by hand from the Python package's rules (`registry.py`).

use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde_json::json;

use super::{About, Call, Declared, Kind, Output, Registry, Returns, Thing};
use crate::formats::{Event, EventOptions, Grid};
use crate::inputs::{Input, InputKind};
use crate::params::{Kind as ParamKind, Param, ParamTable, ParamUnit};
use crate::{Error, Signal};

/// A decimal field in `unit` from `min` to `max`.
fn number(name: &str, default: f64, unit: ParamUnit, (min, max): (f64, f64)) -> Param {
    Param {
        name: name.to_owned(),
        kind: ParamKind::Float,
        default: default.into(),
        doc: format!("The {name}."),
        unit: Some(unit),
        min: Some(min),
        max: Some(max),
        advanced: false,
        cyclic: false,
    }
}

/// A field of `kind` (not a number) with `default`.
fn plain(name: &str, kind: ParamKind, default: serde_json::Value) -> Param {
    Param {
        name: name.to_owned(),
        kind,
        default,
        doc: format!("The {name}."),
        unit: None,
        min: None,
        max: None,
        advanced: false,
        cyclic: false,
    }
}

/// A switch, on by default.
fn switch(name: &str) -> Param {
    plain(name, ParamKind::Switch, true.into())
}

fn table(params: Vec<Param>) -> Result<ParamTable, String> {
    ParamTable::new(params).map_err(|error| error.to_string())
}

/// What a thing of each kind gives when called: a constant 1, or the events it was given.
fn echo(call: &Call) -> Result<Output, Error> {
    match call.inputs.get("events") {
        Some(Input::Events(events)) => Ok(Output::Events(events.clone())),
        _ => Ok(Output::Signal(Signal::constant(1.0)?)),
    }
}

/// The thing `name` of `kind`, `params`, `inputs` and `returns`, defined as `identity`, whose body is [`echo`].
fn thing(
    (name, identity): (&str, &str),
    kind: Kind,
    params: Vec<Param>,
    inputs: &[(InputKind, bool)],
    returns: Returns,
) -> Result<Thing, Error> {
    let about = About {
        doc: format!("The thing {name}."),
        ..About::default()
    };
    let table = ParamTable::new(params).map_err(|error| Error::params(name, error))?;
    let declared = Declared {
        name: name.to_owned(),
        kind,
        identity: identity.to_owned(),
        shown: None,
        table,
        inputs: inputs.to_vec(),
        returns,
    };
    Thing::custom(about, declared, echo)
}

/// `result`'s registration refusal, or `None` if it is not one.
fn registration<T>(result: Result<T, Error>) -> Option<String> {
    match result {
        Err(Error::Registration { message }) => Some(message),
        _ => None,
    }
}

/// A row of [`a_thing_gets_in_only_if_it_keeps_every_rule`]: (row, name, kind, parameters, inputs, returns, the
/// refusal or `None`).
type RuleRow<'a> = (
    &'a str,
    &'a str,
    Kind,
    Vec<&'a str>,
    Vec<(InputKind, bool)>,
    Returns,
    Option<String>,
);

/// A row of [`a_call_is_checked_before_the_thing_runs`]: (row, thing, signals, inputs, values, the refusal: a call
/// error's message, or the fields of a parameter error).
type CallRow<'a> = (
    &'a str,
    &'a Thing,
    Vec<Signal>,
    BTreeMap<String, Input>,
    serde_json::Value,
    Option<&'a str>,
);

const NAME_RULE: &str = "a name is lower case letters, digits and underscores, starting with a letter";

#[test]
#[expect(clippy::too_many_lines, reason = "one row per rule, good and bad")]
fn a_thing_gets_in_only_if_it_keeps_every_rule() {
    let events = (InputKind::Events, true);
    let rows: Vec<RuleRow<'_>> = vec![
        (
            "plain name",
            "level",
            Kind::Source,
            vec!["on"],
            vec![],
            Returns::Signal,
            None,
        ),
        (
            "digits and _",
            "every_2nd",
            Kind::Source,
            vec![],
            vec![],
            Returns::Signal,
            None,
        ),
        (
            "a source with an input",
            "track",
            Kind::Source,
            vec![],
            vec![(InputKind::Points, true)],
            Returns::Signal,
            None,
        ),
        (
            "a shaper",
            "lift",
            Kind::Shaper,
            vec!["on"],
            vec![],
            Returns::Signal,
            None,
        ),
        (
            "events from events",
            "pick",
            Kind::Operation,
            vec![],
            vec![events],
            Returns::Events,
            None,
        ),
        (
            "a series",
            "rate",
            Kind::Operation,
            vec![],
            vec![(InputKind::Series, true)],
            Returns::Series,
            None,
        ),
        (
            "capital",
            "Slew",
            Kind::Source,
            vec![],
            vec![],
            Returns::Signal,
            Some(format!("source 'Slew': {NAME_RULE}")),
        ),
        (
            "digit first",
            "2nd",
            Kind::Shaper,
            vec![],
            vec![],
            Returns::Signal,
            Some(format!("shaper '2nd': {NAME_RULE}")),
        ),
        (
            "empty",
            "",
            Kind::Source,
            vec![],
            vec![],
            Returns::Signal,
            Some(format!("source '': {NAME_RULE}")),
        ),
        (
            "a hyphen",
            "a-b",
            Kind::Source,
            vec![],
            vec![],
            Returns::Signal,
            Some(format!("source 'a-b': {NAME_RULE}")),
        ),
        (
            "a quote",
            "it's",
            Kind::Source,
            vec![],
            vec![],
            Returns::Signal,
            Some(format!("source \"it's\": {NAME_RULE}")),
        ),
        (
            "name before clash",
            "Bad",
            Kind::Source,
            vec!["events"],
            vec![],
            Returns::Signal,
            Some(format!("source 'Bad': {NAME_RULE}")),
        ),
        (
            "an input's name",
            "level",
            Kind::Source,
            vec!["on", "events"],
            vec![],
            Returns::Signal,
            Some("source 'level': the parameter 'events' has the name of an input or a patch key".to_owned()),
        ),
        (
            "the first clash by name",
            "level",
            Kind::Source,
            vec!["use", "grid"],
            vec![],
            Returns::Signal,
            Some("source 'level': the parameter 'grid' has the name of an input or a patch key".to_owned()),
        ),
        (
            "a patch key",
            "lift",
            Kind::Shaper,
            vec!["signal"],
            vec![],
            Returns::Signal,
            Some("shaper 'lift': the parameter 'signal' has the name of an input or a patch key".to_owned()),
        ),
        (
            "an input twice",
            "pick",
            Kind::Operation,
            vec![],
            vec![events, (InputKind::Events, false)],
            Returns::Events,
            Some("operation 'pick': the input 'events' is declared twice".to_owned()),
        ),
        (
            "a source of events",
            "level",
            Kind::Source,
            vec![],
            vec![],
            Returns::Events,
            Some("source 'level': a source returns a signal (not events)".to_owned()),
        ),
        (
            "a shaper of a series",
            "lift",
            Kind::Shaper,
            vec![],
            vec![],
            Returns::Series,
            Some("shaper 'lift': a shaper returns a signal (not series)".to_owned()),
        ),
        (
            "an operation of a signal",
            "pick",
            Kind::Operation,
            vec![],
            vec![events],
            Returns::Signal,
            Some("operation 'pick': an operation returns events or a series (not a signal)".to_owned()),
        ),
    ];
    for (row, name, kind, params, inputs, returns, refusal) in rows {
        let made = thing(
            (name, "tests.thing"),
            kind,
            params.into_iter().map(switch).collect(),
            &inputs,
            returns,
        );
        match (&made, &refusal) {
            (Ok(made), None) => assert_eq!((made.name(), made.kind()), (name, kind), "{row}"),
            _ => assert_eq!(registration(made), refusal, "{row}"),
        }
    }
}

#[test]
fn what_an_error_shows_of_who_defined_a_thing_is_apart_from_who_they_are() -> Result<(), Error> {
    // (identity, shown): two functions one factory made differ in the address that tells them apart
    let made = |(identity, shown): (&str, Option<&str>)| -> Result<Thing, Error> {
        let declared = Declared {
            name: "level".to_owned(),
            kind: Kind::Source,
            identity: identity.to_owned(),
            shown: shown.map(str::to_owned),
            table: ParamTable::new(vec![]).map_err(|error| Error::params("level", error))?,
            inputs: vec![],
            returns: Returns::Signal,
        };
        Thing::custom(About::default(), declared, echo)
    };
    let mut registry = Registry::new();
    registry.register(made(("m.make.<locals>.made at 0x1", Some("m.make.<locals>.made")))?)?;
    // what is shown is never compared: the same identity shown another way is a reload ...
    registry.register(made(("m.make.<locals>.made at 0x1", None))?)?;
    // ... and another identity is another thing, named as the registered one shows itself (here: by its identity)
    assert_eq!(
        registration(registry.register(made(("m.make.<locals>.made at 0x2", Some("m.make.<locals>.made")))?))
            .as_deref(),
        Some("source 'level': the name is taken by the source defined as m.make.<locals>.made at 0x1")
    );
    let mut shown = Registry::new();
    shown.register(made(("m.make.<locals>.made at 0x1", Some("m.make.<locals>.made")))?)?;
    assert_eq!(
        registration(shown.register(made(("other", None))?)).as_deref(),
        Some("source 'level': the name is taken by the source defined as m.make.<locals>.made")
    );
    Ok(())
}

#[test]
fn a_name_is_taken_by_another_thing_but_not_by_the_same_one_registered_again() -> Result<(), Error> {
    let source = |identity: &str, doc: &str| -> Result<Thing, Error> {
        let about = About {
            doc: doc.to_owned(),
            ..About::default()
        };
        let declared = Declared {
            name: "level".to_owned(),
            kind: Kind::Source,
            identity: identity.to_owned(),
            shown: None,
            table: ParamTable::new(vec![]).map_err(|error| Error::params("level", error))?,
            inputs: vec![],
            returns: Returns::Signal,
        };
        Thing::custom(about, declared, echo)
    };
    let mut registry = Registry::new();
    registry.register(source("a.level", "First.")?)?;
    // the same kind and identity: a reload, which replaces it
    registry.register(source("a.level", "Second.")?)?;
    assert_eq!(registry.get("level")?.summary(), "Second.");
    // another identity
    assert_eq!(
        registration(registry.register(source("b.level", "Third.")?)).as_deref(),
        Some("source 'level': the name is taken by the source defined as a.level")
    );
    // the same identity, another kind
    let shaper = thing(("level", "a.level"), Kind::Shaper, vec![], &[], Returns::Signal)?;
    assert_eq!(
        registration(registry.register(shaper)).as_deref(),
        Some("shaper 'level': the name is taken by the source defined as a.level")
    );
    // a refusal leaves the registered one in place
    assert_eq!(registry.get("level")?.summary(), "Second.");
    // another name is free
    registry.register(thing(("drift", "b.level"), Kind::Source, vec![], &[], Returns::Signal)?)?;
    let names: Vec<&str> = registry.things().map(Thing::name).collect();
    assert_eq!(names, ["drift", "level"]);
    Ok(())
}

#[test]
fn an_unknown_name_is_refused_listing_the_names_there_are() -> Result<(), Error> {
    let mut registry = Registry::new();
    for name in ["level", "drift"] {
        registry.register(thing((name, name), Kind::Source, vec![], &[], Returns::Signal)?)?;
    }
    for refused in [
        registry.get("lvl").map(|_| ()),
        registry.clone().unregister("lvl").map(|_| ()),
    ] {
        assert!(
            matches!(&refused, Err(Error::UnknownName { name, available }) if name == "lvl" && available == &["drift", "level"]),
            "{refused:?}"
        );
    }
    assert_eq!(registry.unregister("drift")?.name(), "drift");
    let text = registry.get("drift").map(|_| ()).map_err(|error| error.to_string());
    assert_eq!(
        text,
        Err("unknown source, shaper or operation 'drift'; available: level".to_owned())
    );
    Ok(())
}

/// Events at `times`.
fn events(times: &[f64]) -> Result<Vec<Event>, Error> {
    times
        .iter()
        .map(|&t| Event::new(t, EventOptions::default()).map_err(|source| Error::Format { source }))
        .collect()
}

#[test]
#[expect(clippy::too_many_lines, reason = "one row per check of a call, good and bad")]
fn a_call_is_checked_before_the_thing_runs() -> Result<(), Error> {
    let pick = thing(
        ("pick", "tests.pick"),
        Kind::Operation,
        vec![number("threshold", 0.0, ParamUnit::Fraction, (0.0, 1.0))],
        &[(InputKind::Events, true), (InputKind::Grid, false)],
        Returns::Events,
    )?;
    let lift = thing(
        ("lift", "tests.lift"),
        Kind::Shaper,
        vec![switch("on")],
        &[],
        Returns::Signal,
    )?;
    let level = thing(("level", "tests.level"), Kind::Source, vec![], &[], Returns::Signal)?;
    let one = Signal::constant(1.0)?;
    let kicks = Input::from_events(events(&[0.5, 1.0])?);
    let grid = Input::Grid(Grid::new(vec![0.0, 0.5, 1.0], 4, 0).map_err(|source| Error::Format { source })?);
    let values = Input::from_values(vec![1.0])?;
    let with = |pairs: &[(&str, &Input)]| -> BTreeMap<String, Input> {
        pairs
            .iter()
            .map(|(name, input)| ((*name).to_owned(), (*input).clone()))
            .collect()
    };
    let rows: Vec<CallRow<'_>> = vec![
        (
            "a shaper with its signal",
            &lift,
            vec![one.clone()],
            with(&[]),
            json!({}),
            None,
        ),
        ("a source with none", &level, vec![], with(&[]), json!({}), None),
        (
            "events, no grid",
            &pick,
            vec![],
            with(&[("events", &kicks)]),
            json!({}),
            None,
        ),
        (
            "events and grid",
            &pick,
            vec![],
            with(&[("events", &kicks), ("grid", &grid)]),
            json!({"threshold": 1}),
            None,
        ),
        (
            "a shaper without a signal",
            &lift,
            vec![],
            with(&[]),
            json!({}),
            Some("shaper 'lift' takes 1 signal(s), not 0"),
        ),
        (
            "a shaper of two",
            &lift,
            vec![one.clone(), one.clone()],
            with(&[]),
            json!({}),
            Some("shaper 'lift' takes 1 signal(s), not 2"),
        ),
        (
            "a source of one",
            &level,
            vec![one.clone()],
            with(&[]),
            json!({}),
            Some("source 'level' takes 0 signal(s), not 1"),
        ),
        (
            "the count first",
            &lift,
            vec![],
            with(&[]),
            json!({"on": 3}),
            Some("shaper 'lift' takes 1 signal(s), not 0"),
        ),
        (
            "no events",
            &pick,
            vec![],
            with(&[("grid", &grid)]),
            json!({}),
            Some("operation 'pick' needs the input 'events'"),
        ),
        (
            "an undeclared input",
            &pick,
            vec![],
            with(&[("events", &kicks), ("series", &values)]),
            json!({}),
            Some("operation 'pick' takes no input 'series'"),
        ),
        (
            "the first undeclared by name",
            &pick,
            vec![],
            with(&[("values", &values), ("points", &values)]),
            json!({}),
            Some("operation 'pick' takes no input 'points'"),
        ),
        (
            "not an input's name",
            &level,
            vec![],
            with(&[("bogus", &values)]),
            json!({}),
            Some("source 'level' takes no input 'bogus'"),
        ),
        (
            "an input of another kind",
            &pick,
            vec![],
            with(&[("events", &values)]),
            json!({}),
            Some("operation 'pick': the input 'events' holds values"),
        ),
        (
            "missing before parameters",
            &pick,
            vec![],
            with(&[]),
            json!({"threshold": 2}),
            Some("operation 'pick' needs the input 'events'"),
        ),
        (
            "a parameter out of range",
            &pick,
            vec![],
            with(&[("events", &kicks)]),
            json!({"threshold": 2}),
            Some("threshold"),
        ),
        (
            "an unknown parameter",
            &lift,
            vec![one.clone()],
            with(&[]),
            json!({"depth": 1}),
            Some("depth"),
        ),
    ];
    for (row, called, signals, inputs, values, refusal) in rows {
        let result = called.call(&Call {
            signals,
            inputs,
            values,
        });
        let found = match &result {
            Ok(_) => None,
            Err(Error::Call { message }) => Some(message.clone()),
            Err(Error::Params { shaper, problems, .. }) if shaper == called.name() => Some(
                problems
                    .iter()
                    .map(|problem| problem.path.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            Err(other) => Some(format!("another error: {other:?}")),
        };
        assert_eq!(found.as_deref(), refusal, "{row}");
    }
    Ok(())
}

#[test]
fn the_thing_gets_its_parameters_as_the_table_read_them_and_only_its_inputs() -> Result<(), Error> {
    let mut phase = number("phase", 0.25, ParamUnit::Cycles, (0.0, 1.0));
    phase.cyclic = true;
    let about = About::default();
    let declared = Declared {
        name: "phase".to_owned(),
        kind: Kind::Source,
        identity: "tests.phase".to_owned(),
        shown: None,
        table: ParamTable::new(vec![phase, switch("on")]).map_err(|error| Error::params("phase", error))?,
        inputs: vec![(InputKind::Grid, false)],
        returns: Returns::Signal,
    };
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&seen);
    let thing = Thing::custom(about, declared, move |call| {
        if let Ok(mut seen) = kept.lock() {
            seen.push((call.values.clone(), call.inputs.keys().cloned().collect::<Vec<_>>()));
        }
        Ok(Output::Signal(Signal::constant(1.0)?))
    })?;
    thing.call(&Call::default())?;
    thing.call(&Call {
        values: json!({"phase": 1.75, "on": false}),
        ..Call::default()
    })?;
    let seen = seen.lock().map(|seen| seen.clone()).unwrap_or_default();
    // the defaults filled in, the phase wrapped into its cycle
    assert_eq!(
        seen,
        [
            (json!({"phase": 0.25, "on": true}), vec![]),
            (json!({"phase": 0.75, "on": false}), vec![]),
        ]
    );
    Ok(())
}

/// The error a body gives.
#[derive(Debug)]
struct Broke;

impl std::fmt::Display for Broke {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the body broke")
    }
}

impl std::error::Error for Broke {}

#[test]
fn an_error_of_the_body_reaches_the_caller_unchanged() -> Result<(), Error> {
    let about = About::default();
    let declared = Declared {
        name: "broken".to_owned(),
        kind: Kind::Source,
        identity: "tests.broken".to_owned(),
        shown: None,
        table: ParamTable::new(vec![]).map_err(|error| Error::params("broken", error))?,
        inputs: vec![],
        returns: Returns::Signal,
    };
    let broken = Thing::custom(about, declared, |_call| {
        Err(Error::Node {
            source: Arc::new(Broke),
        })
    })?;
    let result = broken.call(&Call::default());
    let source = match &result {
        Err(error) => std::error::Error::source(error).and_then(|source| source.downcast_ref::<Broke>()),
        Ok(_) => None,
    };
    assert!(source.is_some(), "{result:?}");
    assert_eq!(
        result.map(|_| ()).map_err(|error| error.to_string()),
        Err("the body broke".to_owned())
    );
    Ok(())
}

#[test]
fn a_thing_that_gives_another_kind_of_result_than_it_returns_is_refused() -> Result<(), Error> {
    let about = About::default();
    let declared = Declared {
        name: "liar".to_owned(),
        kind: Kind::Operation,
        identity: "tests.liar".to_owned(),
        shown: None,
        table: ParamTable::new(vec![]).map_err(|error| Error::params("liar", error))?,
        inputs: vec![],
        returns: Returns::Series,
    };
    let liar = Thing::custom(about, declared, |_call| Ok(Output::Events(vec![])))?;
    let result = liar.call(&Call::default());
    assert!(
        matches!(&result, Err(Error::Call { message }) if message == "operation 'liar' gave events, not the series it returns"),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_thing_answers_what_it_is_takes_and_gives() -> Result<(), String> {
    let about = About {
        doc: "The events\n  kept, in   order.\n\nEach one unchanged.".to_owned(),
        ..About::default()
    };
    let declared = Declared {
        name: "pick".to_owned(),
        kind: Kind::Operation,
        identity: "tests.pick".to_owned(),
        shown: None,
        table: table(vec![switch("on"), switch("loud")])?,
        inputs: vec![(InputKind::Grid, false), (InputKind::Events, true)],
        returns: Returns::Events,
    };
    let pick = Thing::custom(about, declared, echo).map_err(|error| error.to_string())?;
    assert_eq!(
        (pick.name(), pick.kind(), pick.returns()),
        ("pick", Kind::Operation, Returns::Events)
    );
    assert_eq!(pick.summary(), "The events kept, in order.");
    assert_eq!(pick.inputs(), [(InputKind::Grid, false), (InputKind::Events, true)]);
    let names: Vec<&str> = pick.table().params().iter().map(|param| param.name.as_str()).collect();
    assert_eq!(names, ["on", "loud"]);
    Ok(())
}

/// What `help()` says of the inputs `events` and `grid` (the core's input docs, as the Python package shows them).
const EVENTS_DOC: &str =
    "Events: a list of {t, strength, duration, label, pitch}, as in a bundle, in any order (checked into time order).";
const GRID_DOC: &str = "A beat grid: {beats, meter, first_downbeat} as in a bundle (beats in seconds, strictly \
                        increasing, at least two). Converts seconds to beats, so a source can follow tempo changes.";

#[test]
fn help_shows_the_call_the_doc_the_inputs_and_each_parameter_in_its_own_terms() -> Result<(), String> {
    let mut rate = number("rate", 1000.0, ParamUnit::Hertz, (10.0, 48000.0));
    rate.advanced = true;
    let mut phase = number("phase", 0.0, ParamUnit::Cycles, (0.0, 1.0));
    phase.cyclic = true;
    let every = Param {
        kind: ParamKind::Whole,
        default: 4.into(),
        ..number("every", 0.0, ParamUnit::Count, (1.0, 64.0))
    };
    let params = vec![
        rate,
        number("threshold", 0.5, ParamUnit::Fraction, (0.0, 1.0)),
        plain("label", ParamKind::Text, "it's".into()),
        every,
        phase,
    ];
    let declared = Declared {
        name: "pick".to_owned(),
        kind: Kind::Operation,
        identity: "tests.pick".to_owned(),
        shown: None,
        table: table(params)?,
        inputs: vec![(InputKind::Events, true), (InputKind::Grid, false)],
        returns: Returns::Events,
    };
    let about = About {
        doc: "The events kept.\n\nEach one unchanged.".to_owned(),
        ..About::default()
    };
    let pick = Thing::custom(about, declared, echo).map_err(|error| error.to_string())?;
    let expected = format!(
        "operation pick(*, events, grid=None, **values) -> events

The events kept.

Each one unchanged.

Inputs:
  events (required)
      {EVENTS_DOC}
  grid (optional)
      {GRID_DOC}

Parameters:
  threshold (fraction, 0 to 1), default 0.5
      The threshold.
  label, default \"it's\"
      The label.
  every (count, 1 to 64), default 4
      The every.
  phase (cycles, 0 to 1), default 0.0, cyclic
      The phase.
  rate (hertz, 10 to 48000), default 1000.0, advanced
      The rate."
    );
    assert_eq!(pick.help(), expected);
    Ok(())
}

#[test]
fn help_of_a_thing_without_inputs_parameters_or_a_doc_is_its_call() -> Result<(), String> {
    let help = |kind, doc: &str, params| -> Result<String, String> {
        let about = About {
            doc: doc.to_owned(),
            ..About::default()
        };
        let declared = Declared {
            name: "lift".to_owned(),
            kind,
            identity: "tests.lift".to_owned(),
            shown: None,
            table: table(params)?,
            inputs: vec![],
            returns: Returns::Signal,
        };
        Ok(Thing::custom(about, declared, echo)
            .map_err(|error| error.to_string())?
            .help())
    };
    assert_eq!(
        help(Kind::Shaper, "Raises it.", vec![])?,
        "shaper lift(signal, **values) -> signal\n\nRaises it."
    );
    assert_eq!(help(Kind::Source, "", vec![])?, "source lift(**values) -> signal");
    assert_eq!(
        help(Kind::Source, "", vec![switch("on")])?,
        "source lift(**values) -> signal\n\n\n\nParameters:\n  on, default True\n      The on."
    );
    Ok(())
}

#[test]
fn help_writes_each_default_as_python_writes_the_value() -> Result<(), String> {
    let decimal = |default: f64| number("level", default, ParamUnit::Ratio, (-1e30, 1e30));
    let whole = |default: i64| Param {
        kind: ParamKind::Whole,
        default: default.into(),
        ..number("level", 0.0, ParamUnit::Count, (0.0, 4_294_967_295.0))
    };
    let text = |default: &str| plain("level", ParamKind::Text, default.into());
    let choice = plain(
        "level",
        ParamKind::Choice(vec!["saw-down".to_owned()]),
        "saw-down".into(),
    );
    // (the field, what Python's repr writes for its default: each from `python3 -c 'print(repr(...))'`)
    let rows = [
        (decimal(0.1), "0.1"),
        (decimal(1000.0), "1000.0"),
        (decimal(1e16), "1e+16"),
        (decimal(1e15), "1000000000000000.0"),
        (decimal(1e-5), "1e-05"),
        (decimal(5e-5), "5e-05"),
        (decimal(0.0001), "0.0001"),
        (decimal(1.5e-7), "1.5e-07"),
        (decimal(123_456.789), "123456.789"),
        (decimal(2.5e22), "2.5e+22"),
        (decimal(-2.5), "-2.5"),
        (whole(4_294_967_295), "4294967295"),
        (text("it's"), "\"it's\""),
        (text("say \"hi\""), "'say \"hi\"'"),
        (text("it's \"x\""), "'it\\'s \"x\"'"),
        (text("a\\b"), "'a\\\\b'"),
        (text("tab\there"), "'tab\\there'"),
        (text("bell\u{7}"), "'bell\\x07'"),
        (text("é"), "'é'"),
        (plain("level", ParamKind::Switch, false.into()), "False"),
        (choice, "'saw-down'"),
    ];
    for (param, written) in rows {
        let declared = Declared {
            name: "level".to_owned(),
            kind: Kind::Source,
            identity: "tests.level".to_owned(),
            shown: None,
            table: table(vec![param])?,
            inputs: vec![],
            returns: Returns::Signal,
        };
        let about = About::default();
        let help = Thing::custom(about, declared, echo)
            .map_err(|error| error.to_string())?
            .help();
        let line = help
            .lines()
            .find(|line| line.starts_with("  level"))
            .unwrap_or_default();
        let default = line.split(", default ").nth(1).unwrap_or_default();
        assert_eq!(default, written, "{help}");
    }
    Ok(())
}

#[test]
fn a_description_is_what_the_python_package_lists() -> Result<(), String> {
    let mut rate = number("rate", 1000.0, ParamUnit::Hertz, (10.0, 48000.0));
    rate.advanced = true;
    let about = About {
        doc: "The events kept.\n\nEach one unchanged.".to_owned(),
        params_title: "PickParams".to_owned(),
        params_doc: "Which events to keep.".to_owned(),
    };
    let declared = Declared {
        name: "pick".to_owned(),
        kind: Kind::Operation,
        identity: "tests.pick".to_owned(),
        shown: None,
        table: table(vec![rate])?,
        inputs: vec![(InputKind::Events, true), (InputKind::Grid, false)],
        returns: Returns::Events,
    };
    let pick = Thing::custom(about, declared, echo).map_err(|error| error.to_string())?;
    assert_eq!(
        pick.description(),
        json!({
            "kind": "operation",
            "summary": "The events kept.",
            "returns": "events",
            "inputs": {"events": {"required": true, "doc": EVENTS_DOC}, "grid": {"required": false, "doc": GRID_DOC}},
            "parameters": {
                "additionalProperties": false,
                "properties": {"rate": {"type": "number", "default": 1000.0, "title": "Rate", "unit": "hertz",
                    "description": "The rate.", "minimum": 10.0, "maximum": 48000.0, "advanced": true, "cyclic": false}},
                "description": "Which events to keep.",
                "title": "PickParams",
                "type": "object",
            },
        })
    );
    // parameters without a doc have no description
    let bare =
        thing(("bare", "tests.bare"), Kind::Source, vec![], &[], Returns::Signal).map_err(|error| error.to_string())?;
    assert_eq!(
        bare.description()["parameters"],
        json!({"additionalProperties": false, "properties": {}, "title": "", "type": "object"})
    );
    Ok(())
}

#[test]
fn a_registry_and_its_things_go_to_any_thread() {
    fn shared<T: Send + Sync>() {}
    shared::<Registry>();
    shared::<Thing>();
    shared::<Call>();
    shared::<Output>();
}

/// How many callbacks run at once in [`calls_from_many_threads_run_their_callbacks_at_the_same_time`].
const THREADS: usize = 16;

#[test]
fn calls_from_many_threads_run_their_callbacks_at_the_same_time() -> Result<(), Error> {
    // each callback waits until every thread's callback has started: it can only return if none of them holds a lock
    // the others need (a hard deadline turns a lock held across the callback into a failure, not a hang)
    let arrived = Arc::new((Mutex::new(0_usize), Condvar::new()));
    let gate = Arc::clone(&arrived);
    let about = About::default();
    let declared = Declared {
        name: "together".to_owned(),
        kind: Kind::Source,
        identity: "tests.together".to_owned(),
        shown: None,
        table: ParamTable::new(vec![]).map_err(|error| Error::params("together", error))?,
        inputs: vec![],
        returns: Returns::Signal,
    };
    let together = Thing::custom(about, declared, move |_call| {
        let (count, all) = &*gate;
        let all_there = count.lock().ok().and_then(|mut count| {
            *count += 1;
            all.notify_all();
            all.wait_timeout_while(count, Duration::from_secs(10), |count| *count < THREADS)
                .ok()
        });
        match all_there {
            Some((_, waited)) if !waited.timed_out() => Ok(Output::Signal(Signal::constant(1.0)?)),
            _ => Err(Error::Signal {
                message: "not every callback ran at once".to_owned(),
            }),
        }
    })?;
    let mut registry = Registry::new();
    registry.register(together)?;
    let registry = Arc::new(registry);
    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || registry.get("together")?.call(&Call::default()).map(|_| ()))
        })
        .collect();
    for handle in handles {
        let result = handle.join().map_err(|_| Error::Signal {
            message: "a thread panicked".to_owned(),
        })?;
        result?;
    }
    Ok(())
}

#[test]
fn the_built_in_slew_says_what_the_python_package_says_of_it() -> Result<(), Error> {
    let slew = Registry::builtin().get("slew")?;
    assert_eq!(
        slew.help(),
        "shaper slew(signal, **values) -> signal

A signal that follows its input but climbs and falls no faster than `rise` and `fall` allow.

It starts at the input's value at time 0. A limit is a time per unit of change: rise = 2 means at most 0.5 per
second.

Parameters:
  rise (seconds, 0 to 3600), default 0.1
      Seconds the output takes to climb by 1; larger is slower. 0 follows a rise at once.
  fall (seconds, 0 to 3600), default 0.1
      Seconds the output takes to fall by 1; larger is slower. 0 follows a fall at once.
  rate (hertz, 10 to 48000), default 1000.0, advanced
      Internal samples per second; the output is exact to one sample. Raising it is more exact and slower."
    );
    assert_eq!(
        slew.description(),
        json!({
            "kind": "shaper",
            "summary": "A signal that follows its input but climbs and falls no faster than `rise` and `fall` allow.",
            "returns": "signal",
            "inputs": {},
            "parameters": {
                "additionalProperties": false,
                "properties": {
                    "rise": {"advanced": false, "cyclic": false, "default": 0.1,
                        "description": "Seconds the output takes to climb by 1; larger is slower. 0 follows a rise at once.",
                        "maximum": 3600.0, "minimum": 0.0, "title": "Rise", "type": "number", "unit": "seconds"},
                    "fall": {"advanced": false, "cyclic": false, "default": 0.1,
                        "description": "Seconds the output takes to fall by 1; larger is slower. 0 follows a fall at once.",
                        "maximum": 3600.0, "minimum": 0.0, "title": "Fall", "type": "number", "unit": "seconds"},
                    "rate": {"advanced": true, "cyclic": false, "default": 1000.0,
                        "description": "Internal samples per second; the output is exact to one sample. Raising it is more exact and slower.",
                        "maximum": 48000.0, "minimum": 10.0, "title": "Rate", "type": "number", "unit": "hertz"},
                },
                "description": "How fast the output may climb and fall.",
                "title": "SlewParams",
                "type": "object",
            },
        })
    );
    let follow = Registry::builtin().get("follow")?;
    assert_eq!(follow.description()["parameters"]["title"], "FollowParams");
    assert!(
        follow
            .summary()
            .starts_with("A signal that chases its input like a capacitor:"),
        "{}",
        follow.summary()
    );
    Ok(())
}

#[test]
fn a_built_in_called_by_name_is_the_typed_shaper() -> Result<(), Error> {
    use crate::builtin::slew;
    use crate::{FromValues, Interpolation};

    let step = Signal::from_values(&[0.0, 1.0, 0.0, 0.0], 10.0, FromValues::default(), Interpolation::Hold)?;
    let call = Call {
        signals: vec![step.clone()],
        values: json!({"rise": 0.25, "fall": 0.5, "rate": 100.0}),
        ..Call::default()
    };
    let Output::Signal(by_name) = Registry::builtin().get("slew")?.call(&call)? else {
        return Err(Error::Signal {
            message: "slew gave no signal".to_owned(),
        });
    };
    let typed = step.slew(slew::Params {
        rise: 0.25,
        fall: 0.5,
        rate: 100.0,
    })?;
    let times = [0.0, 0.1, 0.15, 0.2, 0.25, 0.4, 1.0];
    assert_eq!(by_name.sample(&times)?, typed.sample(&times)?);
    // 0.01 s a sample, a climb of 1 in 0.25 s: 0.04 a sample, from the sample at 0.1 s: 6 of them by 0.15 s
    assert!((by_name.at(0.15)? - 0.24).abs() < 1e-9, "{}", by_name.at(0.15)?);
    Ok(())
}

/// How many of the first events to keep (an operation of these tests).
#[derive(Clone, Debug, PartialEq, crate::Params)]
struct First {
    /// How many of the first events are kept.
    #[param(default = 1, unit = Count, min = 0, max = 100)]
    n: i64,
}

impl crate::Operation for First {
    const NAME: &'static str = "first";
    type Inputs = Vec<Event>;
    const RETURNS: Returns = Returns::Events;

    fn run(&self, events: Vec<Event>) -> Result<Output, Error> {
        Ok(Output::Events(
            events.into_iter().take(usize::try_from(self.n).unwrap_or(0)).collect(),
        ))
    }
}

/// An event on each beat of the grid, none without one (an operation of these tests).
#[derive(Clone, Debug, PartialEq, crate::Params)]
struct Beats {}

impl crate::Operation for Beats {
    const NAME: &'static str = "beats";
    type Inputs = Option<Grid>;
    const RETURNS: Returns = Returns::Events;

    fn run(&self, grid: Option<Grid>) -> Result<Output, Error> {
        let beats = grid.map(|grid| grid.beats().to_vec()).unwrap_or_default();
        Ok(Output::Events(events(&beats)?))
    }
}

/// The events given, last first (an operation of these tests that breaks the time order it promises).
#[derive(Clone, Debug, PartialEq, crate::Params)]
struct Backwards {}

impl crate::Operation for Backwards {
    const NAME: &'static str = "backwards";
    type Inputs = Vec<Event>;
    const RETURNS: Returns = Returns::Events;

    fn run(&self, events: Vec<Event>) -> Result<Output, Error> {
        Ok(Output::Events(events.into_iter().rev().collect()))
    }
}

/// A row of [`events_a_thing_gives_are_in_time_order_or_refused_naming_it`]: (row, the times a body gives, the times a
/// call gives or its refusal).
type OrderRow = (&'static str, Vec<f64>, Result<Vec<f64>, String>);

#[test]
fn events_a_thing_gives_are_in_time_order_or_refused_naming_it() -> Result<(), Error> {
    let backwards = Thing::operation::<Backwards>(About::default())?;
    let call = |times: &[f64]| -> Result<Call, Error> {
        Ok(Call {
            inputs: [("events".to_owned(), Input::from_events(events(times)?))].into(),
            ..Call::default()
        })
    };
    // the input arrives sorted (0.5, 1, 2), so the body gives 2, 1, 0.5: the event at index 1 comes after a later one
    assert_eq!(
        times(backwards.call(&call(&[1.0, 0.5, 2.0])?)),
        Err(
            "operation 'backwards' returned events out of time order (event 1 at 1 s comes after one at 2 s)"
                .to_owned()
        )
    );
    assert_eq!(times(backwards.call(&call(&[1.5])?)), Ok(vec![1.5]));

    // a callback's (a Python body's) events are held to the same rule; events at one time keep their order
    let rows: [OrderRow; 4] = [
        (
            "in time order, two at once",
            vec![1.0, 2.0, 2.0, 3.0],
            Ok(vec![1.0, 2.0, 2.0, 3.0]),
        ),
        ("none", vec![], Ok(vec![])),
        (
            "later first",
            vec![3.0, 1.0],
            Err(
                "operation 'given' returned events out of time order (event 1 at 1 s comes after one at 3 s)"
                    .to_owned(),
            ),
        ),
        (
            "one out of order at the end",
            vec![1.0, 2.0, 1.5],
            Err(
                "operation 'given' returned events out of time order (event 2 at 1.5 s comes after one at 2 s)"
                    .to_owned(),
            ),
        ),
    ];
    for (row, given, expected) in rows {
        let declared = Declared {
            name: "given".to_owned(),
            kind: Kind::Operation,
            identity: "tests.given".to_owned(),
            shown: None,
            table: ParamTable::new(vec![]).map_err(|error| Error::params("given", error))?,
            inputs: vec![],
            returns: Returns::Events,
        };
        let made = events(&given)?;
        let thing = Thing::custom(About::default(), declared, move |_call| {
            Ok(Output::Events(made.clone()))
        })?;
        assert_eq!(times(thing.call(&Call::default())), expected, "{row}");
    }
    Ok(())
}

/// The events of `output`'s times, or why there are none.
fn times(output: Result<Output, Error>) -> Result<Vec<f64>, String> {
    match output {
        Ok(Output::Events(events)) => Ok(events.iter().map(Event::t).collect()),
        Ok(other) => Err(format!("not events: {other:?}")),
        Err(error) => Err(error.to_string()),
    }
}

#[test]
fn a_rust_operation_takes_its_inputs_by_their_kinds() -> Result<(), Error> {
    let about = || About {
        doc: "Keeps some.".to_owned(),
        ..About::default()
    };
    let first = Thing::operation::<First>(about())?;
    assert_eq!(
        (first.kind(), first.returns(), first.inputs()),
        (Kind::Operation, Returns::Events, &[(InputKind::Events, true)][..])
    );
    assert!(
        first
            .help()
            .starts_with("operation first(*, events, **values) -> events\n"),
        "{}",
        first.help()
    );
    let kicks = Input::from_events(events(&[1.0, 0.5, 2.0])?);
    let given = |n: i64, inputs: &[(&str, &Input)]| Call {
        inputs: inputs
            .iter()
            .map(|(name, input)| ((*name).to_owned(), (*input).clone()))
            .collect(),
        values: json!({ "n": n }),
        ..Call::default()
    };
    // in time order, the first two
    assert_eq!(times(first.call(&given(2, &[("events", &kicks)]))), Ok(vec![0.5, 1.0]));
    assert_eq!(
        times(first.call(&given(2, &[]))),
        Err("operation 'first' needs the input 'events'".to_owned())
    );

    let beats = Thing::operation::<Beats>(about())?;
    assert_eq!(beats.inputs(), [(InputKind::Grid, false)]);
    assert!(
        beats
            .help()
            .starts_with("operation beats(*, grid=None, **values) -> events\n"),
        "{}",
        beats.help()
    );
    let grid = Input::Grid(Grid::new(vec![0.0, 0.5, 1.0], 4, 0).map_err(|source| Error::Format { source })?);
    let call = Call {
        inputs: [("grid".to_owned(), grid)].into(),
        ..Call::default()
    };
    assert_eq!(times(beats.call(&call)), Ok(vec![0.0, 0.5, 1.0]));
    assert_eq!(times(beats.call(&Call::default())), Ok(vec![]));
    Ok(())
}

/// The signal `output` holds, or why there is none.
fn signal(output: Result<Output, Error>) -> Result<Signal, Error> {
    match output? {
        Output::Signal(signal) => Ok(signal),
        other => Err(Error::Signal {
            message: format!("not a signal: {other:?}"),
        }),
    }
}

#[test]
fn a_signal_made_through_the_registry_carries_its_recipe() -> Result<(), Error> {
    use crate::{FromValues, Interpolation};

    let step = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
    let slew = Registry::builtin().get("slew")?;
    let slewed = signal(slew.call(&Call {
        signals: vec![step.clone()],
        values: json!({"rise": 0.5, "rate": 10.0}),
        ..Call::default()
    }))?;
    let recipe = slewed.recipe();
    // the values as the table read them: every field, `fall` at its default
    assert_eq!(
        recipe.map(|recipe| (recipe.name.as_str(), recipe.values.clone(), recipe.inputs.len())),
        Some(("slew", json!({"rise": 0.5, "fall": 0.1, "rate": 10.0}), 0))
    );
    // the signal it shaped, which reads as the step
    let shaped = recipe
        .and_then(|recipe| recipe.signal.as_ref())
        .map(|input| input.sample(&[0.0, 0.1]));
    assert_eq!(shaped.transpose()?, Some(vec![0.0, 1.0]));
    // it reads as the shaper's signal: 0.1 s a sample, 0.5 s to climb by 1, 0.2 a sample
    assert_eq!(slewed.sample(&[0.0, 0.1, 0.2])?, [0.0, 0.2, 0.4]);

    // a shaper of a recorded signal: each keeps its own, the upstream one through `signal`
    let followed = signal(Registry::builtin().get("follow")?.call(&Call {
        signals: vec![slewed],
        ..Call::default()
    }))?;
    let names: Vec<&str> = std::iter::successors(followed.recipe(), |recipe| {
        recipe.signal.as_ref().and_then(Signal::recipe)
    })
    .map(|recipe| recipe.name.as_str())
    .collect();
    assert_eq!(names, ["follow", "slew"]);
    Ok(())
}

#[test]
fn a_recipe_keeps_the_inputs_in_the_order_the_thing_declares_them() -> Result<(), Error> {
    let about = About::default();
    let declared = Declared {
        name: "track".to_owned(),
        kind: Kind::Source,
        identity: "tests.track".to_owned(),
        shown: None,
        table: ParamTable::new(vec![switch("on")]).map_err(|error| Error::params("track", error))?,
        inputs: vec![
            (InputKind::Values, true),
            (InputKind::Events, false),
            (InputKind::Grid, false),
        ],
        returns: Returns::Signal,
    };
    let track = Thing::custom(about, declared, |_call| Ok(Output::Signal(Signal::constant(0.5)?)))?;
    let values = Input::from_values(vec![0.25, 0.75])?;
    let kicks = Input::from_events(events(&[1.0])?);
    let call = Call {
        inputs: [
            ("events".to_owned(), kicks.clone()),
            ("values".to_owned(), values.clone()),
        ]
        .into(),
        ..Call::default()
    };
    let tracked = signal(track.call(&call))?;
    let recipe = tracked.recipe();
    assert_eq!(
        recipe.map(|recipe| (recipe.inputs.clone(), recipe.values.clone(), recipe.signal.is_none())),
        Some((
            vec![("values".to_owned(), values), ("events".to_owned(), kicks)],
            json!({"on": true}),
            true
        ))
    );
    Ok(())
}

#[test]
fn a_signal_made_any_other_way_has_no_recipe() -> Result<(), Error> {
    let level = Signal::constant(0.5)?;
    let echo = Signal::from_fn(|times, out| {
        out.copy_from_slice(times);
        Ok::<(), std::fmt::Error>(())
    });
    let made = [
        level.clone(),
        echo.clone(),
        &level + &echo,
        Signal::rendered(&level, crate::Rendering::default(), |inputs| {
            Ok::<_, std::fmt::Error>(inputs.to_vec())
        })?,
    ];
    assert!(made.iter().all(|signal| signal.recipe().is_none()));
    // nor does a signal combined with a recorded one: the recipe is the recorded signal's, not its users'
    let recorded = signal(Registry::builtin().get("slew")?.call(&Call {
        signals: vec![level],
        ..Call::default()
    }))?;
    assert!(recorded.recipe().is_some());
    assert!((&recorded * 2.0).recipe().is_none());
    Ok(())
}

#[test]
fn a_recipe_does_not_count_toward_the_depth_of_a_graph() -> Result<(), Error> {
    let mut deep = Signal::constant(0.0)?; // 1 deep
    for _ in 3..crate::MAX_DEPTH {
        deep = deep + 1.0;
    }
    // 998 deep; the shaper makes it 999, recorded or not, and one more node makes it 1000, the most there may be
    let slewed = signal(Registry::builtin().get("slew")?.call(&Call {
        signals: vec![deep],
        values: json!({"rise": 0}),
        ..Call::default()
    }))?;
    assert!(slewed.recipe().is_some());
    assert_eq!((&slewed + 1.0).at(0.5)?, 998.0);
    let refused = (slewed + 1.0 + 1.0).at(0.0);
    assert!(matches!(&refused, Err(Error::Depth { .. })), "{refused:?}");
    Ok(())
}

#[test]
fn a_chain_of_recorded_signals_as_deep_as_there_may_be_reads_on_a_test_thread() -> Result<(), Error> {
    // each call adds one node and its recipe: a read goes through both, 2 frames a level
    let about = About::default();
    let declared = Declared {
        name: "lift".to_owned(),
        kind: Kind::Shaper,
        identity: "tests.lift".to_owned(),
        shown: None,
        table: ParamTable::new(vec![]).map_err(|error| Error::params("lift", error))?,
        inputs: vec![],
        returns: Returns::Signal,
    };
    let lift = Thing::custom(about, declared, |call| match call.signals.as_slice() {
        [input] => Ok(Output::Signal(input + 1.0)),
        _ => Err(Error::Signal {
            message: "one signal".to_owned(),
        }),
    })?;
    let mut lifted = Signal::constant(0.0)?;
    for _ in 1..crate::MAX_DEPTH {
        lifted = signal(lift.call(&Call {
            signals: vec![lifted],
            ..Call::default()
        }))?;
    }
    assert_eq!(lifted.sample(&[0.0, 5.0])?, [999.0, 999.0]);
    Ok(())
}

/// A callback thing of `kind` called `name` without parameters or inputs, whose body is `run`.
fn bare(
    name: &str,
    kind: Kind,
    run: impl Fn(&Call) -> Result<Output, Error> + Send + Sync + 'static,
) -> Result<Thing, Error> {
    let about = About::default();
    let declared = Declared {
        name: name.to_owned(),
        kind,
        identity: format!("tests.{name}"),
        shown: None,
        table: ParamTable::new(vec![]).map_err(|error| Error::params(name, error))?,
        inputs: vec![],
        returns: Returns::Signal,
    };
    Thing::custom(about, declared, run)
}

#[test]
fn a_chain_of_calls_that_add_no_node_is_as_deep_as_one_that_does() -> Result<(), Error> {
    // a read and a drop go through each recorded signal and the signal its recipe keeps, so each call counts as
    // a node does: 999 calls on a constant are the 1000 deep there may be (as 999 `+ 1.0` are), the next is refused
    let identity = bare("identity", Kind::Shaper, |call| match call.signals.as_slice() {
        [input] => Ok(Output::Signal(input.clone())),
        _ => Err(Error::Signal {
            message: "one signal".to_owned(),
        }),
    })?;
    let mute = bare("mute", Kind::Shaper, |_call| Ok(Output::Signal(Signal::constant(0.0)?)))?;
    // a source whose body gives the signal its last call gave: recorded signals inside recorded ones
    let last = Arc::new(Mutex::new(Signal::constant(0.0)?));
    let kept = Arc::clone(&last);
    let again = bare("again", Kind::Source, move |_call| {
        kept.lock()
            .map(|signal| Output::Signal(signal.clone()))
            .map_err(|_| Error::Signal {
                message: "poisoned".to_owned(),
            })
    })?;
    for (row, thing, value) in [
        ("identity", &identity, 1.0),
        ("mute", &mute, 0.0),
        ("again", &again, 0.0),
    ] {
        let mut chain = Signal::constant(1.0)?;
        if let Ok(mut first) = last.lock() {
            *first = Signal::constant(0.0)? + 0.0; // 2 deep, as a constant shaped once is
        }
        for _ in 1..crate::MAX_DEPTH {
            let signals = if thing.kind() == Kind::Shaper {
                vec![chain]
            } else {
                vec![]
            };
            chain = signal(thing.call(&Call {
                signals,
                ..Call::default()
            }))?;
            if let Ok(mut kept) = last.lock() {
                *kept = chain.clone();
            }
        }
        assert_eq!(chain.sample(&[0.0, 5.0])?, [value, value], "{row}");
        let signals = if thing.kind() == Kind::Shaper {
            vec![chain]
        } else {
            vec![]
        };
        let refused = thing.call(&Call {
            signals,
            ..Call::default()
        });
        assert!(
            matches!(&refused, Err(Error::Depth { .. })),
            "{row}: {:?}",
            refused.map(|_| "a signal")
        );
    }
    Ok(())
}

/// How one level of a chain is made from the level below.
type Step = Box<dyn Fn(&Signal) -> Result<Signal, Error>>;

#[test]
fn a_signal_formats_as_its_own_node_however_deep_the_graph_upstream() -> Result<(), Error> {
    // a recipe keeps the signal shaped, and a body that gives it back holds it again: a format that printed either
    // would grow with every level, doubling where a body gives its input; so would a derive over anything holding one
    use crate::builtin::slew;
    use crate::{FromValues, Interpolation, MAX_DEPTH, Rendering, maximum, minimum};

    let identity = bare("identity", Kind::Shaper, |call| match call.signals.as_slice() {
        [input] => Ok(Output::Signal(input.clone())),
        _ => Err(Error::Signal {
            message: "one signal".to_owned(),
        }),
    })?;
    let call = |thing: &Thing, input: &Signal| {
        signal(thing.call(&Call {
            signals: vec![input.clone()],
            ..Call::default()
        }))
    };
    let follow = Registry::builtin().get("follow")?.clone();
    let series = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Linear)?;
    let echo = Signal::from_fn(|times, out| {
        out.copy_from_slice(times);
        Ok::<(), std::fmt::Error>(())
    });
    let constant = Signal::constant(0.5)?;
    // each step is one level deeper, so 999 of them over a constant make the deepest graph there is
    let steps: Vec<(&str, Step)> = vec![
        ("add", Box::new(|input| Ok(input + 0.5))),
        ("maximum", Box::new(move |input| Ok(maximum(input, &series)))),
        ("multiply", Box::new(move |input| Ok(input * &echo))),
        ("minimum", Box::new(move |input| Ok(minimum(input, &constant)))),
        ("multiply", Box::new(|input| Ok(-input))),
        ("subtract", Box::new(|input| Ok(1.0 - input))),
        ("slew", Box::new(|input| input.slew(slew::Params::default()))),
        ("follow", Box::new(move |input| call(&follow, input))),
        (
            "rendered",
            Box::new(|input| {
                Signal::rendered(input, Rendering::default(), |values| {
                    Ok::<_, std::fmt::Error>(values.to_vec())
                })
            }),
        ),
        ("identity", Box::new(move |input| call(&identity, input))),
    ];
    // a few bytes a level: the node's label, the depth's digits, a recipe's own values, never the graph upstream
    let mut chain = Signal::constant(1.0)?;
    for (level, (label, step)) in (2..=MAX_DEPTH).zip(steps.iter().cycle()) {
        chain = step(&chain)?;
        let text = format!("{chain:?}");
        assert!(
            text.contains(&format!("\"{label}\"")) && text.len() <= 48,
            "{label} at {level}: {text}"
        );
        if let Some(recipe) = chain.recipe() {
            let text = format!("{recipe:?}");
            assert!(text.len() < 400, "{label} at {level}: {} bytes", text.len());
        }
    }
    assert!(chain.at(0.0).is_ok() && matches!((&chain + 1.0).at(0.0), Err(Error::Depth { .. })));
    assert!(format!("{:?}", chain + f64::NAN).contains("\"refused\""));
    Ok(())
}

/// A plugin's shaper of these tests, registered by its trait under a name that is not its type's path: what lies
/// above 1 - `depth` is mirrored below it.
#[derive(Clone, Debug, PartialEq, crate::Params)]
struct Fold {
    /// How far down from 1 the fold is.
    #[param(default = 0.5, unit = Fraction, min = 0, max = 1)]
    depth: f64,
}

impl crate::Shaper for Fold {
    const NAME: &'static str = "fold";

    fn shape(&self, input: &Signal) -> Result<Signal, Error> {
        let (line, input) = (1.0 - self.depth, input.clone());
        Ok(Signal::from_fn(move |times, out| {
            input.read_into(times, out)?;
            for value in out.iter_mut() {
                if *value > line {
                    *value = 2.0 * line - *value;
                }
            }
            Ok::<(), Error>(())
        }))
    }
}

/// A plugin's source of these tests: a level that holds.
#[derive(Clone, Debug, PartialEq, crate::Params)]
struct Level {
    /// The value it holds.
    #[param(default = 0.5, unit = Ratio, min = -1, max = 1)]
    value: f64,
}

impl crate::Source for Level {
    const NAME: &'static str = "level";

    fn source(&self) -> Result<Signal, Error> {
        Signal::constant(self.value)
    }
}

/// What a door made, as the doors are compared: the signal's values at [`READ_AT`], its recipe (name, values, inputs,
/// the upstream signal's values at [`READ_AT`], whether the thing of that name in the registry made it), and how long
/// its `{:?}` is at most; or the error's text.
type Made = Result<
    (
        Vec<f64>,
        String,
        serde_json::Value,
        Vec<(String, Input)>,
        Option<Vec<f64>>,
        bool,
        bool,
    ),
    String,
>;

/// The times a door's signal and its upstream are read at.
const READ_AT: [f64; 4] = [0.0, 0.1, 0.2, 1.0];

/// What `made` is, as [`Made`] compares it, against `registry`.
fn made(made: Result<Signal, Error>, registry: &Registry) -> Made {
    let text = |error: Error| error.to_string();
    let signal = made.map_err(text)?;
    let recipe = signal.recipe().ok_or("no recipe")?;
    let upstream = recipe
        .signal
        .as_ref()
        .map(|input| input.sample(&READ_AT))
        .transpose()
        .map_err(text)?;
    let thing = registry.get(&recipe.name).map_err(text)?;
    let bounded = format!("{signal:?}").len() < 200 && format!("{recipe:?}").len() < 600;
    Ok((
        signal.sample(&READ_AT).map_err(text)?,
        recipe.name.clone(),
        recipe.values.clone(),
        recipe.inputs.clone(),
        upstream,
        recipe.made_by(thing),
        bounded,
    ))
}

/// A row of [`every_door_records_the_same_recipe_and_names_the_thing_alike`]: (thing, door, what it made).
type DoorRow = (&'static str, &'static str, Result<Signal, Error>);

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "a row per thing and door, good and bad, and the hand-worked result of each"
)]
fn every_door_records_the_same_recipe_and_names_the_thing_alike() -> Result<(), Error> {
    use crate::builtin::{follow, slew};
    use crate::{FromValues, Interpolation, shape};

    let mut registry = Registry::builtin().clone();
    registry.register(Thing::shaper::<Fold>(About::default())?)?;
    registry.register(Thing::source::<Level>(About::default())?)?;
    // a step from 0 to 1 at 0.1 s, at 10 samples a second
    let step = Signal::from_values(&[0.0, 1.0], 10.0, FromValues::default(), Interpolation::Hold)?;
    let by_name = |name: &str, values: serde_json::Value| -> Result<Signal, Error> {
        let signals = if name == "level" { vec![] } else { vec![step.clone()] };
        signal(registry.get(name)?.call(&Call {
            signals,
            values,
            ..Call::default()
        }))
    };
    let slew = slew::Params {
        rise: 0.5,
        rate: 10.0,
        ..slew::Params::default()
    };
    let follow = follow::Params {
        attack: 0.0,
        rate: 10.0,
        ..follow::Params::default()
    };
    let bad_slew = slew::Params { rise: -1.0, ..slew };
    let bad_follow = follow::Params {
        attack: 4000.0,
        ..follow
    };
    let good: Vec<DoorRow> = vec![
        (
            "slew",
            "Thing::call",
            by_name("slew", json!({"rise": 0.5, "rate": 10.0})),
        ),
        (
            "slew",
            "shape",
            shape("slew", &step, &json!({"rise": 0.5, "rate": 10.0})),
        ),
        ("slew", "pipe", step.pipe(slew)),
        ("slew", "typed method", step.slew(slew)),
        (
            "follow",
            "Thing::call",
            by_name("follow", json!({"attack": 0.0, "rate": 10.0})),
        ),
        (
            "follow",
            "shape",
            shape("follow", &step, &json!({"attack": 0.0, "rate": 10.0})),
        ),
        ("follow", "pipe", step.pipe(follow)),
        ("follow", "typed method", step.follow(follow)),
        ("fold", "Thing::call", by_name("fold", json!({"depth": 0.25}))),
        ("fold", "pipe", step.pipe(Fold { depth: 0.25 })),
        ("level", "Thing::call", by_name("level", json!({}))),
        ("level", "lib", super::sourced(Level::default())),
    ];
    let bad: Vec<DoorRow> = vec![
        (
            "slew",
            "Thing::call",
            by_name("slew", json!({"rise": -1.0, "rate": 10.0})),
        ),
        (
            "slew",
            "shape",
            shape("slew", &step, &json!({"rise": -1.0, "rate": 10.0})),
        ),
        ("slew", "pipe", step.pipe(bad_slew)),
        ("slew", "typed method", step.slew(bad_slew)),
        (
            "follow",
            "Thing::call",
            by_name("follow", json!({"attack": 4000.0, "rate": 10.0})),
        ),
        (
            "follow",
            "shape",
            shape("follow", &step, &json!({"attack": 4000.0, "rate": 10.0})),
        ),
        ("follow", "pipe", step.pipe(bad_follow)),
        ("follow", "typed method", step.follow(bad_follow)),
        ("fold", "Thing::call", by_name("fold", json!({"depth": 2.0}))),
        ("fold", "pipe", step.pipe(Fold { depth: 2.0 })),
        ("level", "Thing::call", by_name("level", json!({"value": 2.0}))),
        ("level", "lib", super::sourced(Level { value: 2.0 })),
    ];
    // worked out by hand: each recipe holds every field, a default where none was given
    let expected = |thing: &str| -> (Made, String) {
        let refused = |field: &str, rule: &str| format!("the parameters of {thing:?} do not fit: {field}: {rule}");
        match thing {
            // 0.1 s a sample, 0.5 s to climb by 1: 0.2 a sample, from the sample at 0.1 s
            "slew" => (
                Ok((
                    vec![0.0, 0.2, 0.4, 1.0],
                    "slew".to_owned(),
                    json!({"rise": 0.5, "fall": 0.1, "rate": 10.0}),
                    vec![],
                    Some(vec![0.0, 1.0, 1.0, 1.0]),
                    true,
                    true,
                )),
                refused("rise", "must be >= 0.0 (got -1.0)"),
            ),
            // an instant attack: the output is the input
            "follow" => (
                Ok((
                    vec![0.0, 1.0, 1.0, 1.0],
                    "follow".to_owned(),
                    json!({"attack": 0.0, "release": 0.25, "rate": 10.0}),
                    vec![],
                    Some(vec![0.0, 1.0, 1.0, 1.0]),
                    true,
                    true,
                )),
                refused("attack", "must be <= 3600.0 (got 4000.0)"),
            ),
            // the fold at 1 - 0.25: 1 is mirrored to 2 * 0.75 - 1 = 0.5
            "fold" => (
                Ok((
                    vec![0.0, 0.5, 0.5, 0.5],
                    "fold".to_owned(),
                    json!({"depth": 0.25}),
                    vec![],
                    Some(vec![0.0, 1.0, 1.0, 1.0]),
                    true,
                    true,
                )),
                refused("depth", "must be <= 1.0 (got 2.0)"),
            ),
            _ => (
                Ok((
                    vec![0.5; 4],
                    "level".to_owned(),
                    json!({"value": 0.5}),
                    vec![],
                    None,
                    true,
                    true,
                )),
                refused("value", "must be <= 1.0 (got 2.0)"),
            ),
        }
    };
    for (thing, door, result) in good {
        assert_eq!(made(result, &registry), expected(thing).0, "{thing} through {door}");
    }
    for (thing, door, result) in bad {
        let text = result.map(|_| ()).map_err(|error| error.to_string());
        assert_eq!(text, Err(expected(thing).1), "{thing} through {door}");
    }
    Ok(())
}

#[test]
fn a_rust_thing_is_identified_by_its_type_not_by_its_path() -> Result<(), Error> {
    // two types of one path (`type_name` cannot tell them apart), both calling themselves "twin"
    let (first, first_made) = {
        /// A twin.
        #[derive(Clone, Debug, PartialEq, crate::Params)]
        struct Twin {}
        impl crate::Shaper for Twin {
            const NAME: &'static str = "twin";
            fn shape(&self, input: &Signal) -> Result<Signal, Error> {
                Ok(input * 2.0)
            }
        }
        (
            Thing::shaper::<Twin>(About::default())?,
            Signal::constant(1.0)?.pipe(Twin {})?,
        )
    };
    let (second, second_made) = {
        /// The other twin.
        #[derive(Clone, Debug, PartialEq, crate::Params)]
        struct Twin {}
        impl crate::Shaper for Twin {
            const NAME: &'static str = "twin";
            fn shape(&self, input: &Signal) -> Result<Signal, Error> {
                Ok(input * 3.0)
            }
        }
        (
            Thing::shaper::<Twin>(About::default())?,
            Signal::constant(1.0)?.pipe(Twin {})?,
        )
    };
    let mut registry = Registry::new();
    registry.register(first.clone())?;
    registry.register(first)?; // the same type again: a reload
    assert_eq!(
        registration(registry.register(second)).as_deref(),
        Some(
            "shaper 'twin': the name is taken by the shaper defined as \
             bullterrier_control::registry::tests::a_rust_thing_is_identified_by_its_type_not_by_its_path::Twin"
        )
    );
    // a recipe knows the very thing that made it, not just its name
    let twin = registry.get("twin")?;
    let made_by = |made: &Signal| made.recipe().is_some_and(|recipe| recipe.made_by(twin));
    assert_eq!((made_by(&first_made), made_by(&second_made)), (true, false));
    // a callback whose identity is a Rust type's path is another thing, and the other way round
    let mut builtin = Registry::builtin().clone();
    let posing = thing(
        ("slew", "bullterrier_control::builtin::slew::Params"),
        Kind::Shaper,
        vec![],
        &[],
        Returns::Signal,
    )?;
    assert_eq!(
        registration(builtin.register(posing.clone())).as_deref(),
        Some("shaper 'slew': the name is taken by the shaper defined as bullterrier_control::builtin::slew::Params")
    );
    let mut callbacks = Registry::new();
    callbacks.register(posing)?;
    let rust = Thing::shaper::<crate::builtin::slew::Params>(About::default())?;
    assert!(registration(callbacks.register(rust)).is_some());
    Ok(())
}

#[test]
fn a_rust_thing_whose_name_breaks_the_rule_is_refused_by_every_door() -> Result<(), Error> {
    /// A shaper with a name that is not one.
    #[derive(Clone, Debug, PartialEq, crate::Params)]
    struct Loud {}
    impl crate::Shaper for Loud {
        const NAME: &'static str = "Loud";
        fn shape(&self, input: &Signal) -> Result<Signal, Error> {
            Ok(input.clone())
        }
    }
    let rule = format!("shaper 'Loud': {NAME_RULE}");
    assert_eq!(
        registration(Thing::shaper::<Loud>(About::default())).as_deref(),
        Some(rule.as_str())
    );
    assert_eq!(
        registration(Signal::constant(1.0)?.pipe(Loud {})).as_deref(),
        Some(rule.as_str())
    );
    Ok(())
}

#[test]
fn inputs_given_back_are_what_they_were_checked_into() -> Result<(), Error> {
    use super::Inputs;
    use crate::formats::{Series, SeriesOptions, Unit};
    use crate::inputs::Keyframe;

    let kicks = events(&[1.0, 0.5])?;
    let grid = Grid::new(vec![0.0, 0.5], 4, 0).map_err(|source| Error::Format { source })?;
    let series = Series::new(10.0, vec![0.0, 1.0], Unit::Ratio, SeriesOptions::default())
        .map_err(|source| Error::Format { source })?;
    let points = vec![Keyframe::at(0.0, 1.0), Keyframe::at(2.0, 0.0)];
    // each kind, as `from_inputs` reads it back: events in time order, as an input holds them
    assert_eq!(
        <Vec<Event>>::from_inputs(&keyed(kicks.clone().into_inputs()?))?,
        events(&[0.5, 1.0])?
    );
    assert_eq!(<Grid>::from_inputs(&keyed(grid.clone().into_inputs()?))?, grid);
    assert_eq!(<Series>::from_inputs(&keyed(series.clone().into_inputs()?))?, series);
    assert_eq!(
        <Vec<Keyframe>>::from_inputs(&keyed(points.clone().into_inputs()?))?,
        points
    );
    assert_eq!(<Vec<f64>>::from_inputs(&keyed(vec![0.25].into_inputs()?))?, vec![0.25]);
    assert_eq!(
        <Option<Grid>>::from_inputs(&keyed(Some(grid.clone()).into_inputs()?))?,
        Some(grid)
    );
    assert_eq!(None::<Grid>.into_inputs()?, vec![]);
    assert_eq!(().into_inputs()?, vec![]);
    // checked by the input's rules on the way: a value that is not finite, keyframes out of order
    assert!(matches!(vec![1.0, f64::NAN].into_inputs(), Err(Error::Input { .. })));
    let backwards = vec![Keyframe::at(2.0, 0.0), Keyframe::at(1.0, 1.0)];
    assert!(matches!(Some(backwards).into_inputs(), Err(Error::Input { .. })));
    Ok(())
}

/// `inputs`, each under its kind's name.
fn keyed(inputs: Vec<Input>) -> BTreeMap<String, Input> {
    inputs
        .into_iter()
        .map(|input| (input.kind().name().to_owned(), input))
        .collect()
}

/// A source `nest` whose body, called with `levels` above 0, calls `nest` again with one level fewer, and at 0 reads
/// `innermost` at time 0 (inside every body that called it), giving that value as a constant.
fn nesting(innermost: Signal) -> Result<Thing, Error> {
    let itself: Arc<Mutex<Option<Thing>>> = Arc::default();
    let inner = Arc::clone(&itself);
    let about = About::default();
    let declared = Declared {
        name: "nest".to_owned(),
        kind: Kind::Source,
        identity: "tests.nest".to_owned(),
        shown: None,
        table: ParamTable::new(vec![number("levels", 0.0, ParamUnit::Count, (0.0, 1000.0))])
            .map_err(|error| Error::params("nest", error))?,
        inputs: vec![],
        returns: Returns::Signal,
    };
    let nest = Thing::custom(about, declared, move |call| {
        let again = inner.lock().ok().and_then(|again| again.clone());
        match (call.values["levels"].as_f64(), again) {
            (Some(levels), Some(again)) if levels > 0.0 => again.call(&Call {
                values: json!({"levels": levels - 1.0}),
                ..Call::default()
            }),
            _ => Ok(Output::Signal(Signal::constant(innermost.at(0.0)?)?)),
        }
    })?;
    if let Ok(mut again) = itself.lock() {
        *again = Some(nest.clone());
    }
    Ok(nest)
}

#[test]
fn bodies_and_callbacks_nest_at_most_48_deep_together() {
    // (bodies one inside another, callbacks one inside another inside the innermost body, refused): a body and a
    // `from_fn` count as one level each, against one count; run on a large stack, as a debug build's levels are large
    let rows = [
        (48, 0, false),
        (49, 0, true),
        (24, 24, false),
        (25, 24, true),
        (24, 25, true),
        (0, 48, false),
        (0, 49, true),
    ];
    for (bodies, callbacks, refused) in rows {
        let read = std::thread::Builder::new()
            .stack_size(16 << 20)
            .spawn(move || -> Result<f64, Error> {
                let mut innermost = Signal::constant(0.5)?;
                for _ in 0..callbacks {
                    let input = innermost;
                    innermost = Signal::from_fn(move |times, out| input.read_into(times, out));
                }
                if bodies == 0 {
                    return innermost.at(0.0);
                }
                // the outermost body is level 1: `levels` counts the ones inside it
                let nest = nesting(innermost)?;
                signal(nest.call(&Call {
                    values: json!({"levels": f64::from(bodies - 1)}),
                    ..Call::default()
                }))?
                .at(0.0)
            })
            .map_err(|error| error.to_string())
            .and_then(|thread| thread.join().map_err(|_| "crashed".to_owned()))
            .and_then(|read| read.map_err(|error| error.to_string()));
        let expected = if refused {
            read.as_ref().is_err_and(|message| message.contains("deep"))
        } else {
            read == Ok(0.5)
        };
        assert!(expected, "{bodies} bodies, {callbacks} callbacks: {read:?}");
    }
}
