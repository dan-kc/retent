//! Pure history-block splicing and transactional file replacement.

use std::fs;
use std::io::Write;
use std::path::Path;

use chrono::NaiveDate;
use tempfile::Builder;

use super::{
    CardEvent, Classification, ElementType, History, NoteEvent, ParsedDocument, parse,
    render_card_history, render_note_history,
};

/// Successful note update details.
#[derive(Debug)]
pub struct NoteEdit {
    pub stored_end_line: u32,
    pub pass: u32,
    pub pass_incremented: bool,
}

/// Validate, append a note event, reparse, and atomically replace the file.
pub fn append_note_event(
    path: &Path,
    supplied_end_line: u32,
    date: NaiveDate,
) -> Result<NoteEdit, String> {
    validate_edit_target(path)?;
    let document = super::read(path)?;
    validate_for_edit(&document, ElementType::Note)?;
    let mut events = match &document.history {
        Some(History::Note(events)) => events.clone(),
        None => Vec::new(),
        _ => {
            return Err(format!(
                "{}: history schema does not match note",
                path.display()
            ));
        }
    };
    if let Some(last) = events.last()
        && date < last.date
    {
        return Err(format!(
            "{}: date {date} is earlier than latest history date {}; no changes made",
            path.display(),
            last.date
        ));
    }
    if let Some(span) = &document.history_span {
        let supplied = supplied_end_line as usize;
        if supplied >= span.begin_line && supplied <= span.end_line {
            return Err(format!(
                "{}: supplied line points inside the history block; no changes made",
                path.display()
            ));
        }
    }

    let provisional_pass = events.last().map_or(1, |event| event.pass);
    let placeholder = NoteEvent {
        date,
        end_line: supplied_end_line,
        pass: provisional_pass,
        source_line: 0,
    };
    events.push(placeholder);
    let provisional = render_note_history(&events);
    let delta = document
        .history_span
        .as_ref()
        .map(|span| line_count(&provisional) as i64 - (span.end_line - span.begin_line + 1) as i64)
        .unwrap_or(0);
    let is_before = document
        .history_span
        .as_ref()
        .is_some_and(|span| span.end_line < supplied_end_line as usize);
    let stored = if is_before {
        (supplied_end_line as i64)
            .checked_add(delta)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| format!("{}: adjusted End Line overflows", path.display()))?
    } else {
        supplied_end_line
    };
    let previous = events
        .len()
        .checked_sub(2)
        .and_then(|index| events.get(index));
    let (pass, pass_incremented) = next_pass(previous, stored)?;
    events.last_mut().unwrap().end_line = stored;
    events.last_mut().unwrap().pass = pass;
    let block = render_note_history(&events);
    let candidate = splice(&document, &block);
    validate_candidate(path, &candidate, ElementType::Note)?;
    atomic_replace(path, candidate.as_bytes())?;
    Ok(NoteEdit {
        stored_end_line: stored,
        pass,
        pass_incremented,
    })
}

/// Validate, append a card event, reparse, and atomically replace the file.
pub fn append_card_event(path: &Path, rating: u8, date: NaiveDate) -> Result<(), String> {
    validate_edit_target(path)?;
    if !(1..=4).contains(&rating) {
        return Err("rating must be an integer in 1..=4".to_owned());
    }
    let document = super::read(path)?;
    validate_for_edit(&document, ElementType::Card)?;
    let mut events = match &document.history {
        Some(History::Card(events)) => events.clone(),
        None => Vec::new(),
        _ => {
            return Err(format!(
                "{}: history schema does not match card",
                path.display()
            ));
        }
    };
    if let Some(last) = events.last()
        && date < last.date
    {
        return Err(format!(
            "{}: date {date} is earlier than latest history date {}; no changes made",
            path.display(),
            last.date
        ));
    }
    events.push(CardEvent {
        date,
        raw_rating: rating,
        source_line: 0,
    });
    let candidate = splice(&document, &render_card_history(&events));
    validate_candidate(path, &candidate, ElementType::Card)?;
    atomic_replace(path, candidate.as_bytes())
}

