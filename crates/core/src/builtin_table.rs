//! The Rust source of the built-in table, as a function of the built-in files: the build script writes it, and the
//! crate's tests (which include this file) check it on made-up files.

use std::fmt::{self, Write as _};

/// One file of `src/builtin/`.
pub(crate) struct File<'a> {
    /// The built-in's name: the file's name without `.rs`.
    pub(crate) name: &'a str,
    /// Where the file is, as the generated `mod` declaration names it.
    pub(crate) path: &'a str,
    /// What the file says.
    pub(crate) source: &'a str,
}

/// The text that marks a built-in file as a shaper the core runs. A textual check on purpose: until the sources and
/// operations move to Rust, a file that only declares parameters has no `shape`, and this is the whole convention (it
/// goes when every built-in runs in Rust).
const SHAPE: &str = "pub(crate) fn shape(";

/// The Rust source of the table of `files` (sorted by name): a public module for each file; `PARAMS`, every built-in's
/// parameter table by name; and `SHAPERS`, each file that defines [`SHAPE`] by name with the function that parses its
/// parameters through the table and shapes a signal with them.
pub(crate) fn table(files: &[File<'_>]) -> Result<String, fmt::Error> {
    let mut code = String::new();
    for file in files {
        writeln!(code, "#[path = {:?}]\npub mod {};", file.path, file.name)?;
    }
    writeln!(
        code,
        "/// Every built-in by name, sorted, with the function that gives its parameter table."
    )?;
    writeln!(code, "pub(crate) static PARAMS: &[(&str, Table)] = &[")?;
    for File { name, .. } in files {
        writeln!(code, "    ({name:?}, <{name}::Params as crate::Params>::table),")?;
    }
    writeln!(code, "];")?;
    writeln!(
        code,
        "/// Every shaper the core implements, sorted by name, with the function that shapes a signal with it."
    )?;
    writeln!(code, "pub(crate) static SHAPERS: &[(&str, Shape)] = &[")?;
    for File { name, .. } in files.iter().filter(|file| file.source.contains(SHAPE)) {
        writeln!(code, "    ({name:?}, |input, values| {{")?;
        writeln!(
            code,
            "        let params = {name}::Params::parse(values).map_err(|error| Error::params({name:?}, error))?;"
        )?;
        writeln!(code, "        {name}::shape(input, &params)")?;
        writeln!(code, "    }}),")?;
    }
    writeln!(code, "];")?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::{File, table};

    fn built(files: &[File<'_>]) -> String {
        table(files).unwrap_or_default()
    }

    #[test]
    fn a_file_with_a_shape_function_is_a_shaper_and_one_without_is_not() {
        let code = built(&[
            File {
                name: "plain",
                path: "plain.rs",
                source: "pub struct Params {}\n",
            },
            File {
                name: "recurs",
                path: "recurs.rs",
                source: "pub struct Params {}\npub(crate) fn run(inputs: &[f64]) {}\n",
            },
            File {
                name: "shapes",
                path: "shapes.rs",
                source: "pub struct Params {}\npub(crate) fn shape(input: &Signal, params: &Params) {}\n",
            },
        ]);
        let shapers = code.split("static SHAPERS").nth(1).unwrap_or_default();
        let shapers = shapers.split("static PARAMS").next().unwrap_or_default();
        assert!(
            shapers.contains("(\"shapes\"") && shapers.contains("shapes::shape(input, &params)"),
            "{code}"
        );
        assert!(
            !shapers.contains("\"plain\"") && !shapers.contains("\"recurs\""),
            "{code}"
        );
        let params = code.split("static PARAMS").nth(1).unwrap_or_default();
        for name in ["plain", "recurs", "shapes"] {
            assert!(
                params.contains(&format!("({name:?}, <{name}::Params")),
                "{name}: {code}"
            );
        }
    }
}
