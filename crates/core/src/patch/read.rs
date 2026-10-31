//! Reading a patch: the header, each node's keys checked against its thing, and each reference resolved against the
//! nodes above and the paths into a bundle, every problem reported together.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::repr::{listed, quoted, shown};
use super::{FORMAT, Given, Node, Patch, PatchError, RESERVED, Reference, VERSION};
use crate::Error;
use crate::formats::Problem;
use crate::formats::Problems;
use crate::formats::is_name as is_bundle_name;
use crate::inputs::InputKind;
use crate::registry::Kind;
use crate::registry::NODE_KEYS;
use crate::registry::Registry;
use crate::registry::Returns;
use crate::registry::Thing;
use crate::registry::is_name;

/// What a reference can point at, by what it gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gives {
    Signal,
    Events,
    Series,
    Grid,
}

impl Gives {
    fn name(self) -> &'static str {
        match self {
            Self::Signal => "signal",
            Self::Events => "events",
            Self::Series => "series",
            Self::Grid => "grid",
        }
    }

    /// What a node that returns `returns` gives.
    fn of(returns: Returns) -> Self {
        match returns {
            Returns::Signal => Self::Signal,
            Returns::Events => Self::Events,
            Returns::Series => Self::Series,
        }
    }

    /// What the key `key` (`signal`, or an input's name) takes as a reference: for `signal` a signal or a series.
    fn taken_by(key: &str) -> &'static [Self] {
        match key {
            "signal" => &[Self::Signal, Self::Series],
            "events" => &[Self::Events],
            "series" => &[Self::Series],
            "grid" => &[Self::Grid],
            _ => &[],
        }
    }
}

/// The problems found so far, kept as the formats keep them (the first 100, the rest counted). A path already
/// reported when the text was decoded (a value a patch cannot hold, now `null`) is not reported again.
struct Report {
    problems: Problems,
    /// How many problems the errors absorbed had omitted.
    omitted: usize,
    spoiled: BTreeSet<String>,
}

impl Report {
    fn new(spoiled: Vec<Problem>) -> Self {
        let mut problems = Problems::new();
        let paths = spoiled.iter().map(|problem| problem.path.clone()).collect();
        for problem in spoiled {
            problems.push(problem);
        }
        Self {
            problems,
            omitted: 0,
            spoiled: paths,
        }
    }

    fn add(&mut self, path: String, rule: String) {
        if !self.spoiled.contains(&path) {
            self.problems.push(Problem { path, rule });
        }
    }

    /// The problems of an error about `node`, each under `<node>.<its path>`, and the count of those it omitted.
    fn absorb(&mut self, node: &str, problems: Vec<Problem>, omitted: usize) {
        for problem in problems {
            self.add(format!("{node}.{}", problem.path), problem.rule);
        }
        self.omitted += omitted;
    }

    /// The error for what was found, if anything was.
    fn into_error(self) -> Option<PatchError> {
        let (problems, omitted) = match self.problems.into_error() {
            Some(error) => {
                let omitted = error.omitted();
                (error.problems, omitted)
            }
            None => (Vec::new(), 0),
        };
        let omitted = omitted + self.omitted;
        (!problems.is_empty() || omitted > 0).then_some(PatchError::Invalid { problems, omitted })
    }
}

/// What the nodes read so far give, by name (`None` for one that is broken), and the names of all of them.
struct Nodes<'a> {
    above: BTreeMap<&'a str, Option<Returns>>,
    /// The names above, in file order (for a message).
    order: Vec<&'a str>,
    all: BTreeSet<&'a str>,
}

