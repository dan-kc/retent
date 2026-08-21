use std::fs;
use std::io::Write;
use std::ops::Range;
use std::path::Path;

use serde_yaml_ng::{Mapping, Value};
use tempfile::NamedTempFile;

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Increment(u8),
    Decrement(u8),
    Add(u8),
    Upsert(u8),
}

pub(crate) fn canonical_target(path: &Path) -> Result<std::path::PathBuf, String> {
    inspect_regular_file(path)?;
    fs::canonicalize(path).map_err(|error| format!("cannot resolve file: {error}"))
}

pub(crate) fn edit(path: &Path, action: Action) -> Result<(), String> {
    inspect_regular_file(path)?;
    let bytes = fs::read(path).map_err(|error| format!("cannot read file: {error}"))?;
    let source = String::from_utf8(bytes).map_err(|_| "file is not valid UTF-8".to_owned())?;
    let Some(updated) = update_source(&source, action)? else {
        return Ok(());
    };

    replace_file(path, source.as_bytes(), updated.as_bytes())
}

fn inspect_regular_file(path: &Path) -> Result<fs::Metadata, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("cannot inspect file: {error}"))?;
    if metadata.file_type().is_symlink() {
        Err("path is a symbolic link".to_owned())
    } else if metadata.is_file() {
        Ok(metadata)
    } else {
        Err("path is not a regular file".to_owned())
    }
}

fn update_source(source: &str, action: Action) -> Result<Option<String>, String> {
    let Some(block) = frontmatter(source)? else {
        return match action {
            Action::Add(value) | Action::Upsert(value) => {
                Ok(Some(prepend_frontmatter(source, value)))
            }
            Action::Increment(_) | Action::Decrement(_) => Err("frontmatter is missing".to_owned()),
        };
    };
    let yaml = &source[block.yaml_start..block.closing_start];
    let mapping = match serde_yaml_ng::from_str(yaml) {
        Ok(Value::Mapping(mapping)) => mapping,
        Ok(_) => return Err("frontmatter is not a YAML mapping".to_owned()),
        Err(_) => return Err("frontmatter is malformed".to_owned()),
    };
    let key = Value::String("priority".to_owned());
    let existing = mapping.get(&key);

    match action {
        Action::Add(value) => {
            if existing.is_some() {
                return Err("priority already exists".to_owned());
            }
            insert_priority(source, block, value, &mapping).map(Some)
        }
        Action::Upsert(value) => match existing {
            Some(existing) => {
                if priority_value(existing) == Some(value) {
                    Ok(None)
                } else {
                    replace_priority(source, block, value, &mapping).map(Some)
                }
            }
            None => insert_priority(source, block, value, &mapping).map(Some),
        },
        Action::Increment(amount) => {
            let current = required_priority(existing)?;
            let value = current
                .checked_add(amount)
                .filter(|value| *value <= 10)
                .ok_or_else(|| "increment would raise priority above 10".to_owned())?;
            replace_priority(source, block, value, &mapping).map(Some)
        }
        Action::Decrement(amount) => {
            let current = required_priority(existing)?;
            let value = current
                .checked_sub(amount)
                .ok_or_else(|| "decrement would lower priority below 0".to_owned())?;
            replace_priority(source, block, value, &mapping).map(Some)
        }
    }
}

fn required_priority(value: Option<&Value>) -> Result<u8, String> {
    let value = value.ok_or_else(|| "priority is missing".to_owned())?;
    priority_value(value)
        .ok_or_else(|| "priority must be an unquoted integer from 0 to 10".to_owned())
}

fn priority_value(value: &Value) -> Option<u8> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .filter(|value| *value <= 10),
        _ => None,
    }
}

struct Frontmatter {
    yaml_start: usize,
    closing_start: usize,
    newline: &'static str,
}

fn frontmatter(source: &str) -> Result<Option<Frontmatter>, String> {
    let document_start = source
        .strip_prefix('\u{feff}')
        .map_or(0, |_| '\u{feff}'.len_utf8());
    let Some(opening) = line_at(source, document_start) else {
        return Ok(None);
    };
    if opening.content != "---" {
        return Ok(None);
    }
    let newline = if opening.ending == "\r\n" {
        "\r\n"
    } else {
        "\n"
    };
    let yaml_start = opening.next;
    let mut start = yaml_start;

    while let Some(line) = line_at(source, start) {
        if line.content == "---" {
            return Ok(Some(Frontmatter {
                yaml_start,
                closing_start: start,
                newline,
            }));
        }
        if line.next == start {
            break;
        }
        start = line.next;
    }

    Err("frontmatter is unclosed".to_owned())
}

struct Line<'a> {
    content: &'a str,
    ending: &'a str,
    next: usize,
}

fn line_at(source: &str, start: usize) -> Option<Line<'_>> {
    if start >= source.len() {
        return None;
    }
    match source[start..].find('\n') {
        Some(relative_end) => {
            let end = start + relative_end;
            let raw = &source[start..end];
            let (content, ending) = match raw.strip_suffix('\r') {
                Some(content) => (content, "\r\n"),
                None => (raw, "\n"),
            };
            Some(Line {
                content,
                ending,
                next: end + 1,
            })
        }
        None => Some(Line {
            content: &source[start..],
            ending: "",
            next: source.len(),
        }),
    }
}

fn insert_priority(
    source: &str,
    block: Frontmatter,
    value: u8,
    original: &Mapping,
) -> Result<String, String> {
    let mut updated = String::with_capacity(source.len() + 16);
    updated.push_str(&source[..block.closing_start]);
    updated.push_str("priority: ");
    updated.push_str(&value.to_string());
    updated.push_str(block.newline);
    updated.push_str(&source[block.closing_start..]);
    validate_updated_frontmatter(&updated, value, original)?;
    Ok(updated)
}

