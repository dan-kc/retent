//! Bulk metadata updates for an explicit list of paths.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_yaml_ng::{Mapping, Value};

use crate::document::{Classification, ParsedDocument, parse};
/// Set the priority of every selected valid document.
pub fn priority(root: &Path, paths: &[PathBuf], priority: u8) -> Result<usize, String> {
    update_selected(root, paths, |document| {
        if document.metadata.priority == Some(priority) {
            return Ok(document.source.clone());
        }
        let candidate = priority_candidate(document, priority)?;
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
    mutate_tags(root, paths, |document| {
        let mut updated = if overwrite {
            Vec::new()
        } else {
            deduplicate(document.metadata.tags.iter().cloned())
        };
        let mut present: HashSet<_> = updated.iter().cloned().collect();
        for tag in &requested {
            if present.insert(tag.clone()) {
                updated.push(tag.clone());
            }
        }
        updated
    })
}

/// Rename a tag on every selected valid document.
pub fn tags_rename(root: &Path, paths: &[PathBuf], from: &str, to: &str) -> Result<usize, String> {
    mutate_tags(root, paths, |document| {
        deduplicate(document.metadata.tags.iter().map(|tag| {
            if tag == from {
                to.to_owned()
            } else {
                tag.clone()
            }
        }))
    })
}

/// Remove tags from every selected valid document.
pub fn tags_remove(root: &Path, paths: &[PathBuf], tags: &[String]) -> Result<usize, String> {
    let removed: HashSet<_> = tags.iter().map(String::as_str).collect();
    mutate_tags(root, paths, |document| {
        deduplicate(
            document
                .metadata
                .tags
                .iter()
                .filter(|tag| !removed.contains(tag.as_str()))
                .cloned(),
        )
    })
}

fn mutate_tags(
    root: &Path,
    paths: &[PathBuf],
    mutate: impl Fn(&ParsedDocument) -> Vec<String>,
) -> Result<usize, String> {
    update_selected(root, paths, |document| {
        let updated = mutate(document);
        if updated == document.metadata.tags {
            return Ok(document.source.clone());
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
        if candidate != document.source {
            candidates.push((path, candidate));
        }
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

fn priority_candidate(document: &ParsedDocument, priority: u8) -> Result<String, String> {
    let source = &document.source;
    let (yaml_start, yaml_end) = frontmatter_bounds(document)?;
    let yaml = &source[yaml_start..yaml_end];
    let value_span = top_level_scalar_span(yaml, "priority").ok_or_else(|| {
        format!(
            "{}: cannot locate top-level priority scalar; no changes made",
            document.path.display()
        )
    })?;

    let mut candidate = String::with_capacity(source.len() + 1);
    candidate.push_str(&source[..yaml_start + value_span.start]);
    candidate.push_str(&priority.to_string());
    candidate.push_str(&source[yaml_start + value_span.end..]);
    Ok(candidate)
}

fn frontmatter_bounds(document: &ParsedDocument) -> Result<(usize, usize), String> {
    let source = &document.source;
    let without_bom = source.strip_prefix('\u{feff}').unwrap_or(source);
    let bom_len = source.len() - without_bom.len();
    let yaml_start = without_bom
        .find('\n')
        .map(|index| bom_len + index + 1)
        .ok_or_else(|| format!("{}: front matter has no body", document.path.display()))?;

    let mut yaml_end = None;
    let mut offset = yaml_start;
    for line in source[yaml_start..].split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            yaml_end = Some(offset);
            break;
        }
        offset += line.len();
    }
    let yaml_end =
        yaml_end.ok_or_else(|| format!("{}: front matter is unclosed", document.path.display()))?;
    Ok((yaml_start, yaml_end))
}

fn top_level_scalar_span(yaml: &str, key: &str) -> Option<std::ops::Range<usize>> {
    let mut line_start = 0;
    for line_with_ending in yaml.split_inclusive('\n') {
        let line = line_with_ending.trim_end_matches(['\r', '\n']);
        let Some(after_key) = line.strip_prefix(key) else {
            line_start += line_with_ending.len();
            continue;
        };
        let whitespace = after_key.len() - after_key.trim_start_matches([' ', '\t']).len();
        let after_whitespace = &after_key[whitespace..];
        let Some(after_colon) = after_whitespace.strip_prefix(':') else {
            line_start += line_with_ending.len();
            continue;
        };
        let value_whitespace =
            after_colon.len() - after_colon.trim_start_matches([' ', '\t']).len();
        let value = &after_colon[value_whitespace..];
        let value_len = value
            .find(|character: char| character.is_whitespace() || character == '#')
            .unwrap_or(value.len());
        if value_len == 0 {
            return None;
        }
        let value_start = line_start + key.len() + whitespace + 1 + value_whitespace;
        return Some(value_start..value_start + value_len);
    }
    None
}

fn metadata_candidate(
    document: &ParsedDocument,
    mutate: impl FnOnce(&mut Mapping),
) -> Result<String, String> {
    let source = &document.source;
    let (yaml_start, yaml_end) = frontmatter_bounds(document)?;
    let yaml = &source[yaml_start..yaml_end];
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
    let rendered = if source[..yaml_start].ends_with("\r\n") {
        rendered.replace('\n', "\r\n")
    } else {
        rendered
    };

    let mut candidate = String::with_capacity(source.len() + rendered.len());
    candidate.push_str(&source[..yaml_start]);
    candidate.push_str(&rendered);
    candidate.push_str(&source[yaml_end..]);
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
    let mut seen = HashSet::new();
    for tag in tags {
        if seen.insert(tag.clone()) {
            unique.push(tag);
        }
    }
    unique
}