/// The patch `entries` hold, its things found in `registry`; `spoiled` are the problems found while decoding them.
pub(super) fn read(
    registry: &Registry,
    entries: &[(String, Value)],
    spoiled: Vec<Problem>,
) -> Result<Patch, PatchError> {
    let mut report = Report::new(spoiled);
    let header = |key: &str| entries.iter().find_map(|(name, value)| (name == key).then_some(value));
    match header("format") {
        None => report.add("format".to_owned(), format!("is required: {}", quoted(FORMAT))),
        Some(Value::String(format)) if format == FORMAT => {}
        Some(other) => report.add(
            "format".to_owned(),
            format!("must be {} (got {})", quoted(FORMAT), shown(other)),
        ),
    }
    match header("version") {
        None => report.add("version".to_owned(), format!("is required: {VERSION}")),
        Some(Value::Number(number)) if number.as_u64() == Some(u64::from(VERSION)) => {}
        Some(other) => report.add(
            "version".to_owned(),
            format!(
                "must be the whole number {VERSION}, the one version this reader knows (got {})",
                shown(other)
            ),
        ),
    }
    let tables: Vec<(&str, &Value)> = entries
        .iter()
        .filter(|(name, _)| name != "format" && name != "version")
        .map(|(name, value)| (name.as_str(), value))
        .collect();
    let mut nodes = Nodes {
        above: BTreeMap::new(),
        order: Vec::new(),
        all: tables.iter().map(|(name, _)| *name).collect(),
    };
    let mut read = Vec::new();
    for (name, table) in tables {
        if !is_name(name) || RESERVED.contains(&name) {
            let rule = format!(
                "lower case letters, digits and _, starting with a letter, and not one of {}",
                RESERVED.join(", ")
            );
            report.add(name.to_owned(), format!("is not a node name: {rule}"));
        }
        if nodes.above.contains_key(name) {
            report.add(
                name.to_owned(),
                "is the name of a node above: each node has a name of its own".to_owned(),
            );
            continue;
        }
        let node = if let Value::Object(table) = table {
            node(registry, name, table, &nodes, &mut report)
        } else {
            report.add(name.to_owned(), "must be a node: a table with `use`".to_owned());
            None
        };
        nodes.above.insert(name, node.as_ref().map(Node::returns));
        nodes.order.push(name);
        if let Some(node) = node {
            read.push((name.to_owned(), node));
        }
    }
    match report.into_error() {
        Some(error) => Err(error),
        None => Ok(Patch { nodes: read }),
    }
}

/// The node `name` written as `table`, every problem found added to `report`; `None` if its `use` names no thing.
fn node(
    registry: &Registry,
    name: &str,
    table: &Map<String, Value>,
    nodes: &Nodes<'_>,
    report: &mut Report,
) -> Option<Node> {
    let at = |key: &str| format!("{name}.{key}");
    let uses = match table.get("use") {
        None => {
            report.add(
                at("use"),
                "is required: the name of a source, shaper or operation".to_owned(),
            );
            return None;
        }
        Some(Value::String(uses)) => uses,
        Some(other) => {
            let rule = format!(
                "must be text, the name of a source, shaper or operation (got {})",
                shown(other)
            );
            report.add(at("use"), rule);
            return None;
        }
    };
    let thing = match registry.get(uses) {
        Ok(thing) => thing,
        Err(error) => {
            report.add(at("use"), error.to_string());
            return None;
        }
    };
    let label = format!("{} {}", thing.kind(), quoted(uses));

    let mut signal = None;
    match (thing.kind(), table.get("signal")) {
        (Kind::Shaper, None) => {
            report.add(at("signal"), "is required: a shaper takes a signal".to_owned());
        }
        (Kind::Shaper, Some(Value::String(text))) => {
            signal = reference(text, "signal", nodes)
                .map_err(|rule| report.add(at("signal"), rule))
                .ok();
        }
        (Kind::Shaper, Some(_)) => {
            report.add(
                at("signal"),
                "must be a reference: a node above, or series.<name>".to_owned(),
            );
        }
        (_, Some(_)) => report.add(at("signal"), format!("{label} takes no signal")),
        (_, None) => {}
    }

    let (inputs, values) = keys(thing, name, table, nodes, report);
    Some(Node {
        thing: thing.clone(),
        signal,
        inputs,
        values,
    })
}

