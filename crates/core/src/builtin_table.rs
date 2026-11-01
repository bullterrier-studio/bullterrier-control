//! The Rust source of the built-in table, as a function of the built-in files: the build script writes it, and the
//! crate's tests (which include this file) check it on made-up files.
//!
//! The convention, read from the text on purpose (a build script cannot ask the compiler): every file's code
//! implements exactly one of [`KINDS`]' traits for `Params` (`impl Shaper for Params`; the trait by any path,
//! `crate::Shaper`, the struct as `Params` or `self::Params`, with or without generics), and is a thing the core runs,
//! of that kind; a file with none or two is refused. Comments, docs, string and character literals and `macro_rules!`
//! bodies are not its code. Its name is the file's; its doc (what `help()` shows) is the file's leading `//!` lines;
//! its parameters' doc is the `///` lines above `pub struct Params`, and their title the name in title case with
//! `Params` (`every_n` is `EveryNParams`), as the Python classes are named.
//!
//! A source's `lib` function takes what `docs/api.md` rule 2 says, read from its `type Inputs` (one input's data, or
//! `Inputs`, the file's `pub struct Inputs { pub <input>: <data>, .. }` of several): each required input borrowed
//! (`&[Event]`, `&Grid`) before its parameters, in the order declared, each optional one (`Option<&Grid>`) after them,
//! and no parameters if `Params` has no fields; it clones the inputs once, into what the door records.

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
    /// A public module for each file, and `register`, which registers each as the thing the core runs.
    pub(crate) builtin: String,
    /// A free function for each source the core runs.
    pub(crate) lib: String,
}

/// The kinds of thing a file can be: the `Thing` constructor that registers it and its trait.
const KINDS: [(&str, &str); 3] = [("source", "Source"), ("shaper", "Shaper"), ("operation", "Operation")];

/// The inputs a source's `lib` function takes: (its data as `type Inputs` writes it, paths left out; the input's name;
/// the type the function borrows, a slice's in brackets). One of them, or an `Option` of one.
const BORROWED: [(&str, &str, &str); 5] = [
    ("Vec<Event>", "events", "[crate::formats::Event]"),
    ("Vec<Keyframe>", "points", "[crate::inputs::Keyframe]"),
    ("Vec<f64>", "values", "[f64]"),
    ("Grid", "grid", "crate::formats::Grid"),
    ("Series", "series", "crate::formats::Series"),
];