fn validate_edit_target(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{}: {error}; no changes made", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{}: symbolic links cannot be edited",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!("{}: expected a regular file", path.display()));
    }
    if !crate::discover::is_markdown(path) {
        return Err(format!("{}: expected a .md file", path.display()));
    }
    Ok(())
}

fn validate_for_edit(document: &ParsedDocument, expected: ElementType) -> Result<(), String> {
    if let Some(diagnostic) = document.diagnostics.first() {
        return Err(format!("{diagnostic}; no changes made"));
    }
    match document.metadata.element_type {
        Some(found) if found == expected => {}
        Some(found) => {
            return Err(format!(
                "{}: expected type '{expected}', found '{found}'; no changes made",
                document.path.display()
            ));
        }
        None => {
            return Err(format!(
                "{}: missing required type '{expected}'; no changes made",
                document.path.display()
            ));
        }
    }
    // Edits only require a valid type and history.
    Ok(())
}

fn next_pass(last: Option<&NoteEvent>, supplied_end_line: u32) -> Result<(u32, bool), String> {
    match last {
        None => Ok((1, false)),
        Some(last) if supplied_end_line < last.end_line => last
            .pass
            .checked_add(1)
            .map(|pass| (pass, true))
            .ok_or_else(|| "cannot increment Pass beyond u32::MAX".to_owned()),
        Some(last) => Ok((last.pass, false)),
    }
}

fn splice(document: &ParsedDocument, block: &str) -> String {
    if let Some(span) = &document.history_span {
        let mut candidate = String::with_capacity(document.source.len() + block.len());
        candidate.push_str(&document.source[..span.bytes.start]);
        candidate.push_str(block);
        candidate.push_str(&document.source[span.bytes.end..]);
        candidate
    } else {
        let mut candidate = document.source.clone();
        if !candidate.is_empty() && !candidate.ends_with('\n') {
            candidate.push('\n');
        }
        if !candidate.is_empty() && !candidate.ends_with("\n\n") {
            candidate.push('\n');
        }
        candidate.push_str(block);
        candidate
    }
}

fn validate_candidate(path: &Path, candidate: &str, expected: ElementType) -> Result<(), String> {
    let reparsed = parse(path, candidate);
    if let Some(diagnostic) = reparsed.diagnostics.first() {
        return Err(format!(
            "candidate failed validation: {diagnostic}; no changes made"
        ));
    }
    let history_matches = matches!(
        (&reparsed.history, expected),
        (Some(History::Note(_)), ElementType::Note) | (Some(History::Card(_)), ElementType::Card)
    );
    if !history_matches || reparsed.classification() == Classification::Invalid {
        return Err("candidate history failed validation; no changes made".to_owned());
    }
    Ok(())
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| format!("{}: path has no parent directory", path.display()))?;
    let permissions = fs::metadata(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .permissions();
    let mut temporary = Builder::new()
        .prefix(".retent-")
        .suffix(".md")
        .tempfile_in(directory)
        .map_err(|error| format!("{}: cannot create temporary file: {error}", path.display()))?;
    temporary
        .as_file()
        .set_permissions(permissions)
        .map_err(|error| format!("{}: cannot preserve permissions: {error}", path.display()))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file_mut().flush())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("{}: cannot write temporary file: {error}", path.display()))?;
    temporary.persist(path).map_err(|error| {
        format!(
            "{}: cannot atomically replace file: {}",
            path.display(),
            error.error
        )
    })?;
    Ok(())
}

