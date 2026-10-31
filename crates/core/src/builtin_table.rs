//! The Rust source of the built-in table, as a function of the built-in files: the build script writes it, and the
//! crate's tests (which include this file) check it on made-up files.
//!
//! The convention, textual on purpose (a build script cannot ask the compiler): a file whose text holds one of
//! [`KINDS`]' markers (`impl Shaper for Params`) is a thing the core runs, of that kind; one without only declares
//! its parameters (its thing still runs in Python). Its name is the file's; its doc (what `help()` shows) is the
//! file's leading `//!` lines; its parameters' doc is the `///` lines above `pub struct Params`, and their title the
//! name in title case with `Params` (`every_n` is `EveryNParams`), as the Python classes are named.

use std::fmt::Write as _;

/// One file of `src/builtin/`.
pub(crate) struct File<'a> {
    /// The built-in's name: the file's name without `.rs`.
    pub(crate) name: &'a str,
    /// Where the file is, as the generated `mod` declaration names it.
    pub(crate) path: &'a str,
    /// What the file says.
    pub(crate) source: &'a str,
}

/// The generated Rust: [`builtin`](Self::builtin) for `src/builtin.rs`, [`lib`](Self::lib) for the `lib` module.
pub(crate) struct Generated {
    /// A public module for each file; `PARAMS`, every built-in's parameter table by name; and `register`, which
    /// registers every thing the core runs.
    pub(crate) builtin: String,
    /// A free function for each source the core runs.
    pub(crate) lib: String,
}

/// The kinds of thing a file can be: the text that marks it, and the `Thing` constructor that registers it.
const KINDS: [(&str, &str); 3] = [
    ("impl Source for Params", "source"),
    ("impl Shaper for Params", "shaper"),
    ("impl Operation for Params", "operation"),
];

