//! Marker scanning, history table parsing, and canonical rendering.

use std::path::Path;

use chrono::NaiveDate;

use super::table::{cells, valid_separator};
use super::{CardEvent, ElementType, History, HistorySpan, NoteEvent, trim_line_ending};
use crate::diagnostics::Diagnostic;

const BEGIN: &str = "<!-- HISTORY:BEGIN -->";
const END: &str = "<!-- HISTORY:END -->";

pub(super) struct MarkerResult {
    pub span: Option<HistorySpan>,
    pub diagnostics: Vec<Diagnostic>,
}

pub(super) struct HistoryResult {
    pub history: Option<History>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MarkerKind {
    Begin,
    End,
}

#[derive(Clone, Copy)]
struct Marker {
    kind: MarkerKind,
    start: usize,
    end: usize,
    line: usize,
}

pub(super) fn find_history_span(path: &Path, source: &str) -> MarkerResult {
    let mut markers = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let frontmatter_end = frontmatter_end_line(source);
    for line in source_lines(source) {
        if frontmatter_end.is_some_and(|end| line.number <= end) {
            continue;
        }
        let trimmed = line.text.trim();
        if let Some((character, length)) = fence {
            if fence_run(trimmed, character) >= length {
                fence = None;
            }
            continue;
        }
        if let Some((character, length)) = opening_fence(trimmed) {
            fence = Some((character, length));
            continue;
        }
        let kind = match trimmed {
            BEGIN => Some(MarkerKind::Begin),
            END => Some(MarkerKind::End),
            _ => None,
        };
        if let Some(kind) = kind {
            markers.push(Marker {
                kind,
                start: line.start,
                end: line.end,
                line: line.number,
            });
        }
    }

    match markers.as_slice() {
        [] => MarkerResult {
            span: None,
            diagnostics: Vec::new(),
        },
        [marker] if marker.kind == MarkerKind::Begin => invalid_marker(
            path,
            marker.line,
            "history-unmatched-begin",
            "history begin marker has no matching end marker",
        ),
        [marker] => invalid_marker(
            path,
            marker.line,
            "history-unmatched-end",
            "history end marker has no preceding begin marker",
        ),
        [begin, end] if begin.kind == MarkerKind::Begin && end.kind == MarkerKind::End => {
            MarkerResult {
                span: Some(HistorySpan {
                    bytes: begin.start..end.end,
                    begin_line: begin.line,
                    end_line: end.line,
                }),
                diagnostics: Vec::new(),
            }
        }
        [first, ..] if first.kind == MarkerKind::End => invalid_marker(
            path,
            first.line,
            "history-unmatched-end",
            "history markers are reversed",
        ),
        [_, second] => invalid_marker(
            path,
            second.line,
            "history-duplicate",
            "history markers are nested or duplicated",
        ),
        [_, second, ..] => invalid_marker(
            path,
            second.line,
            "history-duplicate",
            "at most one history block is allowed",
        ),
    }
}

pub(super) fn parse_history(
    path: &Path,
    source: &str,
    span: &HistorySpan,
    element_type: ElementType,
) -> HistoryResult {
    let lines = source_lines(source);
    let mut inside: Vec<_> = lines
        .into_iter()
        .filter(|line| line.number > span.begin_line && line.number < span.end_line)
        .collect();
    while inside
        .first()
        .is_some_and(|line| line.text.trim().is_empty())
    {
        inside.remove(0);
    }
    while inside
        .last()
        .is_some_and(|line| line.text.trim().is_empty())
    {
        inside.pop();
    }
    if inside.len() < 2 {
        return history_error(
            path,
            Some(span.begin_line + 1),
            "history-table-missing",
            "history block must contain exactly one table",
        );
    }
    if inside.iter().any(|line| line.text.trim().is_empty()) {
        return history_error(
            path,
            Some(inside[0].number),
            "history-table-missing",
            "blank lines or prose are not allowed inside the history table",
        );
    }

    let header = cells(inside[0].text);
    let separator = cells(inside[1].text);
    let expected: &[&str] = match element_type {
        ElementType::Note => &["Date", "End Line", "Pass"],
        ElementType::Card => &["Date", "Rating"],
    };
    if header != expected {
        return history_error(
            path,
            Some(inside[0].number),
            "history-header-invalid",
            format!("expected columns {}", expected.join(" | ")),
        );
    }
    if separator.len() != expected.len() || !valid_separator(&separator) {
        return history_error(
            path,
            Some(inside[1].number),
            "history-header-invalid",
            "invalid Markdown table separator",
        );
    }

    match element_type {
        ElementType::Note => parse_notes(path, &inside[2..]),
        ElementType::Card => parse_cards(path, &inside[2..]),
    }
}

fn parse_notes(path: &Path, rows: &[SourceLine<'_>]) -> HistoryResult {
    let mut events = Vec::new();
    for row in rows {
        let values = cells(row.text);
        if values.len() != 3 {
            return history_error(
                path,
                Some(row.number),
                "history-column-count",
                format!("expected 3 cells, found {}", values.len()),
            );
        }
        let date = match exact_date(values[0]) {
            Some(date) => date,
            None => {
                return history_error(
                    path,
                    Some(row.number),
                    "history-date-invalid",
                    format!("expected date YYYY-MM-DD, found {:?}", values[0]),
                );
            }
        };
        if events
            .last()
            .is_some_and(|last: &NoteEvent| date < last.date)
        {
            return history_error(
                path,
                Some(row.number),
                "history-date-order",
                "history dates must be nondecreasing",
            );
        }
        let end_line = match values[1].parse::<u32>() {
            Ok(value) => value,
            Err(_) => {
                return history_error(
                    path,
                    Some(row.number),
                    "history-end-line-invalid",
                    "End Line must be an integer in 0..=4294967295",
                );
            }
        };
        let pass = match values[2].parse::<u32>() {
            Ok(value) if value >= 1 => value,
            _ => {
                return history_error(
                    path,
                    Some(row.number),
                    "history-pass-invalid",
                    "Pass must be an integer greater than or equal to 1",
                );
            }
        };
        if let Some(last) = events.last() {
            let valid = if end_line < last.end_line {
                last.pass.checked_add(1) == Some(pass)
            } else {
                pass == last.pass
            };
            if !valid {
                return history_error(
                    path,
                    Some(row.number),
                    "history-pass-transition",
                    "Pass must increment exactly when End Line decreases",
                );
            }
        }
        events.push(NoteEvent {
            date,
            end_line,
            pass,
            source_line: row.number,
        });
    }
    HistoryResult {
        history: Some(History::Note(events)),
        diagnostics: Vec::new(),
    }
}

fn parse_cards(path: &Path, rows: &[SourceLine<'_>]) -> HistoryResult {
    let mut events = Vec::new();
    for row in rows {
        let values = cells(row.text);
        if values.len() != 2 {
            return history_error(
                path,
                Some(row.number),
                "history-column-count",
                format!("expected 2 cells, found {}", values.len()),
            );
        }
        let date = match exact_date(values[0]) {
            Some(date) => date,
            None => {
                return history_error(
                    path,
                    Some(row.number),
                    "history-date-invalid",
                    format!("expected date YYYY-MM-DD, found {:?}", values[0]),
                );
            }
        };
        if events
            .last()
            .is_some_and(|last: &CardEvent| date < last.date)
        {
            return history_error(
                path,
                Some(row.number),
                "history-date-order",
                "history dates must be nondecreasing",
            );
        }
        let raw_rating = match values[1].parse::<u8>() {
            Ok(value @ 1..=4) => value,
            _ => {
                return history_error(
                    path,
                    Some(row.number),
                    "history-rating-invalid",
                    "Rating must be an integer in 1..=4",
                );
            }
        };
        events.push(CardEvent {
            date,
            raw_rating,
            source_line: row.number,
        });
    }
    HistoryResult {
        history: Some(History::Card(events)),
        diagnostics: Vec::new(),
    }
}

/// Render a note history block in canonical form.
pub fn render_note_history(events: &[NoteEvent]) -> String {
    let mut rendered = String::from(
        "<!-- HISTORY:BEGIN -->\n\n| Date       | End Line | Pass |\n| ---------- | -------: | ---: |\n",
    );
    for event in events {
        rendered.push_str(&format!(
            "| {} | {:>8} | {:>4} |\n",
            event.date, event.end_line, event.pass
        ));
    }
    rendered.push_str("\n<!-- HISTORY:END -->\n");
    rendered
}

/// Render a card history block in canonical form.
pub fn render_card_history(events: &[CardEvent]) -> String {
    let mut rendered = String::from(
        "<!-- HISTORY:BEGIN -->\n\n| Date       | Rating |\n| ---------- | -----: |\n",
    );
    for event in events {
        rendered.push_str(&format!("| {} | {:>6} |\n", event.date, event.raw_rating));
    }
    rendered.push_str("\n<!-- HISTORY:END -->\n");
    rendered
}

pub(super) fn card_sections(source: &str) -> (bool, bool) {
    let mut front = false;
    let mut back = false;
    let mut fence = None;
    let frontmatter_end = frontmatter_end_line(source);
    for (index, line) in source.lines().enumerate() {
        if frontmatter_end.is_some_and(|end| index < end) {
            continue;
        }
        let trimmed = line.trim();
        if let Some((character, length)) = fence {
            if fence_run(trimmed, character) >= length {
                fence = None;
            }
            continue;
        }
        if let Some(opening) = opening_fence(trimmed) {
            fence = Some(opening);
            continue;
        }
        front |= trimmed == "## Front";
        back |= trimmed == "## Back";
    }
    (front, back)
}

fn frontmatter_end_line(source: &str) -> Option<usize> {
    let mut lines = source.lines();
    let first = lines.next()?;
    let first = first.strip_prefix('\u{feff}').unwrap_or(first);
    if first != "---" {
        return None;
    }
    Some(
        lines
            .position(|line| line == "---")
            .map_or(usize::MAX, |index| index + 2),
    )
}

fn exact_date(value: &str) -> Option<NaiveDate> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return None;
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn history_error(
    path: &Path,
    line: Option<usize>,
    code: &'static str,
    message: impl Into<String>,
) -> HistoryResult {
    HistoryResult {
        history: None,
        diagnostics: vec![Diagnostic::new(path, line, code, message)],
    }
}

fn invalid_marker(
    path: &Path,
    line: usize,
    code: &'static str,
    message: &'static str,
) -> MarkerResult {
    MarkerResult {
        span: None,
        diagnostics: vec![Diagnostic::new(path, Some(line), code, message)],
    }
}

#[derive(Clone, Copy)]
struct SourceLine<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    number: usize,
}

fn source_lines(source: &str) -> Vec<SourceLine<'_>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        let end = start + text.len();
        lines.push(SourceLine {
            text: trim_line_ending(text),
            start,
            end,
            number: index + 1,
        });
        start = end;
    }
    if start < source.len() || source.is_empty() {
        lines.push(SourceLine {
            text: &source[start..],
            start,
            end: source.len(),
            number: lines.len() + 1,
        });
    }
    lines
}

