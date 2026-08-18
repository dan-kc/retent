use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

pub(crate) fn markdown_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(not_git)
    {
        let entry = entry.map_err(|error| format!("{}: {error}", root.display()))?;
        if entry.file_type().is_file() && has_markdown_extension(entry.path()) {
            paths.push(entry.into_path());
        }
    }

    paths.sort_by(|left, right| {
        let left = left.strip_prefix(root).unwrap_or(left);
        let right = right.strip_prefix(root).unwrap_or(right);
        left.as_os_str().cmp(right.as_os_str())
    });
    Ok(paths)
}

fn not_git(entry: &DirEntry) -> bool {
    entry.file_name() != ".git"
}

fn has_markdown_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}
