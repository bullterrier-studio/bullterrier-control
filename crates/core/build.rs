//! Writes the table of built-ins the core compiles in (see `src/builtin_table.rs`): one module per file in
//! `src/builtin/`, the registration of each as a thing the core runs, and a `lib` function for each source. Adding a
//! built-in adds its file and edits nothing else.

#[path = "src/builtin_table.rs"]
mod table;

use table::File;

use std::path::{Path, PathBuf};
use std::{env, fs, io};

fn main() -> io::Result<()> {
    let manifest =
        env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| io::Error::other("cargo sets CARGO_MANIFEST_DIR"))?;
    let out = env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("cargo sets OUT_DIR"))?;
    let folder = Path::new(&manifest).join("src").join("builtin");
    println!("cargo::rerun-if-changed={}", folder.display());
    let read = builtin_files(&folder)?;
    let files: Vec<File<'_>> = read
        .iter()
        .map(|(name, path, source)| File { name, path, source })
        .collect();
    let code = table::table(&files).map_err(io::Error::other)?;
    let out = PathBuf::from(out);
    fs::write(out.join("builtin.rs"), code.builtin)?;
    fs::write(out.join("lib.rs"), code.lib)
}

/// The built-in files in `folder` (each `<name>.rs`) as (name, path, source), sorted by name; none if the folder does
/// not exist.
fn builtin_files(folder: &Path) -> io::Result<Vec<(String, String, String)>> {
    if !folder.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "rs")
            && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
        {
            files.push((name.to_owned(), path.display().to_string(), fs::read_to_string(&path)?));
        }
    }
    files.sort();
    Ok(files)
}
