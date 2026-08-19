use std::fs;
use std::path::Path;

use clap::ValueEnum;
use serde_yaml_ng::{Mapping, Value};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum Column {
    #[value(name = "type")]
    Type,
    #[value(name = "priority")]
    Priority,
    #[value(name = "desired retention")]
    DesiredRetention,
}

pub(crate) enum Frontmatter {
    Note(RequiredInteger),
    Card(RequiredInteger),
    Other,
    Invalid,
}

pub(crate) enum RequiredInteger {
    Valid(u64),
    Invalid,
}

impl Frontmatter {
    pub(crate) fn read(path: &Path) -> Self {
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(_) => return Self::Invalid,
        };
        Self::parse(&source)
    }

    fn parse(source: &str) -> Self {
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        let mut lines = source.lines();
        if lines.next() != Some("---") {
            return Self::Other;
        }

        let mut yaml = String::new();
        let mut closed = false;
        for line in lines {
            if line == "---" {
                closed = true;
                break;
            }
            yaml.push_str(line);
            yaml.push('\n');
        }
        if !closed {
            return Self::Invalid;
        }

        let mapping = match serde_yaml_ng::from_str(&yaml) {
            Ok(Value::Mapping(mapping)) => mapping,
            Ok(_) => return Self::Other,
            Err(_) => return Self::Invalid,
        };

        match mapping.get(Value::String("type".to_owned())) {
            Some(Value::String(value)) if value == "note" => {
                Self::Note(required_integer(&mapping, "priority", 10))
            }
            Some(Value::String(value)) if value == "card" => {
                Self::Card(required_integer(&mapping, "desired retention", 100))
            }
            _ => Self::Other,
        }
    }

    pub(crate) fn value(&self, column: Column) -> String {
        match (self, column) {
            (Self::Note(_), Column::Type) => "note".to_owned(),
            (Self::Card(_), Column::Type) => "card".to_owned(),
            (Self::Note(priority), Column::Priority) => priority.value(),
            (Self::Card(retention), Column::DesiredRetention) => retention.value(),
            (Self::Invalid, _) => "?".to_owned(),
            _ => "-".to_owned(),
        }
    }
}

impl RequiredInteger {
    fn value(&self) -> String {
        match self {
            Self::Valid(value) => value.to_string(),
            Self::Invalid => "?".to_owned(),
        }
    }
}

fn required_integer(mapping: &Mapping, key: &str, maximum: u64) -> RequiredInteger {
    match mapping
        .get(Value::String(key.to_owned()))
        .and_then(Value::as_u64)
    {
        Some(value) if value <= maximum => RequiredInteger::Valid(value),
        _ => RequiredInteger::Invalid,
    }
}
