//! What the architecture tests read: the crate's own source, as code (they read source text, which a behaviour test
//! never does).

use std::fs;
use std::path::{Path, PathBuf};

/// `text` without its comments and the insides of its string and character literals (each left as `""` or `''`),
/// so only code is searched.
pub(crate) fn code(text: &str) -> String {
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

/// Every `.rs` file under `folder`, at any depth.
pub(crate) fn rust_files(folder: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut folders = vec![folder.to_path_buf()];
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
