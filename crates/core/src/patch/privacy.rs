//! An architecture test (it reads source text, which a behaviour test never does): the patch uses only what the
//! modules below it make public. The compiler already refuses a private item of another module; what it allows inside
//! the crate is a `pub(crate)` one, which the patch must not reach for either. So every path into the crate that the
//! patch's code names (`crate::..`, or the crate by its own name) must be one of [`ALLOWED`], each a public item of
//! its module; a grouped import (`crate::x::{a, b}`) and a path out of the patch (`super::super`) are refused too, so
//! no path escapes the list. A `pub(crate)` method of a public type is not a path and is not seen here.

use std::fs;
use std::path::{Path, PathBuf};

/// The public items the patch may name, each with its module's path (a path below one, such as an enum's variant,
/// is allowed with it).
const ALLOWED: &[&str] = &[
    "crate::Error",
    "crate::FromValues",
    "crate::Interpolation",
    "crate::Origin",
    "crate::Rendering",
    "crate::Signal",
    "crate::builtin",
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

/// `text` without its comments and the insides of its string and character literals (each left as `""` or `''`),
/// so only code is searched.
fn code(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut kept = String::with_capacity(text.len());
    let mut index = 0;
    let at = |index: usize| characters.get(index).copied().unwrap_or('\0');
    while index < characters.len() {
        let character = characters[index];
        match (character, at(index + 1)) {
            ('/', '/') => {
                while index < characters.len() && characters[index] != '\n' {
                    index += 1;
                }
            }
            ('/', '*') => {
                let mut depth = 0;
                loop {
                    match (at(index), at(index + 1)) {
                        ('/', '*') => (depth, index) = (depth + 1, index + 2),
                        ('*', '/') => (depth, index) = (depth - 1, index + 2),
                        ('\0', _) => break,
                        _ => index += 1,
                    }
                    if depth == 0 {
                        break;
                    }
                }
            }
            ('r', '#' | '"') if !at(index.wrapping_sub(1)).is_alphanumeric() => {
                // a raw string: r"..", r#".."#, ..
                let mut hashes = 0;
                index += 1;
                while at(index) == '#' {
                    hashes += 1;
                    index += 1;
                }
                if at(index) != '"' {
                    kept.push('r');
                    continue;
                }
                index += 1;
                while index < characters.len()
                    && !(characters[index] == '"' && (1..=hashes).all(|ahead| at(index + ahead) == '#'))
                {
                    index += 1;
                }
                index += 1 + hashes;
                kept.push_str("\"\"");
            }
            ('"', _) => {
                index += 1;
                while index < characters.len() && characters[index] != '"' {
                    index += if characters[index] == '\\' { 2 } else { 1 };
                }
                index += 1;
                kept.push_str("\"\"");
            }
            ('\'', next) if next == '\\' || at(index + 2) == '\'' => {
                // a character literal ('x', '\n', '\''); a lifetime ('a) is code
                index += 2;
                while index < characters.len() && characters[index] != '\'' {
                    index += 1;
                }
                index += 1;
                kept.push_str("''");
            }
            _ => {
                kept.push(character);
                index += 1;
            }
        }
    }
    kept
}

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
    let mut folders = vec![root.join("patch")];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(folder)? {
            let path = entry?.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    Ok(files)
}

#[test]
fn the_scan_refuses_a_path_beyond_the_list_and_sees_only_code() {
    // (row, text, the paths refused)
    let rows: [(&str, &str, &[&str]); 12] = [
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
    assert!(files.len() >= 7 && paths >= 40, "{} files, {paths} paths", files.len());
    Ok(())
}
