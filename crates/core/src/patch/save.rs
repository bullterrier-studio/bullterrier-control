//! Saving a graph as a patch: each named signal and every signal it is made from, as nodes in an order a patch reads.
//!
//! The graph a recipe describes is a chain: a thing takes at most one signal (a shaper's), its other inputs are data.
//! Two named signals may share the chain above them, which is then one set of nodes, named once.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Map;

use super::repr::quoted;
use super::{Given, Node, Patch, PatchError, Reference, is_node_name, not_a_node_name};
use crate::Origin;
use crate::Signal;
use crate::formats::Problem;
use crate::formats::Problems;
use crate::registry::Recipe;
use crate::registry::Registry;
use crate::registry::Thing;

#[cfg(test)]
mod tests;

/// Where a signal is: its recipe's place in memory, which every handle of the same signal shares (a recipe lives in
/// the signal's node, and the signals held by the caller keep every node alive while saving).
type Place = *const Recipe;

/// A signal to write as a node: its recipe, the name of the node it is written under or the given name nearest below
/// it (its owner), and how many steps up from that given signal it is (0 for the given signal itself).
struct Step<'a> {
    recipe: &'a Recipe,
    owner: &'a str,
    up: usize,
}

/// The patch of `signals` for `registry`: every signal they are made from, each as one node.
pub(super) fn save(registry: &Registry, signals: &[(&str, &Signal)]) -> Result<Patch, PatchError> {
    let mut problems = Problems::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (name, _) in signals {
        if !is_node_name(name) {
            problems.push(at(name, 0, not_a_node_name()));
        } else if !taken.insert((*name).to_owned()) {
            problems.push(at(
                name,
                0,
                "is given twice: each node has a name of its own".to_owned(),
            ));
        }
    }
    // the given name of each signal given (the first, for a signal given under two)
    let mut given: BTreeMap<Place, &str> = BTreeMap::new();
    for (name, signal) in signals {
        if let Some(recipe) = signal.recipe() {
            given.entry(std::ptr::from_ref(recipe)).or_insert(name);
        }
    }
    let mut saver = Saver {
        registry,
        problems,
        taken,
        numbered: BTreeMap::new(),
        written: BTreeMap::new(),
        nodes: Vec::new(),
    };
    for (name, signal) in signals {
        saver.save(name, signal, &given);
    }
    let Saver { problems, nodes, .. } = saver;
    match problems.into_error() {
        Some(error) => {
            let omitted = error.omitted();
            Err(PatchError::Unsaved {
                problems: error.problems,
                omitted,
            })
        }
        None => Ok(Patch { nodes }),
    }
}

/// The patch being written, and what it knows so far.
struct Saver<'a> {
    registry: &'a Registry,
    problems: Problems,
    /// Every node name given or made so far.
    taken: BTreeSet<String>,
    /// The last number of a name made after each given name (`glow_2`).
    numbered: BTreeMap<String, usize>,
    /// The node each signal written so far is, by its place: an index into `nodes`.
    written: BTreeMap<Place, usize>,
    nodes: Vec<(String, Node)>,
}

impl<'a> Saver<'a> {
    /// Writes the signal given as `name` and every signal above it not written yet, the highest first.
    fn save(&mut self, name: &'a str, signal: &'a Signal, given: &BTreeMap<Place, &'a str>) {
        let recipe = match recipe_of(signal) {
            Ok(recipe) => recipe,
            Err(rule) => return self.problems.push(at(name, 0, rule)),
        };
        if let Some(&index) = self.written.get(&std::ptr::from_ref(recipe)) {
            // written already: under this name (it is above a signal given before), or under another (given twice)
            let (written, node) = &self.nodes[index];
            if written != name {
                let copy = node.clone();
                self.nodes.push((name.to_owned(), copy));
            }
            return;
        }
        let mut steps = vec![Step {
            recipe,
            owner: name,
            up: 0,
        }];
        let (mut owner, mut up) = (name, 0);
        let mut above = recipe.signal.as_ref();
        while let Some(signal) = above {
            up += 1;
            let recipe = match recipe_of(signal) {
                Ok(recipe) => recipe,
                Err(rule) => {
                    self.problems.push(at(owner, up, rule));
                    break;
                }
            };
            let place = std::ptr::from_ref(recipe);
            if self.written.contains_key(&place) {
                break;
            }
            // a given signal above owns what is above it, and the paths there start at it
            if let Some(&given) = given.get(&place) {
                (owner, up) = (given, 0);
            }
            steps.push(Step { recipe, owner, up });
            above = recipe.signal.as_ref();
        }
        for step in steps.into_iter().rev() {
            self.write(&step);
        }
    }

    /// Writes the node of `step`, under its owner's name if it is the owner, else under a name made from it.
    fn write(&mut self, step: &Step<'_>) {
        let Step { recipe, owner, up } = *step;
        let name = if up == 0 {
            owner.to_owned()
        } else {
            self.made_name(owner)
        };
        let thing = match self.registry.get(&recipe.name) {
            Ok(thing) if recipe.made_by(thing) => thing,
            Ok(_) => {
                let rule = format!(
                    "is made by a thing called {} other than the one the registry it is saved for holds",
                    quoted(&recipe.name)
                );
                self.problems.push(at(owner, up, rule));
                return;
            }
            Err(_) => {
                let rule = format!(
                    "is made by {}, which the registry it is saved for does not hold",
                    quoted(&recipe.name)
                );
                self.problems.push(at(owner, up, rule));
                return;
            }
        };
        let signal = recipe.signal.as_ref().and_then(|above| {
            let place = std::ptr::from_ref(above.recipe()?);
            let index = *self.written.get(&place)?;
            Some(Reference::Node(self.nodes[index].0.clone()))
        });
        let inputs = recipe
            .inputs
            .iter()
            .map(|(_, input)| (input.kind(), Given::Data(input.clone())))
            .collect();
        let values: Map<String, serde_json::Value> = thing
            .table()
            .params()
            .iter()
            .filter_map(|param| Some((param.name.clone(), recipe.values.get(&param.name)?.clone())))
            .collect();
        let node = Node {
            thing: Thing::clone(thing),
            signal,
            inputs,
            values,
        };
        self.written.insert(std::ptr::from_ref(recipe), self.nodes.len());
        self.nodes.push((name, node));
    }

    /// The next name `<owner>_<n>` that no node has and no signal is given under.
    fn made_name(&mut self, owner: &str) -> String {
        let number = self.numbered.entry(owner.to_owned()).or_insert(0);
        loop {
            *number += 1;
            let name = format!("{owner}_{number}");
            if self.taken.insert(name.clone()) {
                return name;
            }
        }
    }
}

/// The problem at the signal `up` steps above the one given as `name`: `name.signal.signal` for two.
fn at(name: &str, up: usize, rule: String) -> Problem {
    Problem {
        path: format!("{name}{}", ".signal".repeat(up)),
        rule,
    }
}

/// The recipe of `signal`, or why a patch cannot hold it: it was not made by a thing.
fn recipe_of(signal: &Signal) -> Result<&Recipe, String> {
    let why = match signal.origin() {
        Origin::Thing(recipe) => return Ok(recipe),
        Origin::Operator => "is made by an operator (+, -, *, negation, minimum or maximum)",
        Origin::Values => "holds values given as they are (a constant, from_values or from_series)",
        Origin::Callback => "is made by a callback (from_fn, or a Python compute)",
        Origin::Rendered => "is rendered by a callback (rendered, or a Python Rendered(run=...))",
    };
    Err(format!(
        "{why}, which a patch has no node for: make it with a source, shaper or operation"
    ))
}
