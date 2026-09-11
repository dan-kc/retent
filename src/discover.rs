use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

pub(crate) struct VaultFiles {
    pub(crate) inheritance_root: PathBuf,
    pub(crate) markdown: Vec<PathBuf>,
    pub(crate) metadata: Vec<PathBuf>,
}

pub(crate) fn vault_files(root: &Path) -> Result<VaultFiles, String> {
    let inheritance_root = inheritance_root(root)?;
    let mut markdown = Vec::new();
    let mut metadata = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(not_git)
    {
        let entry = entry.map_err(|error| format!("{}: {error}", root.display()))?;
        if entry.file_type().is_file() && has_markdown_extension(entry.path()) {
            markdown.push(entry.into_path());
        } else if entry.file_type().is_file() && is_metadata_file(&entry) {
            metadata.push(entry.into_path());
        }
    }

    for directory in root
        .ancestors()
        .skip(1)
        .take_while(|directory| directory.starts_with(&inheritance_root))
    {
        let path = directory.join(".defaults.yaml");
        match fs::symlink_metadata(&path) {
            Ok(file_metadata) if file_metadata.file_type().is_file() => metadata.push(path),
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("cannot inspect {}: {error}", path.display()));
            }
        }
    }

    markdown.sort_by(|left, right| {
        let left = left.strip_prefix(root).unwrap_or(left);
        let right = right.strip_prefix(root).unwrap_or(right);
        left.as_os_str().cmp(right.as_os_str())
    });
    metadata.sort();
    metadata.dedup();

    Ok(VaultFiles {
        inheritance_root,
        markdown,
        metadata,
    })
}

fn not_git(entry: &DirEntry) -> bool {
    entry.file_name() != ".git"
}

fn has_markdown_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn is_metadata_file(entry: &DirEntry) -> bool {
    entry.file_name() == ".defaults.yaml"
}

fn inheritance_root(root: &Path) -> Result<PathBuf, String> {
    for directory in root.ancestors() {
        let marker = directory.join(".git");
        match fs::symlink_metadata(&marker) {
            Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_dir() => {
                return Ok(directory.to_path_buf());
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("cannot inspect {}: {error}", marker.display()));
            }
        }
    }

    Ok(root.to_path_buf())
}
