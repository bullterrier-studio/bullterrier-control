//! The doors that only the crate itself can reach: `sourced`, the typed door of a `lib` function, for a source of these
//! tests (the built-ins' are reached through `lib::`, in `tests/it/registry.rs`). Each expected number is worked out
//! by hand.

use serde_json::json;

use super::Kind;
use super::{About, Call, Output, Registry, Thing};
use crate::formats::{Event, EventOptions, Grid};
use crate::inputs::{Input, InputKind};
use crate::{Error, Signal};

/// Events at `times`.
fn events(times: &[f64]) -> Result<Vec<Event>, Error> {
    times
        .iter()
        .map(|&t| Event::new(t, EventOptions::default()).map_err(|source| Error::Format { source }))
        .collect()
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

/// A source of these tests that takes data: at a time, `step` for each event at or before it.
#[derive(Clone, Debug, PartialEq, crate::Params)]
struct Hits {
    /// What each event adds.
    #[param(default = 1.0, unit = Ratio, min = 0, max = 10)]
    step: f64,
}

impl crate::Source for Hits {
    const NAME: &'static str = "hits";
    type Inputs = Vec<Event>;

    fn source(&self, events: Vec<Event>) -> Result<Signal, Error> {
        Ok(counted(&events, self.step))
    }
}

/// A shaper of these tests that takes data if it is given: its input, raised by `step` at each event.
#[derive(Clone, Debug, PartialEq, crate::Params)]
struct Bump {
    /// What each event adds.
    #[param(default = 1.0, unit = Ratio, min = 0, max = 10)]
    step: f64,
}

impl crate::Shaper for Bump {
    const NAME: &'static str = "bump";
    type Inputs = Option<Vec<Event>>;

    fn shape(&self, input: &Signal, events: Option<Vec<Event>>) -> Result<Signal, Error> {
        Ok(input + &counted(&events.unwrap_or_default(), self.step))
    }
}

/// The signal that is, at a time, `step` for each of `events` at or before it.
fn counted(events: &[Event], step: f64) -> Signal {
    let times: Vec<f64> = events.iter().map(Event::t).collect();
    Signal::from_fn(move |at, out| {
        for (time, value) in at.iter().zip(out.iter_mut()) {
            *value = times.iter().filter(|event| **event <= *time).map(|_| step).sum();
        }
        Ok::<(), Error>(())
    })
}

/// The times [`Hits`] and [`Bump`] are read at.
const HIT_AT: [f64; 4] = [0.0, 0.5, 1.5, 2.0];

/// A row of [`a_rust_source_and_shaper_take_their_inputs_by_their_kinds`]: (row, thing, inputs by name, its values at
/// [`HIT_AT`] or the call's refusal).
type HitRow<'a> = (&'a str, &'a Thing, Vec<(&'a str, &'a Input)>, Result<Vec<f64>, String>);

#[test]
fn a_rust_source_and_shaper_take_their_inputs_by_their_kinds() -> Result<(), Error> {
    let hits = Thing::source::<Hits>(About::default())?;
    let bump = Thing::shaper::<Bump>(About::default())?;
    assert_eq!(hits.inputs(), [(InputKind::Events, true)]);
    assert_eq!(bump.inputs(), [(InputKind::Events, false)]);
    assert!(
        hits.help().starts_with("source hits(*, events, **values) -> signal\n"),
        "{}",
        hits.help()
    );
    assert!(
        bump.help()
            .starts_with("shaper bump(signal, *, events=None, **values) -> signal\n"),
        "{}",
        bump.help()
    );
    let kicks = Input::from_events(events(&[1.0, 0.5, 2.0])?);
    let grid = Input::Grid(Grid::new(vec![0.0, 0.5], 4, 0).map_err(|source| Error::Format { source })?);
    let level = Signal::constant(0.25)?;
    let call = |thing: &Thing, inputs: &[(&str, &Input)]| -> Result<Vec<f64>, String> {
        let signals = if thing.kind() == Kind::Shaper {
            vec![level.clone()]
        } else {
            vec![]
        };
        let given = Call {
            signals,
            inputs: inputs
                .iter()
                .map(|(name, input)| ((*name).to_owned(), (*input).clone()))
                .collect(),
            values: json!({"step": 0.5}),
        };
        match thing.call(&given) {
            Ok(Output::Signal(signal)) => signal.sample(&HIT_AT).map_err(|error| error.to_string()),
            Ok(other) => Err(format!("not a signal: {other:?}")),
            Err(error @ Error::Call { .. }) => Err(error.to_string()),
            Err(other) => Err(format!("not a call error: {other}")),
        }
    };
    // 0.5 for each of the events at 0.5, 1 and 2 s at or before the time; the shaper adds it to its input, 0.25
    let rows: [HitRow<'_>; 7] = [
        (
            "a source given its events",
            &hits,
            vec![("events", &kicks)],
            Ok(vec![0.0, 0.5, 1.0, 1.5]),
        ),
        (
            "a source without the events it needs",
            &hits,
            vec![],
            Err("source 'hits' needs the input 'events'".to_owned()),
        ),
        (
            "a source given another kind under the name events",
            &hits,
            vec![("events", &grid)],
            Err("source 'hits': the input 'events' holds grid".to_owned()),
        ),
        (
            "a source given an input it does not take",
            &hits,
            vec![("events", &kicks), ("grid", &grid)],
            Err("source 'hits' takes no input 'grid'".to_owned()),
        ),
        (
            "a shaper given its events",
            &bump,
            vec![("events", &kicks)],
            Ok(vec![0.25, 0.75, 1.25, 1.75]),
        ),
        ("a shaper without its optional events", &bump, vec![], Ok(vec![0.25; 4])),
        (
            "a shaper given another kind under the name events",
            &bump,
            vec![("events", &grid)],
            Err("shaper 'bump': the input 'events' holds grid".to_owned()),
        ),
    ];
    for (row, thing, inputs, expected) in rows {
        assert_eq!(call(thing, &inputs), expected, "{row}");
    }
    Ok(())
}

/// A row of [`a_signal_with_inputs_made_by_any_door_saves_as_a_patch_and_builds_again`]: (thing, door, the signal it
/// made, the inputs its recipe holds, its values at [`HIT_AT`]).
type InputDoorRow = (&'static str, &'static str, Signal, Vec<(String, Input)>, Vec<f64>);

#[test]
fn a_signal_with_inputs_made_by_any_door_saves_as_a_patch_and_builds_again() -> Result<(), Box<dyn std::error::Error>> {
    use crate::Patch;

    let mut registry = Registry::builtin().clone();
    registry.register(Thing::source::<Hits>(About::default())?)?;
    registry.register(Thing::shaper::<Bump>(About::default())?)?;
    let kicks = events(&[1.0, 0.5, 2.0])?;
    let given = vec![("events".to_owned(), Input::from_events(kicks.clone()))];
    let by_name = |name: &str, signals: Vec<Signal>, inputs: &[(String, Input)]| {
        signal(registry.get(name)?.call(&Call {
            signals,
            inputs: inputs.iter().cloned().collect(),
            values: json!({"step": 0.5}),
        }))
    };
    let typed = super::sourced(Hits { step: 0.5 }, kicks.clone())?;
    let one = events(&[1.0])?;
    let rows: Vec<InputDoorRow> = vec![
        (
            "hits",
            "Thing::call",
            by_name("hits", vec![], &given)?,
            given.clone(),
            vec![0.0, 0.5, 1.0, 1.5],
        ),
        ("hits", "lib", typed.clone(), given.clone(), vec![0.0, 0.5, 1.0, 1.5]),
        // 0.5 an event below, 2 at the one event at 1 s on top
        (
            "bump",
            "Thing::call",
            by_name(
                "bump",
                vec![typed.clone()],
                &[("events".to_owned(), Input::from_events(one.clone()))],
            )?,
            vec![("events".to_owned(), Input::from_events(one.clone()))],
            vec![0.0, 0.5, 1.5, 2.0],
        ),
        (
            "bump",
            "pipe_with",
            typed.pipe_with(Bump { step: 0.5 }, Some(one.clone()))?,
            vec![("events".to_owned(), Input::from_events(one))],
            vec![0.0, 0.5, 1.5, 2.0],
        ),
        (
            "bump",
            "pipe_with none",
            typed.pipe_with(Bump { step: 0.5 }, None)?,
            vec![],
            vec![0.0, 0.5, 1.0, 1.5],
        ),
    ];
    for (thing, door, made, inputs, values) in rows {
        let recipe = made.recipe().ok_or("no recipe")?;
        assert_eq!(
            (recipe.name.as_str(), &recipe.inputs),
            (thing, &inputs),
            "{thing} through {door}"
        );
        assert_eq!(made.sample(&HIT_AT)?, values, "{thing} through {door}");
        // saved with its inputs inline, it builds the same values again
        let text = Patch::from_signals_with(&registry, &[("made", &made)])?.to_toml();
        let built = Patch::from_toml_with(&registry, &text)?.build(None)?;
        let Some(Output::Signal(again)) = built.get("made") else {
            return Err(format!("{thing} through {door}: not built: {text}").into());
        };
        assert_eq!(again.sample(&HIT_AT)?, values, "{thing} through {door}: {text}");
    }
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
    type Inputs = ();

    fn shape(&self, input: &Signal, (): ()) -> Result<Signal, Error> {
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
    type Inputs = ();

    fn source(&self, (): ()) -> Result<Signal, Error> {
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
        ("level", "lib", super::sourced(Level::default(), ())),
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
        ("level", "lib", super::sourced(Level { value: 2.0 }, ())),
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
