//! Discover and validate a pack's source files.

use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

/// A source file and its inferred destination, relative to its pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Source {
    /// Relative source path.
    pub source: PathBuf,
    /// Relative inferred output path.
    pub output: PathBuf,
    /// Whether the source is a Jinja template.
    pub render: bool,
}

/// Failure to discover a safe pack source.
#[derive(Debug)]
pub struct SourceError(String);
impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for SourceError {}

/// Recursively discovers regular files, excluding the root pack manifest.
///
/// # Errors
/// Returns an error for unreadable entries, symlinks, special files, or unsafe paths.
pub fn discover(root: &Path) -> Result<Vec<Source>, SourceError> {
    let mut result = Vec::new();
    visit(root, Path::new(""), &mut result)?;
    result.sort_by(|a, b| a.source.cmp(&b.source));
    Ok(result)
}

fn visit(root: &Path, relative: &Path, result: &mut Vec<Source>) -> Result<(), SourceError> {
    let directory = root.join(relative);
    for entry in fs::read_dir(&directory)
        .map_err(|e| SourceError(format!("{}: {e}", directory.display())))?
    {
        let entry = entry.map_err(|e| SourceError(format!("{}: {e}", directory.display())))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| SourceError(format!("{}: non-UTF-8 name", entry.path().display())))?;
        if !safe_component(name) {
            return Err(SourceError(format!(
                "{}: invalid path component",
                entry.path().display()
            )));
        }

        let path = relative.join(name);
        let kind = entry
            .file_type()
            .map_err(|e| SourceError(format!("{}: {e}", entry.path().display())))?;
        if kind.is_dir() {
            visit(root, &path, result)?;
        } else if kind.is_file() {
            if path == Path::new("template.toml") {
                continue;
            }

            let render = name.ends_with(".j2");
            let output_name = name.strip_suffix(".j2").unwrap_or(name);
            if !safe_component(output_name) {
                return Err(SourceError(format!(
                    "{}: invalid output name",
                    entry.path().display()
                )));
            }
            result.push(Source {
                output: path.with_file_name(output_name),
                source: path,
                render,
            });
        } else {
            return Err(SourceError(format!(
                "{}: unsupported file type",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

pub(crate) fn safe_component(name: &str) -> bool {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "\\<>:\"|?*".contains(c))
    {
        return false;
    }

    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return false;
    }
    !stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
        .is_some_and(|n| {
            matches!(
                n,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::safe_component;

    #[test]
    fn rejects_windows_reserved_superscript_device_names() {
        for name in ["COM¹", "COM².txt", "COM³.j2", "LPT¹.txt", "LPT²", "LPT³.j2"] {
            assert!(!safe_component(name), "{name} must be rejected");
        }
    }
}
