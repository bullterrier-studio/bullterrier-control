//! The Rust source of the built-in table, as a function of the built-in files: the build script writes it, and the
//! crate's tests (which include this file) check it on made-up files.
//!
//! The convention, read from the text on purpose (a build script cannot ask the compiler): a file whose code
//! implements one of [`KINDS`]' traits for `Params` (`impl Shaper for Params`; the trait by any path,
//! `crate::Shaper`, the struct as `Params` or `self::Params`, with or without generics) is a thing the core runs, of
//! that kind; one without only declares its parameters (its thing still runs in Python). Comments, docs, string and
//! character literals and `macro_rules!` bodies are not its code. Its name is the file's; its doc (what `help()` shows) is the
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

/// The kinds of thing a file can be: the `Thing` constructor that registers it and its trait.
const KINDS: [(&str, &str); 3] = [("source", "Source"), ("shaper", "Shaper"), ("operation", "Operation")];

/// The Rust of the table of `files` (sorted by name), or the error naming a file whose text marks it as more than one
/// kind.
pub(crate) fn table(files: &[File<'_>]) -> Result<Generated, String> {
    let mut builtin = String::new();
    let mut lib = String::new();
    let mut registered = String::new();
    let mut checks = String::new();
    for file in files {
        let implemented = implemented(&tokens(file.source));
        let kinds: Vec<(&str, &str)> = KINDS
            .into_iter()
            .filter(|(_, named)| implemented.contains(named))
            .collect();
        let kind = match kinds.as_slice() {
            [] => None,
            [kind] => Some(*kind),
            _ => {
                let names: Vec<&str> = kinds.iter().map(|(kind, _)| *kind).collect();
                return Err(format!("{}: a built-in is one of {}", file.path, names.join(", ")));
            }
        };
        write(
            &mut builtin,
            format_args!("#[path = {:?}]\npub mod {};\n", file.path, file.name),
        );
        if let Some(kind) = kind {
            registration(&mut checks, &mut registered, file, kind);
        }
        if kind.is_some_and(|(kind, _)| kind == "source") {
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
    builtin.push_str(&checks);
    builtin.push_str("/// Registers every built-in the core runs, under its name, with its doc.\n");
    builtin
        .push_str("pub(crate) fn register(registry: &mut crate::registry::Registry) -> Result<(), crate::Error> {\n");
    builtin.push_str(&registered);
    builtin.push_str("    let _ = registry;\n    Ok(())\n}\n");
    Ok(Generated { builtin, lib })
}

/// Writes into `checks` a compile-time check that the trait's `NAME` of `file`, a thing of `kind` (its constructor
/// and its trait), is the file's name, and into `code` its registration with its doc.
fn registration(checks: &mut String, code: &mut String, file: &File<'_>, (kind, named): (&str, &str)) {
    let name = file.name;
    write(
        checks,
        format_args!(
            "const _: () = assert!(\n    named(<{name}::Params as crate::{named}>::NAME, {name:?}),\n    \
             \"the NAME of a built-in is its file's name: {name}\"\n);\n"
        ),
    );
    write(
        code,
        format_args!(
            "    registry.register(crate::registry::Thing::{kind}::<{name}::Params>(crate::registry::About {{\n        \
             doc: {:?}.to_owned(),\n        params_title: {:?}.to_owned(),\n        \
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
            "/// The signal of the source [`{name}`](crate::builtin::{name}) with `params`, made through the one door \
             of the\n/// [registry](crate::registry): checked, named and recorded as every door makes it.\n///\n\
             /// # Errors\n/// As [`Thing::call`](crate::registry::Thing::call): [`Error::Params`](crate::Error::Params) \
             under the name `{name}`\n/// listing every rule `params` breaks, and whatever error the source gives.\n\
             pub fn {name}(params: crate::builtin::{name}::Params) -> Result<crate::Signal, crate::Error> {{\n    \
             crate::registry::sourced(params)\n}}\n"
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

/// The traits (by their last name) that `tokens` implement for `Params` (or `self::Params`): `impl`, generics if any,
/// a path, `for`, the struct's path.
fn implemented<'a>(tokens: &[&'a str]) -> Vec<&'a str> {
    let mut traits = Vec::new();
    for (at, _) in tokens.iter().enumerate().filter(|(_, token)| **token == "impl") {
        let mut at = at + 1;
        if tokens.get(at) == Some(&"<") {
            at = after_balanced(tokens, at, "<", ">");
        }
        let (named, after) = path(tokens, at);
        if tokens.get(after) != Some(&"for") {
            continue;
        }
        let (target, end) = path(tokens, after + 1);
        let whole = &tokens[after + 1..end];
        if target == Some("Params")
            && (whole == ["Params"] || whole == ["self", "::", "Params"])
            && let Some(named) = named
        {
            traits.push(named);
        }
    }
    traits
}

/// The last name of the path (`a::b::C`) at `at` in `tokens`, and where it ends; `None` if there is none.
fn path<'a>(tokens: &[&'a str], mut at: usize) -> (Option<&'a str>, usize) {
    let mut last = None;
    while let Some(token) = tokens.get(at).filter(|token| is_word(token)) {
        last = Some(*token);
        at += 1;
        if tokens.get(at) != Some(&"::") {
            break;
        }
        at += 1;
    }
    (last, at)
}

/// Where the group opened by `open` at `at` in `tokens` ends: after its matching `close`.
fn after_balanced(tokens: &[&str], mut at: usize, open: &str, close: &str) -> usize {
    let mut depth = 0_usize;
    while let Some(token) = tokens.get(at) {
        at += 1;
        if *token == open {
            depth += 1;
        } else if *token == close {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
    }
    at
}

/// Whether `token` is a word: an identifier, a keyword or a number.
fn is_word(token: &str) -> bool {
    token
        .chars()
        .all(|character| character == '_' || character.is_alphanumeric())
}

/// The tokens of `source`'s code: words, `::` and single marks, without comments (line and nested block, docs
/// included), string literals (raw and byte ones too), character literals and the bodies of `macro_rules!`.
fn tokens(source: &str) -> Vec<&str> {
    let characters: Vec<(usize, char)> = source.char_indices().collect();
    let at = |index: usize| characters.get(index).map(|(_, character)| *character);
    let offset = |index: usize| characters.get(index).map_or(source.len(), |(offset, _)| *offset);
    let mut tokens = Vec::new();
    let mut index = 0;
    while let Some(character) = at(index) {
        let next = at(index + 1);
        if character.is_whitespace() {
            index += 1;
        } else if character == '/' && next == Some('/') {
            while at(index).is_some_and(|character| character != '\n') {
                index += 1;
            }
        } else if character == '/' && next == Some('*') {
            let mut depth = 0_usize;
            while let Some(character) = at(index) {
                match (character, at(index + 1)) {
                    ('/', Some('*')) => (depth, index) = (depth + 1, index + 2),
                    ('*', Some('/')) => (depth, index) = (depth - 1, index + 2),
                    _ => index += 1,
                }
                if depth == 0 {
                    break;
                }
            }
        } else if character == '"' {
            index = after_string(&at, index + 1, None);
        } else if let Some((opening, hashes)) = raw_string(&at, index) {
            index = after_string(&at, index + opening, Some(hashes));
        } else if character == '\'' && next == Some('\\') {
            index += 3; // an escaped character literal (`'\''`, `'\u{7}'`): past the escaped one, to its closing quote
            while at(index).is_some_and(|character| character != '\'') {
                index += 1;
            }
            index += 1;
        } else if character == '\'' && at(index + 2) == Some('\'') {
            index += 3; // a character literal (`'"'`), not a lifetime (`'a`)
        } else if character == ':' && next == Some(':') {
            tokens.push("::");
            index += 2;
        } else if character == '_' || character.is_alphanumeric() {
            let start = index;
            while at(index).is_some_and(|character| character == '_' || character.is_alphanumeric()) {
                index += 1;
            }
            tokens.push(&source[offset(start)..offset(index)]);
        } else {
            tokens.push(&source[offset(index)..offset(index + 1)]);
            index += 1;
        }
    }
    without_macro_bodies(&tokens)
}

/// `tokens` without the bodies of the `macro_rules!` they define.
fn without_macro_bodies<'a>(tokens: &[&'a str]) -> Vec<&'a str> {
    let mut kept = Vec::new();
    let mut at = 0;
    while let Some(token) = tokens.get(at) {
        if *token == "macro_rules" && tokens.get(at + 1) == Some(&"!") {
            let open = at + 3; // after the macro's name
            at = match tokens.get(open) {
                Some(&"{") => after_balanced(tokens, open, "{", "}"),
                Some(&"(") => after_balanced(tokens, open, "(", ")"),
                Some(&"[") => after_balanced(tokens, open, "[", "]"),
                _ => at + 1,
            };
        } else {
            kept.push(*token);
            at += 1;
        }
    }
    kept
}

