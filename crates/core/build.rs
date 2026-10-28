//! Writes the table of built-in shapers the core compiles in: one module per file in `src/builtin/`, and every shaper by
//! name with the function that runs it. Adding a shaper adds its file and edits nothing else.

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::{env, fs, io};

fn main() -> io::Result<()> {
    let manifest =
        env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| io::Error::other("cargo sets CARGO_MANIFEST_DIR"))?;
    let out = env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("cargo sets OUT_DIR"))?;
    let folder = Path::new(&manifest).join("src").join("builtin");
    println!("cargo::rerun-if-changed={}", folder.display());
    let code = table(&builtin_files(&folder)?).map_err(io::Error::other)?;
    fs::write(PathBuf::from(out).join("builtin.rs"), code)
}

/// The built-in files in `folder` (each `<name>.rs`), sorted by name; none if the folder does not exist.
fn builtin_files(folder: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    if !folder.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "rs")
            && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
        {
            files.push((name.to_owned(), path.clone()));
        }
    }
    files.sort();
    Ok(files)
}

/// The Rust source of the table: a module per built-in file, and `SHAPERS`, each name with the function that runs it
/// (it reads the shaper's `Params` from the parameters and calls its `run`).
fn table(files: &[(String, PathBuf)]) -> Result<String, fmt::Error> {
    let mut code = String::new();
    for (name, path) in files {
        writeln!(code, "#[path = {:?}]\nmod {name};", path.display().to_string())?;
    }
    writeln!(
        code,
        "/// Every shaper the core implements, sorted by name, with the function that runs it."
    )?;
    writeln!(code, "pub(crate) static SHAPERS: &[(&str, Run)] = &[")?;
    for (name, _) in files {
        writeln!(code, "    ({name:?}, |inputs, params| {{")?;
        writeln!(
            code,
            "        let params = {name}::Params::deserialize(params).map_err(|error| Error::Params {{"
        )?;
        writeln!(code, "            shaper: {name:?}.to_owned(),")?;
        writeln!(code, "            message: error.to_string(),")?;
        writeln!(code, "        }})?;")?;
        writeln!(code, "        Ok({name}::run(inputs, &params))")?;
        writeln!(code, "    }}),")?;
    }
    writeln!(code, "];")?;
    Ok(code)
}