/// The Rust of the table of `files` (sorted by name), or the error naming a file whose text marks it as no kind or
/// more than one, or a source whose `type Inputs` no `lib` function is written for.
pub(crate) fn table(files: &[File<'_>]) -> Result<Generated, String> {
    let mut builtin = String::new();
    let mut lib = String::new();
    let mut registered = String::new();
    let mut checks = String::new();
    for file in files {
        let code = tokens(file.source);
        let implemented = implemented(&code);
        let kinds: Vec<(&str, &str)> = KINDS
            .into_iter()
            .filter(|(_, named)| implemented.contains(named))
            .collect();
        let kind = match kinds.as_slice() {
            [kind] => *kind,
            [] => {
                return Err(format!(
                    "{}: a built-in implements one of Source, Shaper and Operation for its Params \
                     (`impl Shaper for Params`)",
                    file.path
                ));
            }
            _ => {
                let names: Vec<&str> = kinds.iter().map(|(kind, _)| *kind).collect();
                return Err(format!("{}: a built-in is one of {}", file.path, names.join(", ")));
            }
        };
        write(
            &mut builtin,
            format_args!("#[path = {:?}]\npub mod {};\n", file.path, file.name),
        );
        registration(&mut checks, &mut registered, file, kind);
        if kind.0 == "source" {
            source_function(&mut lib, file, &code)?;
        }
    }
    builtin.push_str(&checks);
    builtin.push_str("/// Registers every built-in the core runs, under its name, with its doc.\n");
    builtin
        .push_str("#[allow(clippy::too_many_lines, reason = \"generated: five lines per built-in the core runs\")]\n");
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

/// One argument of a `lib` function: the input's name, whether it is optional, and the type it is borrowed as.
struct Argument {
    /// The input's name, which the argument and a struct's field are called.
    input: &'static str,
    /// Whether the input may be left out (`Option<..>`): it then comes after the parameters.
    optional: bool,
    /// The type the function borrows, a slice's in brackets.
    data: &'static str,
}

impl Argument {
    /// The argument of the data `written` (paths left out), required or an `Option` of one; `None` if
    /// [`BORROWED`] holds no such data.
    fn of(written: &str) -> Option<Self> {
        let inner = written
            .strip_prefix("Option<")
            .and_then(|inner| inner.strip_suffix('>'));
        let (_, input, data) = BORROWED.iter().find(|(data, ..)| *data == inner.unwrap_or(written))?;
        Some(Self {
            input,
            optional: inner.is_some(),
            data,
        })
    }

    /// The argument as the function declares it.
    fn declared(&self) -> String {
        let (input, data) = (self.input, self.data);
        if self.optional {
            format!("{input}: Option<&{data}>")
        } else {
            format!("{input}: &{data}")
        }
    }

    /// The argument cloned into the data the source takes.
    fn cloned(&self) -> String {
        let cloned = match (self.optional, self.data.starts_with('[')) {
            (false, true) => "to_vec()",
            (false, false) => "clone()",
            (true, true) => "map(<[_]>::to_vec)",
            (true, false) => "cloned()",
        };
        format!("{}.{cloned}", self.input)
    }
}

/// Writes into `code` the `lib` function of the source `file`, whose code is `tokens`; or the error naming the file
/// if its `type Inputs` is not `()`, one input's data ([`BORROWED`], or an `Option` of one) or the file's own
/// `Inputs` struct of them ([`fields`]).
fn source_function(code: &mut String, file: &File<'_>, tokens: &[&str]) -> Result<(), String> {
    let name = file.name;
    let fieldless = tokens.windows(3).any(|words| words == ["struct", "Params", ";"])
        || tokens.windows(4).any(|words| words == ["struct", "Params", "{", "}"]);
    // `Params {}` builds a unit struct too
    let (params, given) = if fieldless {
        (Vec::new(), format!("crate::builtin::{name}::Params {{}}"))
    } else {
        (
            vec![format!("params: crate::builtin::{name}::Params")],
            "params".to_owned(),
        )
    };
    let written = inputs(tokens);
    let (taken, inputs) = match (written.as_str(), Argument::of(&written)) {
        ("()", _) => (Vec::new(), "()".to_owned()),
        (_, Some(argument)) => {
            let cloned = argument.cloned();
            (vec![argument], cloned)
        }
        ("Inputs", None) => {
            let taken = fields(tokens).map_err(|problem| format!("{}: {problem}", file.path))?;
            let each: Vec<String> = taken
                .iter()
                .map(|field| format!("{}: {}", field.input, field.cloned()))
                .collect();
            let inputs = format!("crate::builtin::{name}::Inputs {{ {} }}", each.join(", "));
            (taken, inputs)
        }
        (_, None) => {
            let shown = if written.is_empty() {
                "none".to_owned()
            } else {
                format!("`{written}`")
            };
            return Err(format!(
                "{}: a source's `type Inputs` is (), one input's data (Vec<Event>, Vec<Keyframe>, Vec<f64>, Grid, \
                 Series), an Option of one, or `Inputs`, the file's struct of them; not {shown}",
                file.path
            ));
        }
    };
    let of = match (taken.is_empty(), fieldless) {
        (true, false) => "its parameters",
        (false, false) => "its inputs (borrowed, cloned once) and parameters",
        (false, true) => "its inputs (borrowed, cloned once)",
        (true, true) => "nothing",
    };
    let mut refusals = Vec::new();
    if !fieldless {
        refusals.push(format!(
            "[`Error::Params`](crate::Error::Params) under the name `{name}`\n/// listing every rule the parameters break"
        ));
    }
    if !taken.is_empty() {
        refusals.push("[`Error::Input`](crate::Error::Input) if an input breaks its kind's\n/// rules".to_owned());
    }
    let refusals = match refusals.as_slice() {
        [] => "whatever error the source gives".to_owned(),
        given => format!("{}, and whatever error the source gives", given.join(", ")),
    };
    let (optional, required): (Vec<&Argument>, Vec<&Argument>) = taken.iter().partition(|argument| argument.optional);
    let arguments: Vec<String> = required
        .iter()
        .map(|argument| argument.declared())
        .chain(params)
        .chain(optional.iter().map(|argument| argument.declared()))
        .collect();
    write(
        code,
        format_args!(
            "/// The signal of the source [`{name}`](crate::builtin::{name}) of {of}, made through the one\n/// door \
             of the [registry](crate::registry): checked, named and recorded as every door makes it.\n///\n\
             /// # Errors\n/// As [`Thing::call`](crate::registry::Thing::call): {refusals}.\npub fn {name}({}) -> Result<crate::Signal, crate::Error> {{\n    \
             crate::registry::sourced({given}, {inputs})\n}}\n",
            arguments.join(", ")
        ),
    );
    Ok(())
}

/// What the code `tokens` says its `type Inputs` is, without spaces and with paths left out (`Option<Grid>` for
/// `Option<crate::formats::Grid>`); empty if it says none.
fn inputs(tokens: &[&str]) -> String {
    let Some(at) = tokens.windows(3).position(|words| words == ["type", "Inputs", "="]) else {
        return String::new();
    };
    let written: Vec<&str> = tokens[at + 3..]
        .iter()
        .copied()
        .take_while(|token| *token != ";")
        .collect();
    plain(&written)
}

/// The arguments of the fields of the code `tokens`' `pub struct Inputs { pub <input>: <data>, .. }`, in their order:
/// each field named as its input and of one input's data ([`Argument::of`]); or what is wrong with it.
fn fields(tokens: &[&str]) -> Result<Vec<Argument>, String> {
    let shape = "`type Inputs = Inputs` needs `pub struct Inputs { pub <input>: <its data>, .. }`";
    let at = tokens
        .windows(4)
        .position(|words| words == ["pub", "struct", "Inputs", "{"])
        .ok_or_else(|| format!("{shape}, which the file lacks"))?;
    let end = after_balanced(tokens, at + 3, "{", "}") - 1;
    let mut fields = Vec::new();
    let mut depth = 0_usize;
    let mut field = Vec::new();
    for token in &tokens[at + 4..end] {
        match *token {
            "<" | "(" => depth += 1,
            ">" | ")" => depth = depth.saturating_sub(1),
            _ => {}
        }
        if *token == "," && depth == 0 {
            fields.push(std::mem::take(&mut field));
        } else {
            field.push(*token);
        }
    }
    fields.push(field);
    let mut arguments = Vec::new();
    for field in fields.iter().filter(|field| !field.is_empty()) {
        let argument = match field.as_slice() {
            ["pub", name, ":", data @ ..] => Argument::of(&plain(data))
                .filter(|argument| argument.input == *name)
                .ok_or_else(|| {
                    let known: Vec<String> = BORROWED
                        .iter()
                        .map(|(data, input, _)| format!("{input}: {data}"))
                        .collect();
                    format!(
                        "{shape}: the field `{name}` is not an input's data under its name ({}, or an Option of one)",
                        known.join(", ")
                    )
                })?,
            _ => return Err(format!("{shape}, not `{}`", field.join(" "))),
        };
        arguments.push(argument);
    }
    Ok(arguments)
}

/// The type of the code `tokens` without spaces and with paths left out (`Option<Grid>` for
/// `Option<crate::formats::Grid>`).
fn plain(tokens: &[&str]) -> String {
    tokens
        .iter()
        .enumerate()
        .filter(|(index, token)| **token != "::" && tokens.get(index + 1) != Some(&"::"))
        .map(|(_, token)| *token)
        .collect()
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
    fn a_file_is_registered_as_the_kind_it_implements() {
        let code = built(&[
            file(
                "rises",
                "pub struct Params {\n    pub level: f64,\n}\nimpl Source for Params {\n    type Inputs = ();\n}\n",
            ),
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
        // each one's trait `NAME` is checked to be its file's name when the core compiles
        for (name, kind) in [("rises", "Source"), ("shapes", "Shaper"), ("picks", "Operation")] {
            let check = format!("named(<{name}::Params as crate::{kind}>::NAME, {name:?})");
            assert!(code.builtin.contains(&check), "{name}: {}", code.builtin);
        }
        // through the one door, as every way of making a signal is; only a source is a `lib` function
        assert!(
            code.lib.contains(
                "pub fn rises(params: crate::builtin::rises::Params) -> Result<crate::Signal, crate::Error> {\n    \
                 crate::registry::sourced(params, ())\n}"
            ),
            "{}",
            code.lib
        );
        for name in ["shapes", "picks"] {
            assert!(!code.lib.contains(&format!("fn {name}(")), "{name}: {}", code.lib);
        }
    }

    #[test]
    fn a_file_of_no_kind_or_of_two_is_refused_naming_it() {
        let none =
            "a built-in implements one of Source, Shaper and Operation for its Params (`impl Shaper for Params`)";
        // (row, the file's text, the refusal after its name)
        let rows = [
            ("parameters only", "pub struct Params {}\n", none),
            (
                "a body but no trait",
                "pub struct Params {}\npub(crate) fn shape(input: &Signal, params: &Params) {}\n",
                none,
            ),
            (
                "two kinds",
                "impl Source for Params {}\nimpl Shaper for Params {}\n",
                "a built-in is one of source, shaper",
            ),
        ];
        for (row, source, refusal) in rows {
            // among good files, the refused one is named
            let files = [file("fine", "impl Shaper for Params {}"), file("it", source)];
            assert_eq!(table(&files).err(), Some(format!("it: {refusal}")), "{row}");
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
                "impl bullterrier_control::Source for Params { type Inputs = (); }",
                Some("source"),
            ),
            (
                "with generics",
                "impl<'a> Source for Params { type Inputs = (); }",
                Some("source"),
            ),
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
            // a file of no kind is refused
            let found = table(&[file("it", source)]).ok().and_then(|code| {
                ["source", "shaper", "operation"]
                    .into_iter()
                    .find(|kind| code.builtin.contains(&format!("Thing::{kind}::<it::Params>")))
            });
            assert_eq!(found, kind, "{row}");
        }
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "a row per input a source can take, and the function each makes"
    )]
    fn a_source_is_a_lib_function_of_its_inputs_borrowed_required_first_and_optional_last() {
        let lib = |inputs: &str, params: &str| {
            let source = format!("{params}\nimpl Source for Params {{\n    type Inputs = {inputs};\n}}\n");
            // the code, without its doc
            let code = |code: Generated| {
                let lines: Vec<&str> = code.lib.lines().filter(|line| !line.starts_with("///")).collect();
                lines.join("\n") + "\n"
            };
            table(&[file("it", &source)]).map(code)
        };
        let fields = "pub struct Params {\n    pub level: f64,\n}";
        let made = |signature: &str, inputs: &str| {
            Ok(format!(
                "pub fn it({signature}) -> Result<crate::Signal, crate::Error> {{\n    \
                 crate::registry::sourced({inputs})\n}}\n"
            ))
        };
        let params = "params: crate::builtin::it::Params";
        let not_an_input = |field: &str| {
            format!(
                "it: `type Inputs = Inputs` needs `pub struct Inputs {{ pub <input>: <its data>, .. }}`: the field \
                 `{field}` is not an input's data under its name (events: Vec<Event>, points: Vec<Keyframe>, \
                 values: Vec<f64>, grid: Grid, series: Series, or an Option of one)"
            )
        };
        let three = format!(
            "{fields}\npub struct Inputs {{ pub grid: Option<Grid>, pub values: Vec<f64>, pub series: Series }}"
        );
        let over_lines = format!(
            "{fields}\npub struct Inputs {{\n    pub values: Vec<f64>,\n    pub grid: Option<crate::formats::Grid>,\n}}"
        );
        let of_f32 = format!("{fields}\npub struct Inputs {{ pub values: Vec<f32> }}");
        let kicks = format!("{fields}\npub struct Inputs {{ pub kicks: Vec<Event> }}");
        let private = format!("{fields}\npub struct Inputs {{ pub values: Vec<f64>, grid: Option<Grid> }}");
        // (row, its `type Inputs`, its parameters and inputs struct, the function's arguments and what it gives the door)
        let rows = [
            ("no inputs", "()", fields, made(params, "params, ()")),
            (
                "required events, borrowed and first",
                "Vec<Event>",
                fields,
                made(
                    &format!("events: &[crate::formats::Event], {params}"),
                    "params, events.to_vec()",
                ),
            ),
            (
                "required points",
                "Vec<Keyframe>",
                fields,
                made(
                    &format!("points: &[crate::inputs::Keyframe], {params}"),
                    "params, points.to_vec()",
                ),
            ),
            (
                "required values",
                "Vec<f64>",
                fields,
                made(&format!("values: &[f64], {params}"), "params, values.to_vec()"),
            ),
            (
                "a required grid",
                "Grid",
                fields,
                made(
                    &format!("grid: &crate::formats::Grid, {params}"),
                    "params, grid.clone()",
                ),
            ),
            (
                "a required series by its path",
                "crate::formats::Series",
                fields,
                made(
                    &format!("series: &crate::formats::Series, {params}"),
                    "params, series.clone()",
                ),
            ),
            (
                "an optional grid, last",
                "Option<Grid>",
                fields,
                made(
                    &format!("{params}, grid: Option<&crate::formats::Grid>"),
                    "params, grid.cloned()",
                ),
            ),
            (
                "optional events",
                "Option<Vec<crate::formats::Event>>",
                fields,
                made(
                    &format!("{params}, events: Option<&[crate::formats::Event]>"),
                    "params, events.map(<[_]>::to_vec)",
                ),
            ),
            (
                "no parameters, no struct",
                "Vec<Keyframe>",
                "pub struct Params;",
                made(
                    "points: &[crate::inputs::Keyframe]",
                    "crate::builtin::it::Params {}, points.to_vec()",
                ),
            ),
            (
                "no parameters in braces, no struct",
                "()",
                "pub struct Params {}",
                made("", "crate::builtin::it::Params {}, ()"),
            ),
            (
                "several inputs: the required ones first as declared, then the parameters, then the optional ones",
                "Inputs",
                three.as_str(),
                made(
                    &format!(
                        "values: &[f64], series: &crate::formats::Series, {params}, \
                         grid: Option<&crate::formats::Grid>"
                    ),
                    "params, crate::builtin::it::Inputs { grid: grid.cloned(), values: values.to_vec(), \
                     series: series.clone() }",
                ),
            ),
            (
                "several inputs by their paths, over lines, with a last comma",
                "self::Inputs",
                over_lines.as_str(),
                made(
                    &format!("values: &[f64], {params}, grid: Option<&crate::formats::Grid>"),
                    "params, crate::builtin::it::Inputs { values: values.to_vec(), grid: grid.cloned() }",
                ),
            ),
            (
                "several inputs and no parameters",
                "Inputs",
                "pub struct Params;\npub struct Inputs { pub events: Vec<Event>, pub grid: Grid }",
                made(
                    "events: &[crate::formats::Event], grid: &crate::formats::Grid",
                    "crate::builtin::it::Params {}, crate::builtin::it::Inputs { events: events.to_vec(), \
                     grid: grid.clone() }",
                ),
            ),
            (
                "several inputs without their struct",
                "Inputs",
                fields,
                Err(
                    "it: `type Inputs = Inputs` needs `pub struct Inputs { pub <input>: <its data>, .. }`, which \
                     the file lacks"
                        .to_owned(),
                ),
            ),
            (
                "a field of data no input holds",
                "Inputs",
                of_f32.as_str(),
                Err(not_an_input("values")),
            ),
            (
                "a field named apart from its input",
                "Inputs",
                kicks.as_str(),
                Err(not_an_input("kicks")),
            ),
            (
                "a field that is not pub",
                "Inputs",
                private.as_str(),
                Err(
                    "it: `type Inputs = Inputs` needs `pub struct Inputs { pub <input>: <its data>, .. }`, not \
                     `grid : Option < Grid >`"
                        .to_owned(),
                ),
            ),
            (
                "data no input holds",
                "Vec<Signal>",
                fields,
                Err(
                    "it: a source's `type Inputs` is (), one input's data (Vec<Event>, Vec<Keyframe>, Vec<f64>, \
                     Grid, Series), an Option of one, or `Inputs`, the file's struct of them; not `Vec<Signal>`"
                        .to_owned(),
                ),
            ),
        ];
        for (row, inputs, params, expected) in rows {
            assert_eq!(lib(inputs, params), expected, "{row}");
        }
        // a shaper or an operation is no `lib` function, whatever its inputs
        for kind in ["Shaper", "Operation"] {
            let source = format!("pub struct Params {{}}\nimpl {kind} for Params {{\n    type Inputs = Inputs;\n}}\n");
            assert_eq!(
                table(&[file("it", &source)]).map(|code| code.lib),
                Ok(String::new()),
                "{kind}"
            );
        }
    }

    #[test]
    fn a_lib_functions_doc_names_only_the_parameters_and_inputs_it_takes() {
        let fields = "pub struct Params {\n    pub level: f64,\n}";
        let doc = |inputs: &str, params: &str| {
            let source = format!("{params}\nimpl Source for Params {{\n    type Inputs = {inputs};\n}}\n");
            let code = table(&[file("it", &source)]).map_or_else(|error| error, |code| code.lib);
            let lines: Vec<&str> = code.lines().filter_map(|line| line.strip_prefix("/// ")).collect();
            lines.join(" ")
        };
        let (made, params, input) = ("made through the one door", "[`Error::Params`]", "[`Error::Input`]");
        // (row, `type Inputs`, the parameters, what it is the signal of, whether the errors name parameters, inputs)
        let rows = [
            ("parameters only", "()", fields, "of its parameters,", true, false),
            (
                "inputs and parameters",
                "Vec<f64>",
                fields,
                "of its inputs (borrowed, cloned once) and parameters,",
                true,
                true,
            ),
            (
                "inputs only",
                "Vec<Keyframe>",
                "pub struct Params;",
                "of its inputs (borrowed, cloned once),",
                false,
                true,
            ),
            ("neither", "()", "pub struct Params;", "of nothing,", false, false),
        ];
        for (row, inputs, struct_params, of, names_params, names_inputs) in rows {
            let doc = doc(inputs, struct_params);
            assert!(doc.contains(&format!("{of} {made}")), "{row}: {doc}");
            assert_eq!(
                (doc.contains(params), doc.contains(input)),
                (names_params, names_inputs),
                "{row}: {doc}"
            );
            assert!(doc.contains("whatever error the source gives."), "{row}: {doc}");
        }
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
        let bare = built(&[file(
            "bare",
            "pub struct Params {}\nimpl Source for Params { type Inputs = (); }\n",
        )]);
        assert!(
            bare.builtin
                .contains("doc: \"\".to_owned(),\n        params_title: \"BareParams\""),
            "{}",
            bare.builtin
        );
        assert!(bare.builtin.contains("params_doc: \"\".to_owned()"), "{}", bare.builtin);
    }
}