fn opening_fence(line: &str) -> Option<(u8, usize)> {
    for character in *b"`~" {
        let length = fence_run(line, character);
        if length >= 3 {
            return Some((character, length));
        }
    }
    None
}

fn fence_run(line: &str, character: u8) -> usize {
    line.as_bytes()
        .iter()
        .take_while(|byte| **byte == character)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(source: &str) -> &'static str {
        let parsed = crate::document::parse("test.md", source);
        parsed.diagnostics[0].code
    }

    #[test]
    fn parses_pipe_variants_and_same_day_note_rows() {
        let source = "---\ntype: note\npriority: 0\n---\n<!-- HISTORY:BEGIN -->\nDate | End Line | Pass\n:---:|---:|---\n2026-08-14 | 2 | 1\n2026-08-14 | 1 | 2\n<!-- HISTORY:END -->\n";
        let parsed = crate::document::parse("note.md", source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(matches!(parsed.history, Some(History::Note(events)) if events.len() == 2));
    }

    #[test]
    fn ignores_markers_in_fences() {
        let source = "```html\n<!-- HISTORY:BEGIN -->\n```\n";
        assert!(find_history_span(Path::new("x.md"), source).span.is_none());
    }

    #[test]
    fn ignores_markers_and_card_headings_in_frontmatter_scalars() {
        let source = "---\ntype: card\npriority: 1\ndescription: |\n  <!-- HISTORY:BEGIN -->\n  ## Front\n  ## Back\n---\nbody\n";
        assert!(find_history_span(Path::new("x.md"), source).span.is_none());
        assert_eq!(card_sections(source), (false, false));
    }

    #[test]
    fn rejects_bad_pass_transition() {
        let source = "---\ntype: note\npriority: 1\n---\n<!-- HISTORY:BEGIN -->\n| Date | End Line | Pass |\n| --- | --- | --- |\n| 2026-01-01 | 3 | 1 |\n| 2026-01-02 | 4 | 2 |\n<!-- HISTORY:END -->\n";
        let parsed = crate::document::parse("note.md", source);
        assert_eq!(parsed.diagnostics[0].code, "history-pass-transition");
    }

    #[test]
    fn rejects_descending_dates_for_both_history_types() {
        let note = "---\ntype: note\npriority: 1\n---\n<!-- HISTORY:BEGIN -->\n| Date | End Line | Pass |\n| --- | --- | --- |\n| 2026-01-02 | 3 | 1 |\n| 2026-01-01 | 4 | 1 |\n<!-- HISTORY:END -->\n";
        let card = "---\ntype: card\npriority: 1\n---\n## Front\nQ\n## Back\nA\n<!-- HISTORY:BEGIN -->\n| Date | Rating |\n| --- | --- |\n| 2026-01-02 | 3 |\n| 2026-01-01 | 4 |\n<!-- HISTORY:END -->\n";

        assert_eq!(diagnostic(note), "history-date-order");
        assert_eq!(diagnostic(card), "history-date-order");
    }

    #[test]
    fn rejects_card_ratings_outside_the_cli_range() {
        for rating in ["0", "5", "easy"] {
            let source = format!(
                concat!(
                    "---\ntype: card\npriority: 1\n---\n",
                    "## Front\nQ\n## Back\nA\n",
                    "<!-- HISTORY:BEGIN -->\n",
                    "| Date | Rating |\n",
                    "| --- | --- |\n",
                    "| 2026-01-01 | {} |\n",
                    "<!-- HISTORY:END -->\n",
                ),
                rating
            );
            assert_eq!(diagnostic(&source), "history-rating-invalid");
        }
    }

    #[test]
    fn rejects_unmatched_reversed_and_duplicate_markers() {
        let cases = [
            ("<!-- HISTORY:BEGIN -->\n", "history-unmatched-begin"),
            ("<!-- HISTORY:END -->\n", "history-unmatched-end"),
            (
                "<!-- HISTORY:END -->\n<!-- HISTORY:BEGIN -->\n",
                "history-unmatched-end",
            ),
            (
                "<!-- HISTORY:BEGIN -->\n<!-- HISTORY:BEGIN -->\n<!-- HISTORY:END -->\n",
                "history-duplicate",
            ),
        ];

        for (source, expected) in cases {
            let result = find_history_span(Path::new("test.md"), source);
            assert_eq!(result.diagnostics[0].code, expected);
        }
    }

    #[test]
    fn card_sections_must_be_real_unfenced_headings() {
        let source = "```md\n## Front\n## Back\n```\n## Front\ntext\n## Back\ntext\n";
        assert_eq!(card_sections(source), (true, true));
        assert_eq!(card_sections("# Front\n### Back\n"), (false, false));
    }
}