fn line_count(value: &str) -> usize {
    value.lines().count()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::NaiveDate;
    use tempfile::tempdir;

    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn position_creates_block_without_rewriting_body() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("note.md");
        let original = "---\ntype: note\n---\n\n# Body\nNo final newline";
        fs::write(&path, original).unwrap();
        let result = append_note_event(&path, 6, date("2026-08-14")).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert_eq!(result.pass, 1);
        assert!(updated.starts_with(original));
        assert!(updated.contains("| 2026-08-14 |        6 |    1 |"));
    }

    #[test]
    fn position_in_middle_preserves_following_content_and_adjusts_line() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("note.md");
        let original = "---\ntype: note\npriority: 1\n---\n\n<!-- HISTORY:BEGIN -->\n\n| Date | End Line | Pass |\n| --- | --- | --- |\n| 2026-08-01 | 2 | 1 |\n\n<!-- HISTORY:END -->\nAFTER EXACTLY\n";
        fs::write(&path, original).unwrap();
        let result = append_note_event(&path, 13, date("2026-08-14")).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert_eq!(result.stored_end_line, 14);
        assert!(updated.ends_with("AFTER EXACTLY\n"));
        assert!(updated.contains("| 2026-08-14 |       14 |    1 |"));
    }

    #[test]
    fn lower_position_increments_pass() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("note.md");
        fs::write(
            &path,
            "---\ntype: note\n---\n\n# One\n# Two\n# Three\n# Four\n# Five\n# Six\n# Seven\n# Eight\n\n<!-- HISTORY:BEGIN -->\n| Date | End Line | Pass |\n| --- | --- | --- |\n| 2026-08-01 | 10 | 1 |\n<!-- HISTORY:END -->\n",
        )
        .unwrap();
        let result = append_note_event(&path, 5, date("2026-08-14")).unwrap();
        assert_eq!(result.pass, 2);
        assert!(result.pass_incremented);
    }

    #[test]
    fn wrong_type_does_not_modify_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("card.md");
        let original = "---\ntype: card\n---\n## Front\nQ\n## Back\nA\n";
        fs::write(&path, original).unwrap();
        let error = append_note_event(&path, 1, date("2026-08-14")).unwrap_err();
        assert!(error.contains("expected type 'note', found 'card'"));
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn rate_accepts_missing_priority_and_writes_no_cached_state() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("card.md");
        fs::write(&path, "---\ntype: card\n---\n## Front\nQ\n## Back\nA\n").unwrap();
        append_card_event(&path, 3, date("2026-08-14")).unwrap();
        let updated = fs::read_to_string(path).unwrap();
        assert!(updated.contains("| 2026-08-14 |      3 |"));
        for forbidden in ["stability:", "difficulty:", "due:", "interval:"] {
            assert!(!updated.contains(forbidden));
        }
    }

    #[test]
    fn earlier_card_date_does_not_modify_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("card.md");
        let original = concat!(
            "---\ntype: card\n---\n",
            "## Front\nQ\n## Back\nA\n",
            "<!-- HISTORY:BEGIN -->\n",
            "| Date | Rating |\n",
            "| --- | --- |\n",
            "| 2026-08-14 | 3 |\n",
            "<!-- HISTORY:END -->\n",
        );
        fs::write(&path, original).unwrap();

        let error = append_card_event(&path, 4, date("2026-08-13")).unwrap_err();
        assert!(error.contains("earlier than latest history date"));
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn position_inside_history_does_not_modify_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("note.md");
        let original = concat!(
            "---\ntype: note\n---\n",
            "<!-- HISTORY:BEGIN -->\n",
            "| Date | End Line | Pass |\n",
            "| --- | --- | --- |\n",
            "<!-- HISTORY:END -->\n",
        );
        fs::write(&path, original).unwrap();

        let error = append_note_event(&path, 5, date("2026-08-14")).unwrap_err();
        assert!(error.contains("inside the history block"));
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_edit_symbolic_links() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let target = directory.path().join("target.md");
        let link = directory.path().join("link.md");
        let original = "---\ntype: note\n---\nbody\n";
        fs::write(&target, original).unwrap();
        symlink(&target, &link).unwrap();

        let error = append_note_event(&link, 4, date("2026-08-14")).unwrap_err();
        assert!(error.contains("symbolic links cannot be edited"));
        assert_eq!(fs::read_to_string(target).unwrap(), original);
    }
}
