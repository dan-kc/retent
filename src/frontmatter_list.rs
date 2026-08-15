//! Lossless syntax changes for a top-level frontmatter sequence.

use serde_yaml_ng::Value;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum Style {
    Flow,
    Block,
    Toggle,
}

pub fn format_paths(
    root: &Path,
    paths: &[PathBuf],
    field: &str,
    style: Style,
) -> Result<usize, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    let mut edits = Vec::new();
    let mut seen = Vec::new();
    for relative in paths {
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(format!(
                "{}: selected path must be relative and remain inside root",
                relative.display()
            ));
        }
        let path = root.join(relative);
        let resolved = path
            .canonicalize()
            .map_err(|error| format!("{}: {error}", relative.display()))?;
        if !resolved.starts_with(&root) {
            return Err(format!(
                "{}: selected path is outside root",
                relative.display()
            ));
        }
        if fs::symlink_metadata(&path)
            .map_err(|error| format!("{}: {error}", relative.display()))?
            .file_type()
            .is_symlink()
        {
            return Err(format!(
                "{}: refusing to edit a symbolic link",
                relative.display()
            ));
        }
        if seen.contains(&resolved) {
            continue;
        }
        seen.push(resolved.clone());
        let source = fs::read_to_string(&resolved)
            .map_err(|error| format!("{}: {error}", relative.display()))?;
        let candidate = format(&source, field, style)
            .map_err(|error| format!("{}: {error}", relative.display()))?;
        if candidate != source {
            edits.push((resolved, candidate));
        }
    }
    for (path, candidate) in &edits {
        fs::write(path, candidate).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(edits.len())
}

pub fn format(source: &str, field: &str, style: Style) -> Result<String, String> {
    if field.is_empty() || field.contains(['\r', '\n', ':']) {
        return Err("field must be a non-empty top-level YAML key".to_owned());
    }
    let (yaml_start, yaml_end, newline) = frontmatter_bounds(source)?;
    let yaml_source = &source[yaml_start..yaml_end];
    let document: Value = serde_yaml_ng::from_str(yaml_source)
        .map_err(|error| format!("invalid YAML frontmatter: {error}"))?;
    let mapping = document
        .as_mapping()
        .ok_or_else(|| "YAML frontmatter is not a mapping".to_owned())?;
    let value = mapping
        .get(Value::String(field.to_owned()))
        .ok_or_else(|| format!("frontmatter field {field:?} was not found"))?;
    if !value.is_sequence() {
        return Err(format!("frontmatter field {field:?} is not a sequence"));
    }

    let located = locate_field(yaml_source, field, newline)?;
    let target = match style {
        Style::Flow => Syntax::Flow,
        Style::Block => Syntax::Block,
        Style::Toggle => match located.syntax {
            Syntax::Flow => Syntax::Block,
            Syntax::Block => Syntax::Flow,
        },
    };
    if target == located.syntax
        || (value.as_sequence().unwrap().is_empty() && target == Syntax::Block)
    {
        return Ok(source.to_owned());
    }

    let replacement = match (located.syntax, target) {
        (Syntax::Block, Syntax::Flow) => block_to_flow(&located, newline)?,
        (Syntax::Flow, Syntax::Block) => flow_to_block(&located, newline)?,
        _ => unreachable!(),
    };
    let absolute_start = yaml_start + located.start;
    let absolute_end = yaml_start + located.end;
    Ok(format!(
        "{}{}{}",
        &source[..absolute_start],
        replacement,
        &source[absolute_end..]
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Flow,
    Block,
}

struct Located<'a> {
    start: usize,
    end: usize,
    key: &'a str,
    value: &'a str,
    suffix: &'a str,
    syntax: Syntax,
}

fn frontmatter_bounds(source: &str) -> Result<(usize, usize, &str), String> {
    let without_bom = source.strip_prefix('\u{feff}').unwrap_or(source);
    let offset = source.len() - without_bom.len();
    let newline = if without_bom.starts_with("---\r\n") {
        "\r\n"
    } else if without_bom.starts_with("---\n") {
        "\n"
    } else {
        return Err("input does not start with YAML frontmatter".to_owned());
    };
    let start = offset + 3 + newline.len();
    let marker = format!("{newline}---");
    let relative_end = source[start..]
        .find(&marker)
        .ok_or_else(|| "frontmatter has no closing delimiter".to_owned())?;
    Ok((start, start + relative_end + newline.len(), newline))
}

fn locate_field<'a>(yaml: &'a str, field: &str, newline: &str) -> Result<Located<'a>, String> {
    let mut offset = 0;
    for line_with_newline in yaml.split_inclusive(newline) {
        let line = line_with_newline
            .strip_suffix(newline)
            .unwrap_or(line_with_newline);
        let prefix = format!("{field}:");
        if !line.starts_with(&prefix) {
            offset += line_with_newline.len();
            continue;
        }
        let after_key = &line[prefix.len()..];
        let trimmed = after_key.trim_start();
        if trimmed.starts_with('[') {
            let value_start = offset + line.find('[').unwrap();
            let value_end = flow_end(yaml, value_start)?;
            let line_end = yaml[value_end..]
                .find(newline)
                .map_or(yaml.len(), |end| value_end + end + newline.len());
            let suffix_end = line_end - newline.len().min(line_end);
            let suffix = &yaml[value_end..suffix_end];
            if contains_comment(&yaml[value_start..value_end]) {
                return Err(format!(
                    "frontmatter field {field:?} has sequence item comments"
                ));
            }
            return Ok(Located {
                start: offset,
                end: line_end,
                key: &yaml[offset..value_start],
                value: &yaml[value_start..value_end],
                suffix,
                syntax: Syntax::Flow,
            });
        }
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            return Err(format!(
                "frontmatter field {field:?} is not written as a sequence"
            ));
        }
        let mut end = offset + line_with_newline.len();
        for following in yaml[end..].split_inclusive(newline) {
            let plain = following.strip_suffix(newline).unwrap_or(following);
            if plain.starts_with("  ") || plain.trim().is_empty() {
                end += following.len();
            } else {
                break;
            }
        }
        return Ok(Located {
            start: offset,
            end,
            key: &yaml[offset..offset + prefix.len()],
            value: &yaml[offset + prefix.len()..end],
            suffix: trimmed,
            syntax: Syntax::Block,
        });
    }
    Err(format!("frontmatter field {field:?} was not found"))
}

