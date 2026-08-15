//! Shared vault listing used by `list` and scheduled queue views.

use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostics::Diagnostic;
use crate::discover::{markdown_files, relative};
use crate::document::{Classification, ParsedDocument, parse};
use crate::filter::Filter;

/// One parsed Markdown entry and its root-relative path.
#[derive(Debug)]
pub struct ListEntry {
    pub path: PathBuf,
    pub document: ParsedDocument,
}

/// Matching entries, invalid documents, and invalid UTF-8 diagnostics.
#[derive(Debug)]
pub struct ListResult {
    pub entries: Vec<ListEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

/// List root entries in path order. Invalid documents bypass the filter.
pub fn scan(root: &Path, filter: Option<&Filter>) -> Result<ListResult, String> {
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    for path in markdown_files(root)? {
        let relative_path = relative(root, &path);
        let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            Err(_) => {
                diagnostics.push(Diagnostic::new(
                    relative_path,
                    None,
                    "utf8-invalid",
                    "file is not valid UTF-8",
                ));
                continue;
            }
        };
        let document = parse(&relative_path, source);
        if document.classification() == Classification::Invalid
            || filter.is_none_or(|filter| filter.matches(&document.metadata))
        {
            entries.push(ListEntry {
                path: relative_path,
                document,
            });
        }
    }
    Ok(ListResult {
        entries,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn lists_all_entries_by_default_and_filters_metadata_when_requested() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("plain.md"), "plain\n").unwrap();
        fs::write(
            directory.path().join("tagged.md"),
            "---\npriority: 5\ntags: [foo]\n---\n",
        )
        .unwrap();

        let all = scan(directory.path(), None).unwrap();
        assert_eq!(all.entries.len(), 2);

        let filter = "priority >= 5 and tags.any(foo)".parse().unwrap();
        let filtered = scan(directory.path(), Some(&filter)).unwrap();
        assert_eq!(filtered.entries.len(), 1);
        assert_eq!(filtered.entries[0].path, Path::new("tagged.md"));
    }

    #[test]
    fn records_invalid_utf8_without_hiding_other_entries() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("valid.md"), "valid\n").unwrap();
        fs::write(directory.path().join("invalid.md"), [0xff]).unwrap();

        let result = scan(directory.path(), None).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "utf8-invalid");
    }
}
