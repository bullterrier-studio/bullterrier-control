//! Building a patch: each node wanted, in file order, called through its thing with its references resolved against
//! the nodes built above it and the bundle.

use std::collections::{BTreeMap, BTreeSet};

use super::repr::quoted;
use super::{Built, Given, Node, Patch, PatchError, Reference};
use crate::Error;
use crate::Interpolation;
use crate::Signal;
use crate::formats::Bundle;
use crate::formats::Event;
use crate::formats::Grid;
use crate::formats::Problem;
use crate::formats::Series;
use crate::inputs::Input;
use crate::inputs::InputKind;
use crate::registry::Call;
use crate::registry::Output;

/// `patch` built against `bundle`: every node, or only those called `only` and what they use.
pub(super) fn build(patch: &Patch, bundle: Option<&Bundle>, only: Option<&[&str]>) -> Result<Built, PatchError> {
    let wanted = match only {
        Some(names) => Some(needed(patch, names)?),
        None => None,
    };
    let mut made: Vec<(String, Output)> = Vec::new();
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    for (name, node) in &patch.nodes {
        if wanted.as_ref().is_some_and(|wanted| !wanted.contains(name.as_str())) {
            continue;
        }
        let output = made_by(name, node, bundle, |above| index.get(above).map(|&at| &made[at].1))?;
        index.insert(name, made.len());
        made.push((name.clone(), output));
    }
    if let Some(names) = only {
        made.retain(|(name, _)| names.contains(&name.as_str()));
    }
    Ok(Built { made })
}

/// The names of the nodes `names` and every node they use. A node uses only nodes above it, so one pass upward finds
/// all.
fn needed<'a>(patch: &'a Patch, names: &[&str]) -> Result<BTreeSet<&'a str>, PatchError> {
    let mut needed = BTreeSet::new();
    for name in names {
        let Some((known, _)) = patch.nodes.iter().find(|(known, _)| known == name) else {
            return Err(PatchError::UnknownNode {
                name: (*name).to_owned(),
                nodes: patch.nodes.iter().map(|(known, _)| known.clone()).collect(),
            });
        };
        needed.insert(known.as_str());
    }
    for (name, node) in patch.nodes.iter().rev() {
        if needed.contains(name.as_str()) {
            let references = node
                .signal
                .iter()
                .chain(node.inputs.iter().filter_map(|(_, given)| match given {
                    Given::Reference(reference) => Some(reference),
                    Given::Data(_) => None,
                }));
            for reference in references {
                if let Reference::Node(used) = reference
                    && let Some((used, _)) = patch.nodes.iter().find(|(known, _)| known == used)
                {
                    needed.insert(used.as_str());
                }
            }
        }
    }
    Ok(needed)
}

/// What the node `name` gives, built against `bundle`, `above` giving what a node above it gave.
fn made_by<'a>(
    name: &str,
    node: &Node,
    bundle: Option<&'a Bundle>,
    above: impl Fn(&str) -> Option<&'a Output>,
) -> Result<Output, PatchError> {
    let found = |key: &str, reference: &Reference| find(name, key, reference, bundle, &above);
    let mut call = Call::default();
    if let Some(reference) = &node.signal {
        call.signals.push(signal(name, reference, found("signal", reference)?)?);
    }
    for (kind, given) in &node.inputs {
        let input = match given {
            Given::Data(input) => input.clone(),
            Given::Reference(reference) => input(name, *kind, reference, found(kind.name(), reference)?)?,
        };
        call.inputs.insert(kind.name().to_owned(), input);
    }
    call.values = serde_json::Value::Object(node.values.clone());
    node.thing.call(&call).map_err(|source| PatchError::Node {
        node: name.to_owned(),
        source,
    })
}

/// What a reference finds: a node's result or an entry of the bundle.
#[derive(Clone, Copy)]
enum Found<'a> {
    Output(&'a Output),
    Events(&'a [Event]),
    Series(&'a Series),
    Grid(&'a Grid),
}

/// What `reference`, the `key` of the node `name`, finds: a node built above, or an entry of `bundle`.
fn find<'a>(
    name: &str,
    key: &str,
    reference: &Reference,
    bundle: Option<&'a Bundle>,
    above: impl Fn(&str) -> Option<&'a Output>,
) -> Result<Found<'a>, PatchError> {
    let refused = |rule: String| PatchError::Bundle {
        problem: Problem {
            path: format!("{name}.{key}"),
            rule,
        },
    };
    let with_bundle = || {
        bundle.ok_or_else(|| {
            refused(format!(
                "{} is a path into a bundle, and none was given",
                quoted(&reference.to_string())
            ))
        })
    };
    let lacks = |section: &str, entry: &str, held: Vec<&str>| {
        let has = if held.is_empty() {
            "none".to_owned()
        } else {
            held.join(", ")
        };
        refused(format!("the bundle has no {section} {} (it has: {has})", quoted(entry)))
    };
    match reference {
        // a node uses only nodes above it, each built before it
        Reference::Node(used) => above(used)
            .map(Found::Output)
            .ok_or_else(|| refused(format!("{} is a node that was not built", quoted(used)))),
        Reference::Grid => with_bundle()?
            .grid()
            .map(Found::Grid)
            .ok_or_else(|| refused("the bundle has no grid".to_owned())),
        Reference::Events(entry) => {
            let held = with_bundle()?.events();
            held.get(entry)
                .map(|events| Found::Events(events))
                .ok_or_else(|| lacks("events", entry, held.keys().map(String::as_str).collect()))
        }
        Reference::Series(entry) => {
            let held = with_bundle()?.series();
            held.get(entry)
                .map(Found::Series)
                .ok_or_else(|| lacks("series", entry, held.keys().map(String::as_str).collect()))
        }
    }
}

/// The signal `found` gives where the node `name` wants its `signal`: a signal, or a series read linearly.
fn signal(name: &str, reference: &Reference, found: Found<'_>) -> Result<Signal, PatchError> {
    match found {
        Found::Output(Output::Signal(signal)) => Ok(signal.clone()),
        Found::Output(Output::Series(series)) | Found::Series(series) => {
            Ok(Signal::from_series(series, Interpolation::Linear))
        }
        _ => Err(misfit(name, "signal", reference)),
    }
}

/// The input of `kind` that `found` gives where the node `name` wants it.
fn input(name: &str, kind: InputKind, reference: &Reference, found: Found<'_>) -> Result<Input, PatchError> {
    match (kind, found) {
        (InputKind::Events, Found::Output(Output::Events(events))) => Ok(Input::from_events(events.clone())),
        (InputKind::Events, Found::Events(events)) => Ok(Input::from_events(events.to_vec())),
        (InputKind::Series, Found::Output(Output::Series(series)) | Found::Series(series)) => {
            Ok(Input::Series(series.clone()))
        }
        (InputKind::Grid, Found::Grid(grid)) => Ok(Input::Grid(grid.clone())),
        _ => Err(misfit(name, kind.name(), reference)),
    }
}

/// The error for a reference that gives what its key does not take, which reading refuses: a node it was read with
/// gave another kind of result than it declares.
fn misfit(name: &str, key: &str, reference: &Reference) -> PatchError {
    PatchError::Node {
        node: name.to_owned(),
        source: Error::Call {
            message: format!("{} gave what {key} does not take", quoted(&reference.to_string())),
        },
    }
}
