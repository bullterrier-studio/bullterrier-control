//! The built-ins, one module per file in `src/builtin/`, and their tables; the build script writes both.
//!
//! A built-in file declares `Params`, its parameters: a struct with `#[derive(Params)]`, `pub` fields, each with a doc
//! comment (its description) and `#[param(..)]` (its default, unit and range), and a `#[derive(Choice)]` enum for each
//! field that picks among names. That declaration is the only one: the table, the defaults, the description and the
//! checks all come from it. A rule between fields (`high` is not below `low`) is a `fn rule(&Params) -> Vec<Problem>`
//! named by `#[param(rule = rule)]` on the struct; the table runs it once every field is valid.
//!
//! The file also implements one of [`Source`](crate::Source), [`Shaper`](crate::Shaper) and
//! [`Operation`](crate::Operation) for its `Params` (`impl Shaper for Params`, its `NAME` the file's name, which the
//! build checks; a file without one does not build): it is in [`Registry::builtin`](crate::Registry::builtin) under
//! that name, with the file's leading `//!` lines as its doc (what `help()` shows) and the doc of `Params` as its
//! parameters'; a source is also a function of [`lib`](crate::lib). A shaper may also be a method of
//! [`Signal`](crate::Signal) taking its typed
//! `Params`: [`Signal::pipe`](crate::Signal::pipe) of them, so it is checked, named and recorded as every door makes
//! it. A shaper that needs history builds its signal as a [`Signal::rendered`](crate::Signal::rendered) over its
//! private causal recurrence `run` (`slew` is the model).
//! Built-ins never refer to one another: what two of them share goes in a module of the crate named for its concept.

include!(concat!(env!("OUT_DIR"), "/builtin.rs"));

/// Whether a thing's `NAME` is its file's name: what the generated registration asserts at compile time.
const fn named(name: &str, file: &str) -> bool {
    let (name, file) = (name.as_bytes(), file.as_bytes());
    if name.len() != file.len() {
        return false;
    }
    let mut at = 0;
    while at < name.len() {
        if name[at] != file[at] {
            return false;
        }
        at += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::Registry;

    /// The names of the built-in files.
    fn files() -> Result<Vec<String>, std::io::Error> {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("builtin");
        let mut names = Vec::new();
        for entry in std::fs::read_dir(folder)? {
            let path = entry?.path();
            if path.extension().is_some_and(|extension| extension == "rs")
                && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
            {
                names.push(name.to_owned());
            }
        }
        Ok(names)
    }

    /// What does not match between the built-in `files` and the things `registered`: a file no thing is registered
    /// for, and a thing with no file.
    fn unmatched(files: &[String], registered: &[&str]) -> Vec<String> {
        let unregistered = files
            .iter()
            .filter(|file| !registered.contains(&file.as_str()))
            .map(|file| format!("the file {file}.rs is not registered"));
        let fileless = registered
            .iter()
            .filter(|name| !files.iter().any(|file| file == *name))
            .map(|name| format!("{name} is registered without a file"));
        unregistered.chain(fileless).collect()
    }

    #[test]
    fn a_thing_is_registered_for_each_built_in_file_and_for_nothing_else() -> Result<(), Box<dyn std::error::Error>> {
        let mut registry = Registry::new();
        super::register(&mut registry)?;
        let registered: Vec<&str> = registry.things().map(crate::registry::Thing::name).collect();
        let files = files()?;
        assert!(files.iter().any(|file| file == "constant"), "{files:?}");
        assert_eq!(unmatched(&files, &registered), Vec::<String>::new());
        // the core's registry is that one
        let builtin: Vec<&str> = Registry::builtin().things().map(crate::registry::Thing::name).collect();
        assert_eq!(builtin, registered);
        Ok(())
    }

    #[test]
    fn the_match_of_files_and_things_names_a_file_not_registered_and_a_thing_without_a_file() {
        let files = ["clip".to_owned(), "slew".to_owned()];
        // (row, the things registered, what does not match)
        let rows: [(&str, &[&str], &[&str]); 4] = [
            ("the same", &["clip", "slew"], &[]),
            (
                "a file not registered",
                &["clip"],
                &["the file slew.rs is not registered"],
            ),
            (
                "a thing without a file",
                &["clip", "drift", "slew"],
                &["drift is registered without a file"],
            ),
            (
                "both",
                &["clip", "drift"],
                &[
                    "the file slew.rs is not registered",
                    "drift is registered without a file",
                ],
            ),
        ];
        for (row, registered, expected) in rows {
            assert_eq!(unmatched(&files, registered), expected, "{row}");
        }
    }
}