fn block_to_flow(located: &Located<'_>, newline: &str) -> Result<String, String> {
    let mut items = Vec::new();
    for line in located.value.split(newline).skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let item = line.strip_prefix("  - ").ok_or_else(|| {
            "block sequence contains a multiline item; cannot make it one line".to_owned()
        })?;
        if contains_comment(item) {
            return Err(
                "block sequence contains item comments; cannot preserve them on one line"
                    .to_owned(),
            );
        }
        items.push(item);
    }
    let comment = located.value.split(newline).next().unwrap_or("").trim();
    let suffix = if comment.is_empty() { "" } else { comment };
    Ok(format!(
        "{} [{}]{}{}{}",
        located.key,
        items.join(", "),
        if suffix.is_empty() { "" } else { " " },
        suffix,
        newline
    ))
}

fn flow_to_block(located: &Located<'_>, newline: &str) -> Result<String, String> {
    let inside = &located.value[1..located.value.len() - 1];
    let items = split_flow_items(inside)?;
    if items.is_empty() {
        return Ok(format!(
            "{} {}{}{}",
            located.key, located.value, located.suffix, newline
        ));
    }
    let mut output = located.key.trim_end().to_owned();
    output.push_str(located.suffix);
    output.push_str(newline);
    for item in items {
        output.push_str("  - ");
        output.push_str(item.trim());
        output.push_str(newline);
    }
    Ok(output)
}

fn flow_end(source: &str, start: usize) -> Result<usize, String> {
    let mut depth = 0;
    let mut quote = None;
    let mut escaped = false;
    for (relative, character) in source[start..].char_indices() {
        if let Some(active) = quote {
            if active == '"' && character == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if character == active && !escaped {
                quote = None;
            }
            escaped = false;
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(start + relative + character.len_utf8());
                }
            }
            _ => {}
        }
    }
    Err("invalid YAML flow sequence: missing closing bracket".to_owned())
}

fn split_flow_items(source: &str) -> Result<Vec<&str>, String> {
    if source.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in source.char_indices() {
        if let Some(active) = quote {
            if active == '"' && character == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if character == active && !escaped {
                quote = None;
            }
            escaped = false;
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                items.push(&source[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if quote.is_some() || depth != 0 {
        return Err("invalid YAML flow sequence".to_owned());
    }
    items.push(&source[start..]);
    Ok(items)
}

fn contains_comment(source: &str) -> bool {
    let mut quote = None;
    let mut previous_whitespace = true;
    for character in source.chars() {
        match (quote, character) {
            (Some(active), current) if active == current => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, '#') if previous_whitespace => return true,
            _ => {}
        }
        previous_whitespace = character.is_whitespace();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{Style, format};

    #[test]
    fn converts_both_styles_and_toggles() {
        let block = "---\ntags:\n  - one\n  - two\n---\nBody\n";
        let flow = "---\ntags: [one, two]\n---\nBody\n";
        assert_eq!(format(block, "tags", Style::Flow).unwrap(), flow);
        assert_eq!(format(flow, "tags", Style::Block).unwrap(), block);
        assert_eq!(format(flow, "tags", Style::Toggle).unwrap(), block);
    }

    #[test]
    fn handles_quoted_commas_hashes_and_nested_flow_values() {
        let source = "---\nvalues: ['a, b', \"hash # value\", {key: value}, [one, two]]\n---\n";
        let output = format(source, "values", Style::Block).unwrap();
        assert!(output.contains("  - 'a, b'\n"));
        assert!(output.contains("  - \"hash # value\"\n"));
        assert!(output.contains("  - {key: value}\n"));
        assert!(output.contains("  - [one, two]\n"));
    }

    #[test]
    fn preserves_bom_crlf_body_and_idempotent_input() {
        let source =
            "\u{feff}---\r\ntags: [one, two]\r\n---\r\nBody\r\n---\r\ntags:\r\n  - body\r\n";
        assert_eq!(format(source, "tags", Style::Flow).unwrap(), source);
    }

    #[test]
    fn keeps_empty_sequence_flow_style() {
        let source = "---\ntags: []\n---\n";
        assert_eq!(format(source, "tags", Style::Block).unwrap(), source);
    }

    #[test]
    fn rejects_inputs_that_cannot_be_losslessly_converted() {
        for (source, message) in [
            ("# no frontmatter\n", "frontmatter"),
            ("---\ntags: [one]\n", "closing"),
            ("---\ntitle: Example\n---\n", "not found"),
            ("---\ntags: value\n---\n", "not a sequence"),
            ("---\ntags: [one\n---\n", "invalid YAML"),
            ("---\ntags:\n  - one # comment\n---\n", "comments"),
        ] {
            assert!(
                format(source, "tags", Style::Flow)
                    .unwrap_err()
                    .contains(message)
            );
        }
    }
}