/// The inputs (in the order `thing` declares them) and the parameter values of the node `name` of `thing` written as
/// `table`, every problem found with its other keys added to `report`.
fn keys(
    thing: &Thing,
    name: &str,
    table: &Map<String, Value>,
    nodes: &Nodes<'_>,
    report: &mut Report,
) -> (Vec<(InputKind, Given)>, Map<String, Value>) {
    let at = |key: &str| format!("{name}.{key}");
    let label = format!("{} {}", thing.kind(), quoted(thing.name()));
    let mut given: Vec<(InputKind, Given)> = Vec::new();
    let mut values = Map::new();
    let has_param = |key: &str| thing.table().params().iter().any(|param| param.name == key);
    for (key, value) in table {
        if NODE_KEYS.contains(&key.as_str()) {
            continue;
        }
        if let Some(&(kind, _)) = thing.inputs().iter().find(|(kind, _)| kind.name() == key) {
            match value {
                Value::String(text) if !Gives::taken_by(key).is_empty() => {
                    if let Ok(found) = reference(text, key, nodes).map_err(|rule| report.add(at(key), rule)) {
                        given.push((kind, Given::Reference(found)));
                    }
                }
                _ => match kind.check(value) {
                    Ok(input) => {
                        given.push((kind, Given::Data(input)));
                    }
                    Err(Error::Input { problems, omitted }) => report.absorb(name, problems, omitted),
                    Err(other) => report.add(at(key), other.to_string()),
                },
            }
        } else if has_param(key) {
            values.insert(key.clone(), value.clone());
        } else {
            let names: Vec<&str> = thing
                .inputs()
                .iter()
                .map(|(kind, _)| kind.name())
                .chain(thing.table().listed().map(|param| param.name.as_str()))
                .collect();
            let takes = if names.is_empty() {
                "nothing".to_owned()
            } else {
                names.join(", ")
            };
            report.add(
                at(key),
                format!("is neither an input nor a parameter of {label} (it takes {takes})"),
            );
        }
    }
    for (kind, required) in thing.inputs() {
        if *required && !table.contains_key(kind.name()) {
            report.add(
                at(kind.name()),
                format!("is required: {} takes {kind}", quoted(thing.name())),
            );
        }
    }
    if let Err(error) = thing.table().parse(&Value::Object(values.clone())) {
        let omitted = error.omitted();
        report.absorb(name, error.problems, omitted);
    }
    // in the order the thing declares them
    given.sort_by_key(|(kind, _)| thing.inputs().iter().position(|(declared, _)| declared == kind));
    (given, values)
}

/// What `text`, written where `key` wants data, refers to, or the rule it breaks.
fn reference(text: &str, key: &str, nodes: &Nodes<'_>) -> Result<Reference, String> {
    let (root, name) = text.split_once('.').unwrap_or((text, ""));
    let (gives, found) = if root == "events" || root == "series" {
        if !is_bundle_name(name) {
            return Err(format!(
                "{} is not a path into a bundle: {root}.<name>, the name in lower case",
                quoted(text)
            ));
        }
        if root == "events" {
            (Some(Gives::Events), Reference::Events(name.to_owned()))
        } else {
            (Some(Gives::Series), Reference::Series(name.to_owned()))
        }
    } else if text == "grid" {
        (Some(Gives::Grid), Reference::Grid)
    } else if let Some(returns) = nodes.above.get(text) {
        (returns.map(Gives::of), Reference::Node(text.to_owned()))
    } else if nodes.all.contains(text) {
        return Err(format!(
            "{} is a node below this one; a node uses only the nodes above it",
            quoted(text)
        ));
    } else {
        let known = listed(nodes.order.iter().copied());
        return Err(format!(
            "{} is no node above this one (nodes above: {known}) and no bundle path (events.<name>, series.<name>, grid)",
            quoted(text)
        ));
    };
    let takes = Gives::taken_by(key);
    match gives {
        // what a broken node would give is unknown: no reference to it is judged
        Some(gives) if !takes.contains(&gives) => {
            let takes: Vec<&str> = takes.iter().map(|gives| gives.name()).collect();
            Err(format!(
                "{} gives {}, but {key} takes {}",
                quoted(text),
                gives.name(),
                takes.join(" or ")
            ))
        }
        _ => Ok(found),
    }
}
