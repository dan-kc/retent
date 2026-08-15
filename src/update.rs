//! Bulk metadata updates for an explicit list of paths.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_yaml_ng::{Mapping, Value};

use crate::document::{Classification, ParsedDocument, parse};
/// Set the priority of every selected valid document.
pub fn priority(root: &Path, paths: &[PathBuf], priority: u8) -> Result<usize, String> {
    update_selected(root, paths, |document| {
        let candidate = metadata_candidate(document, |mapping| {
            mapping.insert(key("priority"), Value::Number(priority.into()));
        })?;
        validate_candidate(document, &candidate, |updated| {
            updated.metadata.priority == Some(priority)
        })?;
        Ok(candidate)
    })
}

/// Add tags to every selected valid document.
///
/// When `overwrite` is true, existing tags are discarded. In either mode,
/// duplicate tags are removed while retaining first-seen order.
pub fn tags_add(
    root: &Path,
    paths: &[PathBuf],
    tags: &[String],
    overwrite: bool,
) -> Result<usize, String> {
    let requested = deduplicate(tags.iter().cloned());
    update_selected(root, paths, |document| {
        let mut updated = if overwrite {
            Vec::new()
        } else {
            deduplicate(document.metadata.tags.iter().cloned())
        };
        for tag in &requested {
            if !updated.contains(tag) {
                updated.push(tag.clone());
            }
        }
        let candidate = metadata_candidate(document, |mapping| {
            mapping.insert(key("tags"), tags_value(&updated));
        })?;
        validate_candidate(document, &candidate, |document| {
            document.metadata.tags == updated
        })?;
        Ok(candidate)
    })
}

/// Rename a tag on every selected valid document.
pub fn tags_rename(root: &Path, paths: &[PathBuf], from: &str, to: &str) -> Result<usize, String> {
    update_selected(root, paths, |document| {
        let updated = deduplicate(document.metadata.tags.iter().map(|tag| {
            if tag == from {
                to.to_owned()
            } else {
                tag.clone()
            }
        }));
        let candidate = metadata_candidate(document, |mapping| {
            mapping.insert(key("tags"), tags_value(&updated));
        })?;
        validate_candidate(document, &candidate, |document| {
            document.metadata.tags == updated
        })?;
        Ok(candidate)
    })
}

/// Remove tags from every selected valid document.
pub fn tags_remove(root: &Path, paths: &[PathBuf], tags: &[String]) -> Result<usize, String> {
    update_selected(root, paths, |document| {
        let updated = deduplicate(
            document
                .metadata
                .tags
                .iter()
                .filter(|tag| !tags.contains(tag))
                .cloned(),
        );
        let candidate = metadata_candidate(document, |mapping| {
            mapping.insert(key("tags"), tags_value(&updated));
        })?;
        validate_candidate(document, &candidate, |document| {
            document.metadata.tags == updated
        })?;
        Ok(candidate)
    })
}

fn update_selected(
    root: &Path,
    paths: &[PathBuf],
    candidate_for: impl Fn(&ParsedDocument) -> Result<String, String>,
) -> Result<usize, String> {
    let root = fs::canonicalize(root).map_err(|error| format!("{}: {error}", root.display()))?;
    if !root.is_dir() {
        return Err(format!("{}: root is not a directory", root.display()));
    }

    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();

    for supplied in paths {
        let path = resolve_selected_path(&root, supplied)?;
        if !seen.insert(path.clone()) {
            continue;
        }
        let document = crate::document::read(&path)?;
        match document.classification() {
            Classification::Invalid => {
                let diagnostic = document
                    .diagnostics
                    .first()
                    .expect("invalid document has a diagnostic");
                return Err(format!("{diagnostic}; no changes made"));
            }
            Classification::Missing => {
                return Err(format!(
                    "{}: missing {}; no changes made",
                    path.display(),
                    document.missing.join(", ")
                ));
            }
            Classification::Valid => {}
        }
        let candidate = candidate_for(&document)?;
        candidates.push((path, candidate));
    }

    let count = candidates.len();
    for (path, candidate) in candidates {
        crate::document::atomic_replace(&path, candidate.as_bytes())?;
    }
    Ok(count)
}

fn resolve_selected_path(root: &Path, supplied: &Path) -> Result<PathBuf, String> {
    let joined = if supplied.is_absolute() {
        supplied.to_path_buf()
    } else {
        root.join(supplied)
    };
    let metadata = fs::symlink_metadata(&joined)
        .map_err(|error| format!("{}: {error}; no changes made", joined.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{}: symbolic links cannot be edited; no changes made",
            joined.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "{}: expected a regular file; no changes made",
            joined.display()
        ));
    }
    let resolved = fs::canonicalize(&joined)
        .map_err(|error| format!("{}: {error}; no changes made", joined.display()))?;
    if !resolved.starts_with(root) {
        return Err(format!(
            "{}: path is outside root {}; no changes made",
            supplied.display(),
            root.display()
        ));
    }
    if !crate::discover::is_markdown(&resolved) {
        return Err(format!(
            "{}: expected a .md file; no changes made",
            supplied.display()
        ));
    }
    Ok(resolved)
}

fn metadata_candidate(
    document: &ParsedDocument,
    mutate: impl FnOnce(&mut Mapping),
) -> Result<String, String> {
    let source = &document.source;
    let without_bom = source.strip_prefix('\u{feff}').unwrap_or(source);
    let bom_len = source.len() - without_bom.len();
    let opening_end = without_bom
        .find('\n')
        .map(|index| bom_len + index + 1)
        .ok_or_else(|| format!("{}: front matter has no body", document.path.display()))?;

    let mut closing_start = None;
    let mut offset = opening_end;
    for line in source[opening_end..].split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            closing_start = Some(offset);
            break;
        }
        offset += line.len();
    }
    let closing_start = closing_start
        .ok_or_else(|| format!("{}: front matter is unclosed", document.path.display()))?;
    let yaml = &source[opening_end..closing_start];
    let mut mapping: Mapping = serde_yaml_ng::from_str(yaml).map_err(|error| {
        format!(
            "{}: cannot parse front matter: {error}",
            document.path.display()
        )
    })?;
    mutate(&mut mapping);
    let rendered = serde_yaml_ng::to_string(&mapping).map_err(|error| {
        format!(
            "{}: cannot render front matter: {error}",
            document.path.display()
        )
    })?;

    let mut candidate = String::with_capacity(source.len() + rendered.len());
    candidate.push_str(&source[..opening_end]);
    candidate.push_str(&rendered);
    candidate.push_str(&source[closing_start..]);
    Ok(candidate)
}

fn validate_candidate(
    original: &ParsedDocument,
    candidate: &str,
    matches: impl FnOnce(&ParsedDocument) -> bool,
) -> Result<(), String> {
    let reparsed = parse(&original.path, candidate);
    if reparsed.classification() != Classification::Valid || !matches(&reparsed) {
        return Err(format!(
            "{}: updated front matter failed validation; no changes made",
            original.path.display()
        ));
    }
    Ok(())
}

fn key(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn tags_value(tags: &[String]) -> Value {
    Value::Sequence(tags.iter().cloned().map(Value::String).collect())
}

fn deduplicate(tags: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut unique = Vec::new();
    for tag in tags {
        if !unique.contains(&tag) {
            unique.push(tag);
        }
    }
    unique
}
