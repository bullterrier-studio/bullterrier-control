//! Architecture tests: they read the crate's own source as code (which a behaviour test never does).
//! The patch uses only what the modules below it make public (`src/patch.rs`, `src/patch/`), and only the walk
//! (`src/signal/walk.rs`) reads a node's inputs. Each scan has rows that prove it sees what it must and only that.
//!
//! A node that read an input itself, in `Node::start` or `Node::finish`, would bring back a read that recurses as deep
//! as the graph, and no behaviour test on a small stack would notice until a graph deep enough came along. So the code
//! of every node module (every file under `src/signal/` but the walk) names none of the ways a signal is read, and
//! every node is in such a module. A callback (user code) reads signals inside a read; its nesting is capped by
//! `NESTED` (`walk.rs`) instead.

use std::fs;
use std::path::{Path, PathBuf};

use crate::common::scan::{code, rust_files};

/// The public items the patch may name, each with its module's path (a path below one, such as an enum's variant,
/// is allowed with it).
const ALLOWED: &[&str] = &[
    "crate::Error",
    "crate::FromValues",
    "crate::Interpolation",
    "crate::Origin",
    "crate::Rendering",
    "crate::Signal",
    "crate::builtin::constant",
    "crate::builtin::follow",
    "crate::builtin::slew",
    "crate::easing::Ease",
    "crate::formats::Bundle",
    "crate::formats::Event",
    "crate::formats::EventOptions",
    "crate::formats::FormatError",
    "crate::formats::Grid",
    "crate::formats::Problem",
    "crate::formats::Problems",
    "crate::formats::Series",
    "crate::formats::SeriesOptions",
    "crate::formats::Unit",
    "crate::formats::is_name",
    "crate::inputs::Input",
    "crate::inputs::InputKind",
    "crate::inputs::Keyframe",
    "crate::lib",
    "crate::maximum",
    "crate::minimum",
    "crate::params::Kind",
    "crate::params::Param",
    "crate::params::ParamTable",
    "crate::params::ParamUnit",
    "crate::registry::About",
    "crate::registry::Call",
    "crate::registry::Declared",
    "crate::registry::Kind",
    "crate::registry::NAME_PATTERN",
    "crate::registry::NODE_KEYS",
    "crate::registry::Output",
    "crate::registry::Recipe",
    "crate::registry::Registry",
    "crate::registry::Returns",
    "crate::registry::Thing",
    "crate::registry::is_name",
];

/// The paths into the crate that the code of `text` names, each as written (the crate by its own name read as
/// `crate`); a grouped import as its path and `::{`; a path out of the patch from `super::super`.
fn named(text: &str) -> Vec<String> {
    let code = code(text);
    let mut paths = Vec::new();
    for root in ["crate::", "bullterrier_control::", "super::super"] {
        for (start, _) in code.match_indices(root) {
            let before = code[..start].chars().next_back().unwrap_or(' ');
            if before.is_alphanumeric() || before == '_' || before == ':' {
                continue; // a part of a longer name or path
            }
            let rest = &code[start..];
            let end = rest
                .find(|character: char| !(character.is_alphanumeric() || character == '_' || character == ':'))
                .unwrap_or(rest.len());
            let path = rest[..end].replacen("bullterrier_control::", "crate::", 1);
            let grouped = path.ends_with("::") && rest[end..].trim_start().starts_with('{');
            paths.push(if grouped { format!("{path}{{") } else { path });
        }
    }
    paths
}

/// The paths of `text` that are not allowed.
fn refused(text: &str) -> Vec<String> {
    named(text)
        .into_iter()
        .filter(|path| {
            !ALLOWED
                .iter()
                .any(|allowed| path == allowed || path.strip_prefix(allowed).is_some_and(|rest| rest.starts_with("::")))
        })
        .collect()
}

/// The patch's source files: `src/patch.rs` and every file under `src/patch/`.
fn sources() -> std::io::Result<Vec<PathBuf>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![root.join("patch.rs")];
    files.extend(rust_files(&root.join("patch"))?);
    Ok(files)
}

#[test]
fn the_scan_refuses_a_path_beyond_the_list_and_sees_only_code() {
    // (row, text, the paths refused)
    let rows: [(&str, &str, &[&str]); 13] = [
        (
            "public items, one a line",
            "use crate::registry::Thing;\nuse crate::formats::Bundle;\n",
            &[],
        ),
        ("an item renamed", "use crate::formats::is_name as is_bundle_name;", &[]),
        (
            "a variant of an allowed item",
            "let kind = crate::registry::Kind::Shaper;",
            &[],
        ),
        (
            "an item of the crate's own",
            "use crate::registry::shaped;",
            &["crate::registry::shaped"],
        ),
        (
            "a pub(crate) item of the formats",
            "crate::formats::push_with(problems, make)",
            &["crate::formats::push_with"],
        ),
        (
            "a pub(crate) item of the built-ins, whose files' items are allowed",
            "crate::builtin::register(&mut registry)",
            &["crate::builtin::register"],
        ),
        (
            "an item whose name starts like an allowed one",
            "use crate::registry::Things;",
            &["crate::registry::Things"],
        ),
        (
            "a grouped import",
            "use crate::registry::{Thing, shaped};",
            &["crate::registry::{"],
        ),
        (
            "the crate by its own name",
            "bullterrier_control::registry::quoted(text)",
            &["crate::registry::quoted"],
        ),
        (
            "a path out of the patch",
            "use super::super::builtin;",
            &["super::super::builtin"],
        ),
        (
            "in comments",
            "// crate::registry::shaped\n/* crate::x /* nested */ crate::y */ let a = 1;",
            &[],
        ),
        (
            "in strings",
            "let a = \"crate::x \\\" crate::y\"; let b = r#\"crate::z \" crate::w\"#; let c = '\"';",
            &[],
        ),
        (
            "after a lifetime and a character",
            "fn f<'a>(x: &'a str) -> char { 'x' } use crate::secret;",
            &["crate::secret"],
        ),
    ];
    for (row, text, expected) in rows {
        assert_eq!(refused(text), expected, "{row}");
    }
}

#[test]
fn the_patch_names_only_public_items_of_the_modules_it_uses() -> std::io::Result<()> {
    let files = sources()?;
    let mut paths = 0;
    for file in &files {
        let text = fs::read_to_string(file)?;
        paths += named(&text).len();
        assert_eq!(refused(&text), Vec::<String>::new(), "{}", file.display());
    }
    // the scan saw the patch: its files, and the paths they name
    assert!(files.len() >= 6 && paths >= 40, "{} files, {paths} paths", files.len());
    Ok(())
}

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

/// The node modules: every file under `src/signal/` but the walk.
fn node_modules() -> std::io::Result<Vec<PathBuf>> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("signal");
    Ok(rust_files(&folder)?
        .into_iter()
        .filter(|path| {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
            name != "walk.rs"
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
