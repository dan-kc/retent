//! Markdown parsing, classification, and history editing.

mod edit;
mod frontmatter;
mod history;
mod table;

pub use edit::{append_card_event, append_note_event};
pub use history::{render_card_history, render_note_history};

use std::ops::Range;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::diagnostics::Diagnostic;

/// Scheduler kind selected by front matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementType {
    Note,
    Card,
}

impl std::fmt::Display for ElementType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Note => formatter.write_str("note"),
            Self::Card => formatter.write_str("card"),
        }
    }
}

/// Valid scheduler values extracted from front matter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub element_type: Option<ElementType>,
    pub priority: Option<u8>,
    pub tags: Vec<String>,
    pub(crate) type_present: bool,
    pub(crate) priority_present: bool,
}

/// One note-position presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteEvent {
    pub date: NaiveDate,
    pub end_line: u32,
    pub pass: u32,
    pub source_line: usize,
}

/// One flashcard rating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardEvent {
    pub date: NaiveDate,
    pub raw_rating: u8,
    pub source_line: usize,
}

/// Type-specific history reconstructed from the marked table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum History {
    Note(Vec<NoteEvent>),
    Card(Vec<CardEvent>),
}

/// The exclusive byte span and inclusive physical lines of a history block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySpan {
    pub bytes: Range<usize>,
    pub begin_line: usize,
    pub end_line: usize,
}

/// Classification assigned to every parsed Markdown file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Valid,
    Missing,
    Invalid,
}

/// A parsed document retaining the original text and edit spans.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub path: PathBuf,
    pub source: String,
    pub metadata: Metadata,
    pub history: Option<History>,
    pub history_span: Option<HistorySpan>,
    pub diagnostics: Vec<Diagnostic>,
    pub missing: Vec<&'static str>,
}

impl ParsedDocument {
    /// Return the document's mutually exclusive classification.
    pub fn classification(&self) -> Classification {
        if !self.diagnostics.is_empty() {
            Classification::Invalid
        } else if !self.missing.is_empty() {
            Classification::Missing
        } else {
            Classification::Valid
        }
    }
}

/// Parse and validate UTF-8 Markdown from memory.
pub fn parse(path: impl Into<PathBuf>, source: impl Into<String>) -> ParsedDocument {
    let path = path.into();
    let source = source.into();
    let front = frontmatter::parse(&path, &source);
    let mut diagnostics = front.diagnostics;
    let markers = history::find_history_span(&path, &source);
    diagnostics.extend(markers.diagnostics);

    let history = if diagnostics.is_empty() {
        match (front.metadata.element_type, markers.span.as_ref()) {
            (Some(element_type), Some(span)) => {
                let parsed = history::parse_history(&path, &source, span, element_type);
                diagnostics.extend(parsed.diagnostics);
                parsed.history
            }
            _ => None,
        }
    } else {
        None
    };

    let mut missing = Vec::new();
    if diagnostics.is_empty() {
        if !front.metadata.type_present {
            missing.push("type");
        }
        if !front.metadata.priority_present {
            missing.push("priority");
        }
        if front.metadata.element_type == Some(ElementType::Card) {
            let (has_front, has_back) = history::card_sections(&source);
            if !has_front {
                missing.push("Front");
            }
            if !has_back {
                missing.push("Back");
            }
        }
    }

    ParsedDocument {
        path,
        source,
        metadata: front.metadata,
        history,
        history_span: markers.span,
        diagnostics,
        missing,
    }
}

/// Read and parse a Markdown document as UTF-8.
pub fn read(path: &Path) -> Result<ParsedDocument, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let source = String::from_utf8(bytes)
        .map_err(|_| format!("{}: file is not valid UTF-8", path.display()))?;
    Ok(parse(path, source))
}

fn trim_line_ending(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}