fn replace_priority(
    source: &str,
    block: Frontmatter,
    value: u8,
    original: &Mapping,
) -> Result<String, String> {
    let yaml = &source[block.yaml_start..block.closing_start];
    let range = editable_priority_range(yaml)
        .ok_or_else(|| "frontmatter cannot be safely edited without rewriting".to_owned())?;
    let range = block.yaml_start + range.start..block.yaml_start + range.end;
    let mut updated = String::with_capacity(source.len() + 2);
    updated.push_str(&source[..range.start]);
    updated.push_str(&value.to_string());
    updated.push_str(&source[range.end..]);
    validate_updated_frontmatter(&updated, value, original)?;
    Ok(updated)
}

fn editable_priority_range(yaml: &str) -> Option<Range<usize>> {
    let root_indent = root_indent(yaml)?;
    let mut range = None;
    let mut offset = 0;

    while let Some(line) = line_at(yaml, offset) {
        let indent = line.content.len() - line.content.trim_start_matches(' ').len();
        if indent == root_indent {
            let entry = &line.content[indent..];
            if let Some(after_key) = entry.strip_prefix("priority") {
                let spacing = after_key.len() - after_key.trim_start_matches(' ').len();
                let after_spacing = &after_key[spacing..];
                if let Some(value) = after_spacing.strip_prefix(':') {
                    let leading = value.len() - value.trim_start_matches(' ').len();
                    let value = &value[leading..];
                    let comment = yaml_comment_start(value).unwrap_or(value.len());
                    let scalar = &value[..comment];
                    let scalar = scalar.trim_end_matches([' ', '\t']);
                    if scalar.is_empty() || scalar.starts_with(['|', '>', '[', '{', '&', '*', '!'])
                    {
                        return None;
                    }
                    let start =
                        offset + indent + "priority".len() + spacing + ':'.len_utf8() + leading;
                    let candidate = start..start + scalar.len();
                    if range.replace(candidate).is_some() {
                        return None;
                    }
                }
            }
        }
        if line.next == offset {
            break;
        }
        offset = line.next;
    }

    range
}

fn root_indent(yaml: &str) -> Option<usize> {
    let mut indent = None;
    let mut offset = 0;

    while let Some(line) = line_at(yaml, offset) {
        let content = line.content.trim_start_matches(' ');
        if !content.is_empty() && !content.starts_with('#') {
            let current = line.content.len() - content.len();
            indent = Some(indent.map_or(current, |minimum: usize| minimum.min(current)));
        }
        if line.next == offset {
            break;
        }
        offset = line.next;
    }

    indent
}

fn yaml_comment_start(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if double_quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                double_quoted = false;
            }
        } else if single_quoted {
            if byte == b'\'' {
                if bytes.get(index + 1) == Some(&b'\'') {
                    index += 1;
                } else {
                    single_quoted = false;
                }
            }
        } else {
            match byte {
                b'"' => double_quoted = true,
                b'\'' => single_quoted = true,
                b'#' if index == 0 || bytes[index - 1].is_ascii_whitespace() => {
                    return Some(index);
                }
                _ => {}
            }
        }
        index += 1;
    }

    None
}

fn validate_updated_frontmatter(
    source: &str,
    expected: u8,
    original: &Mapping,
) -> Result<(), String> {
    let block = frontmatter(source)?
        .ok_or_else(|| "frontmatter cannot be safely edited without rewriting".to_owned())?;
    let yaml = &source[block.yaml_start..block.closing_start];
    let mapping = match serde_yaml_ng::from_str(yaml) {
        Ok(Value::Mapping(mapping)) => mapping,
        _ => return Err("frontmatter cannot be safely edited without rewriting".to_owned()),
    };
    let key = Value::String("priority".to_owned());
    let mut updated_without_priority = mapping;
    let updated_priority = updated_without_priority.remove(&key);
    let mut original_without_priority = original.clone();
    original_without_priority.remove(&key);
    if updated_priority.as_ref().and_then(priority_value) == Some(expected)
        && updated_without_priority == original_without_priority
    {
        Ok(())
    } else {
        Err("frontmatter cannot be safely edited without rewriting".to_owned())
    }
}

fn prepend_frontmatter(source: &str, value: u8) -> String {
    let (bom, body) = source
        .strip_prefix('\u{feff}')
        .map_or(("", source), |body| ("\u{feff}", body));
    let newline = if body
        .find('\n')
        .is_some_and(|index| body[..index].ends_with('\r'))
    {
        "\r\n"
    } else {
        "\n"
    };
    if body.is_empty() {
        format!("{bom}---{newline}priority: {value}{newline}---{newline}")
    } else {
        format!("{bom}---{newline}priority: {value}{newline}---{newline}{newline}{body}")
    }
}

fn replace_file(path: &Path, original: &[u8], contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "cannot determine the file's parent directory".to_owned())?;
    let permissions = fs::metadata(path)
        .map_err(|error| format!("cannot inspect file: {error}"))?
        .permissions();
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("cannot create temporary file: {error}"))?;
    temporary
        .write_all(contents)
        .map_err(|error| format!("cannot write temporary file: {error}"))?;
    temporary
        .as_file()
        .set_permissions(permissions)
        .map_err(|error| format!("cannot preserve file permissions: {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("cannot sync temporary file: {error}"))?;
    let current = fs::read(path)
        .map_err(|error| format!("cannot verify file before replacement: {error}"))?;
    if current != original {
        return Err("file changed while it was being edited".to_owned());
    }
    temporary
        .persist(path)
        .map_err(|error| format!("cannot replace file: {}", error.error))?;
    Ok(())
}