/// The Rust of the table of `files` (sorted by name), or the error naming a file whose text marks it as more than one
/// kind.
pub(crate) fn table(files: &[File<'_>]) -> Result<Generated, String> {
    let mut builtin = String::new();
    let mut lib = String::new();
    let mut registered = String::new();
    for file in files {
        let kinds: Vec<&str> = KINDS
            .iter()
            .filter(|(marker, _)| file.source.contains(marker))
            .map(|(_, kind)| *kind)
            .collect();
        let kind = match kinds.as_slice() {
            [] => None,
            [kind] => Some(*kind),
            _ => return Err(format!("{}: a built-in is one of {}", file.path, kinds.join(", "))),
        };
        write(
            &mut builtin,
            format_args!("#[path = {:?}]\npub mod {};\n", file.path, file.name),
        );
        if let Some(kind) = kind {
            registration(&mut registered, file, kind);
        }
        if kind == Some("source") {
            source_function(&mut lib, file.name);
        }
    }
    builtin.push_str("/// Every built-in by name, sorted, with the function that gives its parameter table.\n");
    builtin.push_str("pub(crate) static PARAMS: &[(&str, Table)] = &[\n");
    for File { name, .. } in files {
        write(
            &mut builtin,
            format_args!("    ({name:?}, <{name}::Params as crate::Params>::table),\n"),
        );
    }
    builtin.push_str("];\n");
    builtin.push_str("/// Registers every built-in the core runs, under its name, with its doc.\n");
    builtin
        .push_str("pub(crate) fn register(registry: &mut crate::registry::Registry) -> Result<(), crate::Error> {\n");
    builtin.push_str(&registered);
    builtin.push_str("    let _ = registry;\n    Ok(())\n}\n");
    Ok(Generated { builtin, lib })
}

/// Writes into `code` the registration of `file`, a thing of `kind`.
fn registration(code: &mut String, file: &File<'_>, kind: &str) {
    let name = file.name;
    write(
        code,
        format_args!(
            "    registry.register(crate::registry::Thing::{kind}::<{name}::Params>(crate::registry::About {{\n        \
             name: {name:?}.to_owned(),\n        doc: {:?}.to_owned(),\n        params_title: {:?}.to_owned(),\n        \
             params_doc: {:?}.to_owned(),\n    }})?)?;\n",
            doc(file.source),
            title(name),
            params_doc(file.source)
        ),
    );
}

/// Writes into `code` the `lib` function of the source `name`.
fn source_function(code: &mut String, name: &str) {
    write(
        code,
        format_args!(
            "/// The signal of the source [`{name}`](crate::builtin::{name}) with `params`, checked first.\n///\n\
             /// # Errors\n/// [`Error::Params`](crate::Error::Params) listing every rule `params` breaks, and \
             whatever error the source gives.\n\
             pub fn {name}(params: crate::builtin::{name}::Params) -> Result<crate::Signal, crate::Error> {{\n    \
             let params = crate::Params::check(&params).map_err(|error| crate::Error::params({name:?}, error))?;\n    \
             crate::Source::source(&params)\n}}\n"
        ),
    );
}

/// The thing's doc: the file's leading `//!` lines, each without the marker and one space, blank lines around it
/// dropped.
fn doc(source: &str) -> String {
    let lines: Vec<&str> = source
        .lines()
        .map_while(|line| line.trim_start().strip_prefix("//!"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect();
    lines.join("\n").trim().to_owned()
}

/// The parameters' doc: the `///` lines above `pub struct Params` (attributes between them skipped), each without the
/// marker and one space; empty if there are none.
fn params_doc(source: &str) -> String {
    let lines: Vec<&str> = source.lines().map(str::trim_start).collect();
    let Some(at) = lines.iter().position(|line| line.starts_with("pub struct Params")) else {
        return String::new();
    };
    let mut doc: Vec<&str> = lines[..at]
        .iter()
        .rev()
        .skip_while(|line| line.starts_with("#["))
        .map_while(|line| line.strip_prefix("///"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect();
    doc.reverse();
    doc.join("\n").trim().to_owned()
}

/// The parameters' title: the name in title case, then `Params` (`every_n` is `EveryNParams`).
fn title(name: &str) -> String {
    let mut title = String::new();
    for part in name.split('_') {
        let mut characters = part.chars();
        if let Some(first) = characters.next() {
            title.extend(first.to_uppercase());
            title.extend(characters);
        }
    }
    title + "Params"
}

/// Writes `text` into `code` (writing into a `String` cannot fail).
fn write(code: &mut String, text: std::fmt::Arguments<'_>) {
    let _ = code.write_fmt(text);
}

#[cfg(test)]
mod tests {
    use super::{File, Generated, table};

    fn file<'a>(name: &'a str, source: &'a str) -> File<'a> {
        File {
            name,
            path: name,
            source,
        }
    }

    /// The generated code for `files`, or the error as its text.
    fn built(files: &[File<'_>]) -> Generated {
        table(files).unwrap_or_else(|error| Generated {
            builtin: error,
            lib: String::new(),
        })
    }

    #[test]
    fn a_file_marked_as_a_kind_is_registered_as_one_and_one_without_only_declares() {
        let code = built(&[
            file("plain", "pub struct Params {}\n"),
            file(
                "recurs",
                "pub struct Params {}\npub(crate) fn shape(input: &Signal, params: &Params) {}\n",
            ),
            file("rises", "pub struct Params {}\nimpl Source for Params {}\n"),
            file("shapes", "pub struct Params {}\nimpl Shaper for Params {}\n"),
            file("picks", "pub struct Params {}\nimpl Operation for Params {}\n"),
        ]);
        let registered = code.builtin.split("fn register").nth(1).unwrap_or_default();
        for (name, kind) in [("rises", "source"), ("shapes", "shaper"), ("picks", "operation")] {
            assert!(
                registered.contains(&format!("Thing::{kind}::<{name}::Params>")),
                "{name}: {}",
                code.builtin
            );
        }
        for name in ["plain", "recurs"] {
            assert!(!registered.contains(name), "{name}: {}", code.builtin);
        }
        // every file declares its parameters; only a source is a `lib` function
        let params = code.builtin.split("static PARAMS").nth(1).unwrap_or_default();
        for name in ["plain", "recurs", "rises", "shapes", "picks"] {
            assert!(
                params.contains(&format!("({name:?}, <{name}::Params")),
                "{name}: {}",
                code.builtin
            );
        }
        assert!(
            code.lib.contains("pub fn rises(params: crate::builtin::rises::Params)"),
            "{}",
            code.lib
        );
        for name in ["plain", "recurs", "shapes", "picks"] {
            assert!(!code.lib.contains(&format!("fn {name}(")), "{name}: {}", code.lib);
        }
    }

    #[test]
    fn a_file_marked_as_two_kinds_is_refused_naming_it() {
        let refused = table(&[file("both", "impl Source for Params {}\nimpl Shaper for Params {}\n")]).err();
        assert_eq!(refused.as_deref(), Some("both: a built-in is one of source, shaper"));
    }

    #[test]
    fn a_thing_takes_its_doc_from_the_leading_lines_and_its_parameters_doc_from_above_the_struct() {
        let source = "//! Lifts it.\n//!\n//! By `depth`, \"exactly\".\n\nuse crate::Signal;\n\n/// How far.\n/// \
                      And how.\n#[derive(Clone, crate::Params)]\n#[param(rule = rule)]\npub struct Params {}\n\
                      impl Shaper for Params {}\n";
        let code = built(&[file("lift_it", source)]);
        let expected = "name: \"lift_it\".to_owned(),\n        doc: \"Lifts it.\\n\\nBy `depth`, \\\"exactly\\\".\".to_owned(),\n        \
                        params_title: \"LiftItParams\".to_owned(),\n        params_doc: \"How far.\\nAnd how.\".to_owned(),";
        assert!(code.builtin.contains(expected), "{}", code.builtin);
        // a file without either has an empty doc
        let bare = built(&[file("bare", "pub struct Params {}\nimpl Source for Params {}\n")]);
        assert!(
            bare.builtin
                .contains("doc: \"\".to_owned(),\n        params_title: \"BareParams\""),
            "{}",
            bare.builtin
        );
        assert!(bare.builtin.contains("params_doc: \"\".to_owned()"), "{}", bare.builtin);
    }
}
