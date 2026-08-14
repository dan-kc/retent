//! Deterministic vault discovery without following symbolic links.

use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

/// Return regular Markdown files below `root`, sorted by relative path.
pub fn markdown_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.is_dir() {
        return Err(format!("{}: root is not a directory", root.display()));
    }

    let mut paths = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(not_git)
    {
        let entry = entry.map_err(|error| format!("{}: {error}", root.display()))?;
        if entry.file_type().is_file() && is_markdown(entry.path()) {
            paths.push(entry.path().to_path_buf());
        }
    }
    paths.sort_by(|left, right| {
        relative(root, left)
            .as_os_str()
            .cmp(relative(root, right).as_os_str())
    });
    Ok(paths)
}

/// Whether a path has a case-insensitive `.md` extension.
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Produce a display path relative to the scan root.
pub fn relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn not_git(entry: &DirEntry) -> bool {
    entry.file_name() != ".git"
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn discovers_markdown_in_path_order() {
        let directory = tempdir().unwrap();
        fs::create_dir(directory.path().join("z")).unwrap();
        fs::write(directory.path().join("z/b.MD"), "").unwrap();
        fs::write(directory.path().join("a.md"), "").unwrap();
        fs::write(directory.path().join("ignored.txt"), "").unwrap();

        let found = markdown_files(directory.path()).unwrap();
        let relative: Vec<_> = found
            .iter()
            .map(|path| path.strip_prefix(directory.path()).unwrap())
            .collect();
        assert_eq!(relative, [Path::new("a.md"), Path::new("z/b.MD")]);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        fs::write(directory.path().join("real.md"), "").unwrap();
        symlink(
            directory.path().join("real.md"),
            directory.path().join("link.md"),
        )
        .unwrap();
        assert_eq!(markdown_files(directory.path()).unwrap().len(), 1);
    }
}
