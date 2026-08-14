//! Strict scheduler-field parsing from leading YAML front matter.

use std::path::Path;

use serde_yaml_ng::{Mapping, Value};

use super::{ElementType, Metadata, trim_line_ending};
use crate::diagnostics::Diagnostic;

pub(super) struct FrontMatterResult {
    pub metadata: Metadata,
    pub diagnostics: Vec<Diagnostic>,
}

pub(super) fn parse(path: &Path, source: &str) -> FrontMatterResult {
    let without_bom = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut lines = without_bom.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return empty();
    };
    if trim_line_ending(first) != "---" {
        return empty();
    }

    let mut yaml = String::new();
    let mut closed = false;
    for line in lines {
        if trim_line_ending(line) == "---" {
            closed = true;
            break;
        }
        yaml.push_str(line);
    }
    if !closed {
        return FrontMatterResult {
            metadata: Metadata::default(),
            diagnostics: vec![Diagnostic::new(
                path,
                Some(1),
                "frontmatter-unclosed",
                "opening YAML delimiter has no closing delimiter",
            )],
        };
    }

    if yaml
        .lines()
        .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
    {
        return empty();
    }

    let value: Value = match serde_yaml_ng::from_str(&yaml) {
        Ok(value) => value,
        Err(error) => {
            let line = error.location().map(|location| location.line() + 1);
            return FrontMatterResult {
                metadata: Metadata::default(),
                diagnostics: vec![Diagnostic::new(
                    path,
                    line,
                    "frontmatter-yaml",
                    error.to_string(),
                )],
            };
        }
    };
    let Value::Mapping(mapping) = value else {
        return FrontMatterResult {
            metadata: Metadata::default(),
            diagnostics: vec![Diagnostic::new(
                path,
                Some(2),
                "frontmatter-not-mapping",
                "YAML front matter must be a mapping",
            )],
        };
    };
    let type_line = key_line(&yaml, "type");
    let priority_line = key_line(&yaml, "priority");
    let tags_line = key_line(&yaml, "tags");
    parse_mapping(path, &mapping, type_line, priority_line, tags_line)
}

fn parse_mapping(
    path: &Path,
    mapping: &Mapping,
    type_line: Option<usize>,
    priority_line: Option<usize>,
    tags_line: Option<usize>,
) -> FrontMatterResult {
    let type_value = mapping.get(Value::String("type".to_owned()));
    let priority_value = mapping.get(Value::String("priority".to_owned()));
    let tags_value = mapping.get(Value::String("tags".to_owned()));
    let mut metadata = Metadata {
        type_present: type_value.is_some(),
        priority_present: priority_value.is_some(),
        ..Metadata::default()
    };
    let mut diagnostics = Vec::new();

    if let Some(value) = type_value {
        match value {
            Value::String(value) if value == "note" => {
                metadata.element_type = Some(ElementType::Note);
            }
            Value::String(value) if value == "card" => {
                metadata.element_type = Some(ElementType::Card);
            }
            _ => diagnostics.push(Diagnostic::new(
                path,
                type_line,
                "type-invalid",
                format!(
                    "expected string 'note' or 'card', found {}",
                    yaml_kind(value)
                ),
            )),
        }
    }

    if let Some(value) = priority_value {
        match value {
            Value::Number(number) => match number.as_u64() {
                Some(priority @ 0..=100) => metadata.priority = Some(priority as u8),
                _ => diagnostics.push(Diagnostic::new(
                    path,
                    priority_line,
                    "priority-invalid",
                    format!("expected integer 0..=100, found {number}"),
                )),
            },
            _ => diagnostics.push(Diagnostic::new(
                path,
                priority_line,
                "priority-invalid",
                format!("expected integer 0..=100, found {}", yaml_kind(value)),
            )),
        }
    }

    if let Some(value) = tags_value {
        match value {
            Value::Sequence(values) if values.iter().all(|value| value.as_str().is_some()) => {
                metadata.tags = values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
            }
            _ => diagnostics.push(Diagnostic::new(
                path,
                tags_line,
                "tags-invalid",
                format!("expected a sequence of strings, found {}", yaml_kind(value)),
            )),
        }
    }

    FrontMatterResult {
        metadata,
        diagnostics,
    }
}

// serde_yaml_ng does not expose key spans, so locate top-level scheduler keys in
// the source.
fn key_line(yaml: &str, key: &str) -> Option<usize> {
    yaml.lines()
        .position(|line| {
            line.strip_prefix(key)
                .is_some_and(|rest| rest.starts_with(':'))
        })
        .map(|index| index + 2)
}

fn yaml_kind(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => format!("{value:?}"),
        Value::Sequence(_) => "sequence".to_owned(),
        Value::Mapping(_) => "mapping".to_owned(),
        Value::Tagged(_) => "tagged value".to_owned(),
    }
}

fn empty() -> FrontMatterResult {
    FrontMatterResult {
        metadata: Metadata::default(),
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(yaml: &str) -> FrontMatterResult {
        parse(Path::new("test.md"), yaml)
    }

    #[test]
    fn absent_frontmatter_has_missing_values_without_errors() {
        let parsed = result("type: note\npriority: 1\n");
        assert_eq!(parsed.metadata, Metadata::default());
        assert!(parsed.diagnostics.is_empty());
    }

    #[test]
    fn empty_frontmatter_has_missing_values_without_errors() {
        for source in ["---\n---\n", "---\n# no scheduler fields\n---\n"] {
            let parsed = result(source);
            assert_eq!(parsed.metadata, Metadata::default());
            assert!(parsed.diagnostics.is_empty());
        }
    }

    #[test]
    fn explicit_null_frontmatter_is_not_a_mapping() {
        let parsed = result("---\nnull\n---\n");
        assert_eq!(parsed.diagnostics[0].code, "frontmatter-not-mapping");
    }

    #[test]
    fn accepts_bom_and_priority_boundaries() {
        for priority in [0, 100] {
            let parsed = result(&format!(
                "\u{feff}---\ntype: note\npriority: {priority}\nextra: true\n---\n"
            ));
            assert!(parsed.diagnostics.is_empty());
            assert_eq!(parsed.metadata.priority, Some(priority));
        }
    }

    #[test]
    fn rejects_non_integer_and_out_of_range_priorities() {
        for priority in ["-1", "101", "10.5", "\"10\""] {
            let parsed = result(&format!("---\ntype: note\npriority: {priority}\n---\n"));
            assert_eq!(parsed.diagnostics[0].code, "priority-invalid");
        }
    }

    #[test]
    fn rejects_wrong_type_shapes_and_values() {
        for element_type in ["Note", "article", "1", "[note]"] {
            let parsed = result(&format!("---\ntype: {element_type}\npriority: 1\n---\n"));
            assert_eq!(parsed.diagnostics[0].code, "type-invalid");
        }
    }

    #[test]
    fn parses_string_tags_and_rejects_other_shapes() {
        let parsed = result("---\ntags:\n  - foo\n  - two words\n  - foo\n---\n");
        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.metadata.tags, ["foo", "two words", "foo"]);

        let parsed = result("---\ntags: []\n---\n");
        assert!(parsed.diagnostics.is_empty());
        assert!(parsed.metadata.tags.is_empty());

        for tags in ["foo", "[foo, 1]", "{foo: bar}", "null"] {
            let parsed = result(&format!("---\ntags: {tags}\n---\n"));
            assert_eq!(parsed.diagnostics[0].code, "tags-invalid");
            assert_eq!(parsed.diagnostics[0].line, Some(2));
        }
    }
}