/// If a raw string starts at `index` (`r"`, `r#"`, `br##"`, ...): how many characters its opening takes and how many
/// `#` close it.
fn raw_string(at: &impl Fn(usize) -> Option<char>, index: usize) -> Option<(usize, usize)> {
    let before = index.checked_sub(1).and_then(at);
    if before.is_some_and(|character| character == '_' || character.is_alphanumeric()) {
        return None; // inside a word (`for"`), not a prefix
    }
    let mut opening = match (at(index), at(index + 1)) {
        (Some('r'), _) => 1,
        (Some('b'), Some('r')) => 2,
        _ => return None,
    };
    let mut hashes = 0;
    while at(index + opening) == Some('#') {
        (opening, hashes) = (opening + 1, hashes + 1);
    }
    (at(index + opening) == Some('"')).then_some((opening + 1, hashes))
}

/// Where a string whose text starts at `index` ends: after its closing `"`, and for a raw string (`raw` holding how
/// many `#` close it) after those, with no escapes inside.
fn after_string(at: &impl Fn(usize) -> Option<char>, mut index: usize, raw: Option<usize>) -> usize {
    while let Some(character) = at(index) {
        index += 1;
        match raw {
            None if character == '\\' => index += 1,
            None if character == '"' => return index,
            Some(hashes) if character == '"' && (index..index + hashes).all(|after| at(after) == Some('#')) => {
                return index + hashes;
            }
            _ => {}
        }
    }
    index
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
        // each registered one's trait `NAME` is checked to be its file's name when the core compiles
        for (name, kind) in [("rises", "Source"), ("shapes", "Shaper"), ("picks", "Operation")] {
            let check = format!("named(<{name}::Params as crate::{kind}>::NAME, {name:?})");
            assert!(code.builtin.contains(&check), "{name}: {}", code.builtin);
        }
        assert!(!code.builtin.contains("named(<plain::Params"), "{}", code.builtin);
        // every file declares its parameters; only a source is a `lib` function
        let params = code.builtin.split("static PARAMS").nth(1).unwrap_or_default();
        for name in ["plain", "recurs", "rises", "shapes", "picks"] {
            assert!(
                params.contains(&format!("({name:?}, <{name}::Params")),
                "{name}: {}",
                code.builtin
            );
        }
        // through the one door, as every way of making a signal is
        assert!(
            code.lib.contains(
                "pub fn rises(params: crate::builtin::rises::Params) -> Result<crate::Signal, crate::Error> {\n    \
                 crate::registry::sourced(params)\n}"
            ),
            "{}",
            code.lib
        );
        for name in ["plain", "recurs", "shapes", "picks"] {
            assert!(!code.lib.contains(&format!("fn {name}(")), "{name}: {}", code.lib);
        }
    }

    #[test]
    fn a_file_is_a_thing_only_where_its_code_implements_a_trait_for_params() {
        // (row, the file's text, the kind it is registered as)
        let rows: [(&str, &str, Option<&str>); 22] = [
            ("plain", "impl Shaper for Params {}", Some("shaper")),
            (
                "by the crate's path",
                "impl crate::Shaper for Params {}",
                Some("shaper"),
            ),
            (
                "by the module's path",
                "impl crate::registry::Operation for self::Params {}",
                Some("operation"),
            ),
            (
                "by the package's path",
                "impl bullterrier_control::Source for Params {}",
                Some("source"),
            ),
            ("with generics", "impl<'a> Source for Params {}", Some("source")),
            ("over lines", "impl\n    Shaper\n    for Params\n{}", Some("shaper")),
            (
                "after a quote as a character",
                "const Q: char = '\"';\nimpl Shaper for Params {}",
                Some("shaper"),
            ),
            (
                "after an escaped quote",
                "const Q: char = '\\'';\nimpl Shaper for Params {}",
                Some("shaper"),
            ),
            (
                "after a raw string ending in a backslash",
                "const S: &str = r\"\\\";\nimpl Shaper for Params {}",
                Some("shaper"),
            ),
            (
                "a comment marking another kind",
                "// impl Source for Params\nimpl Shaper for Params {}",
                Some("shaper"),
            ),
            ("in a comment", "// impl Shaper for Params\n", None),
            ("in a module doc", "//! impl Shaper for Params\n", None),
            (
                "in a doc",
                "/// ```\n/// impl Shaper for Params {}\n/// ```\npub struct Params {}\n",
                None,
            ),
            (
                "in a nested block comment",
                "/* a /* b */ impl Shaper for Params {} */",
                None,
            ),
            ("in a string", "const S: &str = \"impl Shaper for Params\";", None),
            (
                "in a string after an escaped quote",
                "const S: &str = \"a \\\" impl Shaper for Params\";",
                None,
            ),
            (
                "in a raw string",
                "const S: &str = r#\"impl \"Shaper\" for Params\"#;",
                None,
            ),
            (
                "in a macro",
                "macro_rules! made { () => { impl Shaper for Params {} }; }",
                None,
            ),
            ("for another struct", "impl Shaper for Other {}", None),
            ("for another module's", "impl Shaper for other::Params {}", None),
            ("for a longer name", "impl Shaper for ParamsTwo {}", None),
            ("of another trait", "impl MyShaper for Params {}", None),
        ];
        for (row, source, kind) in rows {
            let code = built(&[file("it", source)]);
            let registered = code.builtin.split("fn register").nth(1).unwrap_or_default();
            let found = ["source", "shaper", "operation"]
                .into_iter()
                .find(|kind| registered.contains(&format!("Thing::{kind}::<it::Params>")));
            assert_eq!(found, kind, "{row}: {}", code.builtin);
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
        let expected = "doc: \"Lifts it.\\n\\nBy `depth`, \\\"exactly\\\".\".to_owned(),\n        \
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
