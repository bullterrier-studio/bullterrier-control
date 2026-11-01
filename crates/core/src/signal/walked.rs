//! An architecture test (it reads source text, which a behaviour test never does): only the walk (`walk.rs`) reads a
//! node's inputs. A node that read one itself, in [`Node::start`](super::node::Node::start) or
//! [`Node::finish`](super::node::Node::finish), would bring back a read that recurses as deep as the graph, and no
//! behaviour test on a small stack would notice until a graph deep enough came along. So the code of every node
//! module (every file under `src/signal/` but the walk and the tests) names none of the ways a signal is read, and
//! every node is in such a module. A callback (user code) reads signals inside a read; its nesting is capped by
//! [`NESTED`](super::walk::NESTED) instead.

use std::fs;
use std::path::{Path, PathBuf};

use crate::scan::{code, rust_files};

/// The ways a signal is read: methods of `Signal` (called, or named as a path) and the walk itself.
const READS: &[&str] = &["read_into", "sample", "sample_into", "at", "walk::read"];

/// The reads the code of `text` names, each as listed in [`READS`], in order of appearance by kind.
fn reads(text: &str) -> Vec<&'static str> {
    let code = code(text);
    let mut found = Vec::new();
    for read in READS {
        for (start, _) in code.match_indices(read) {
            let before = &code[..start];
            let after = code[start + read.len()..].chars().next().unwrap_or(' ');
            let called = if read.contains("::") {
                !before.ends_with(|character: char| character.is_alphanumeric() || character == '_')
            } else {
                before.ends_with('.') || before.ends_with("::")
            };
            if called && !(after.is_alphanumeric() || after == '_') {
                found.push(*read);
            }
        }
    }
    found
}

/// How many types the code of `text` implements `Node` for (`impl Node for`, by any path, generic or not).
fn implements_node(text: &str) -> usize {
    let code = code(text);
    let name_part = |character: char| character.is_alphanumeric() || character == '_';
    code.match_indices("impl")
        .filter(|(start, _)| {
            let end = start + "impl".len();
            if code[..*start].ends_with(name_part) || code[end..].starts_with(name_part) {
                return false; // a part of a longer name
            }
            let mut rest = code[end..].trim_start();
            if rest.starts_with('<') {
                // the generics, to their closing `>` (a `->` inside them is not one)
                let (mut depth, mut previous) = (0, ' ');
                let close = rest.char_indices().find(|&(_, character)| {
                    match (previous, character) {
                        (_, '<') => depth += 1,
                        ('-', '>') => {}
                        (_, '>') => depth -= 1,
                        _ => {}
                    }
                    previous = character;
                    depth == 0
                });
                rest = close.map_or("", |(index, _)| rest[index + 1..].trim_start());
            }
            let path_end = rest
                .find(|character: char| !(name_part(character) || character == ':'))
                .unwrap_or(rest.len());
            let (path, after) = rest.split_at(path_end);
            path.rsplit("::").next() == Some("Node") && after.trim_start().starts_with("for ")
        })
        .count()
}

/// The node modules: every file under `src/signal/` but the walk, the tests and this scan.
fn node_modules() -> std::io::Result<Vec<PathBuf>> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("signal");
    Ok(rust_files(&folder)?
        .into_iter()
        .filter(|path| {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
            !["walk.rs", "walked.rs", "tests.rs"].contains(&name)
        })
        .collect())
}

#[test]
fn the_scan_sees_every_read_of_a_signal_in_code_and_only_there() {
    // (row, text, the reads seen)
    let rows: [(&str, &str, &[&str]); 10] = [
        (
            "a node naming its input for the walk",
            "fn start<'g>(&'g self, _: &[f64], _: &mut [f64]) -> Result<Next<'g>, Box<Error>> { Ok(Next::As(&self.inner)) }",
            &[],
        ),
        (
            "a name that starts like a read",
            "let sample = self.sample_at(time); let at = sample; self.started(); attach(); walk::reader()",
            &[],
        ),
        (
            "in comments and strings",
            "// self.input.read_into(times, out)\nlet a = \".sample(\";",
            &[],
        ),
        (
            "a read into a buffer",
            "self.input.read_into(&times, &mut inputs)?;",
            &["read_into"],
        ),
        ("a checked read", "let values = first.sample(times)?;", &["sample"]),
        (
            "a checked read into a buffer",
            "second.sample_into(times, out)?",
            &["sample_into"],
        ),
        ("one time", "out[0] = input.at(0.0)?;", &["at"]),
        (
            "a read named as a path",
            "times.iter().map(|time| Signal::at(&input, *time))",
            &["at"],
        ),
        (
            "the walk itself",
            "super::walk::read(&self.input, times, out)?",
            &["walk::read"],
        ),
        (
            "a read split over lines",
            "self.inner\n    .read_into(times, out)",
            &["read_into"],
        ),
    ];
    for (row, text, expected) in rows {
        assert_eq!(reads(text), expected, "{row}");
    }
}

#[test]
fn the_scan_sees_every_way_of_implementing_node() {
    // (row, text, implementations seen)
    let rows = [
        ("by its name", "impl Node for Combined {", 1),
        ("by a path", "impl super::node::Node for Constant {}", 1),
        ("generic", "impl<T: Fn() -> Vec<f64> + Send> Node for Wrapped<T> {", 1),
        (
            "another trait",
            "impl Drop for Signal {} impl fmt::Debug for Node {}",
            0,
        ),
        (
            "a name ending like it",
            "impl SeriesNode for X {} impl Nodes for Y {}",
            0,
        ),
        ("in a comment", "// impl Node for Combined {", 0),
    ];
    for (row, text, expected) in rows {
        assert_eq!(implements_node(text), expected, "{row}");
    }
}

#[test]
fn no_node_reads_its_inputs_and_every_node_is_one_the_scan_reads() -> std::io::Result<()> {
    let modules = node_modules()?;
    let mut nodes = 0;
    for module in &modules {
        let text = fs::read_to_string(module)?;
        nodes += implements_node(&text);
        assert_eq!(reads(&text), Vec::<&str>::new(), "{}", module.display());
    }
    // every node of the crate is in a node module: none elsewhere implements the trait
    let mut everywhere = 0;
    for file in rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))? {
        everywhere += implements_node(&fs::read_to_string(file)?);
    }
    // the scan saw the nodes: constant, callback, refused, combined, recorded, rendered, series
    assert!(
        modules.len() >= 5 && nodes >= 7,
        "{} modules, {nodes} nodes",
        modules.len()
    );
    assert_eq!(everywhere, nodes, "a node outside the node modules");
    Ok(())
}
