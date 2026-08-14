//! Deterministic global ranking across notes and cards.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::diagnostics::Diagnostic;
use crate::discover::{markdown_files, relative};
use crate::document::{Classification, ElementType, History, parse};

use super::ScheduleMetrics;
use super::card::{CardSchedule, CardSchedulerConfig};
use super::note::{NoteSchedule, NoteSchedulerConfig};

/// Type-specific queue details.
#[derive(Debug, Clone)]
pub enum Details {
    Note(NoteSchedule),
    Card(CardSchedule),
}

impl Details {
    fn metrics(&self) -> &ScheduleMetrics {
        match self {
            Self::Note(schedule) => &schedule.metrics,
            Self::Card(schedule) => &schedule.metrics,
        }
    }
}

/// One fully scored queue row.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub path: PathBuf,
    pub element_type: ElementType,
    pub priority: u8,
    pub metrics: ScheduleMetrics,
    pub priority_weight: f64,
    pub score: f64,
    pub details: Details,
}

/// Queue filtering and visibility options.
#[derive(Debug, Clone, Copy, Default)]
pub struct QueueOptions {
    pub notes_only: bool,
    pub cards_only: bool,
    pub include_upcoming: bool,
    pub limit: Option<usize>,
}

/// Valid ranked items plus invalid files skipped during the scan.
pub struct QueueResult {
    pub items: Vec<QueueItem>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Scan, reconstruct, score, and sort a vault queue.
pub fn build(root: &Path, as_of: NaiveDate, options: QueueOptions) -> Result<QueueResult, String> {
    let mut items = Vec::new();
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
        match document.classification() {
            Classification::Invalid => {
                diagnostics.extend(document.diagnostics);
                continue;
            }
            Classification::Missing => continue,
            Classification::Valid => {}
        }
        let element_type = document.metadata.element_type.unwrap();
        if (options.notes_only && element_type != ElementType::Note)
            || (options.cards_only && element_type != ElementType::Card)
        {
            continue;
        }
        let priority = document.metadata.priority.unwrap();
        let details = schedule_details(element_type, document.history.as_ref(), priority, as_of)?;
        let metrics = details.metrics().clone();
        if !options.include_upcoming && metrics.status == super::Status::Upcoming {
            continue;
        }
        let priority_weight = priority_weight(priority);
        let score = priority_weight * metrics.pressure;
        items.push(QueueItem {
            path: relative_path,
            element_type,
            priority,
            metrics,
            priority_weight,
            score,
            details,
        });
    }

    items.sort_by(compare);
    if let Some(limit) = options.limit {
        items.truncate(limit);
    }
    Ok(QueueResult { items, diagnostics })
}

fn schedule_details(
    element_type: ElementType,
    history: Option<&History>,
    priority: u8,
    as_of: NaiveDate,
) -> Result<Details, String> {
    match element_type {
        ElementType::Note => {
            let events = match history {
                Some(History::Note(events)) => events.as_slice(),
                None => &[],
                Some(History::Card(_)) => {
                    return Err("card history found in note document".to_owned());
                }
            };
            Ok(Details::Note(super::note::schedule(
                events,
                priority,
                as_of,
                NoteSchedulerConfig::default(),
            )))
        }
        ElementType::Card => {
            let events = match history {
                Some(History::Card(events)) => events.as_slice(),
                None => &[],
                Some(History::Note(_)) => {
                    return Err("note history found in card document".to_owned());
                }
            };
            Ok(Details::Card(super::card::schedule(
                events,
                as_of,
                CardSchedulerConfig::default(),
            )?))
        }
    }
}

fn priority_weight(priority: u8) -> f64 {
    10.0_f64.powf((50.0 - priority as f64) / 50.0)
}

fn compare(left: &QueueItem, right: &QueueItem) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.metrics.due_date.cmp(&right.metrics.due_date))
        .then_with(|| left.priority.cmp(&right.priority))
        .then_with(|| left.path.cmp(&right.path))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn write_note(root: &Path, name: &str, priority: u8, history: &str) {
        fs::write(
            root.join(name),
            format!("---\ntype: note\npriority: {priority}\n---\n{history}"),
        )
        .unwrap();
    }

    fn write_card(root: &Path, name: &str, priority: u8) {
        fs::write(
            root.join(name),
            format!("---\ntype: card\npriority: {priority}\n---\n## Front\nQ\n## Back\nA\n"),
        )
        .unwrap();
    }

    #[test]
    fn priority_weight_has_expected_midpoint() {
        assert_eq!(priority_weight(50), 1.0);
        assert!(priority_weight(0) > priority_weight(100));
    }

    #[test]
    fn upcoming_items_are_only_included_when_requested() {
        let directory = tempdir().unwrap();
        let history = concat!(
            "<!-- HISTORY:BEGIN -->\n",
            "| Date | End Line | Pass |\n",
            "| --- | --- | --- |\n",
            "| 2026-08-14 | 10 | 1 |\n",
            "<!-- HISTORY:END -->\n",
        );
        write_note(directory.path(), "note.md", 100, history);

        let hidden = build(
            directory.path(),
            date("2026-08-14"),
            QueueOptions::default(),
        )
        .unwrap();
        assert!(hidden.items.is_empty());

        let visible = build(
            directory.path(),
            date("2026-08-14"),
            QueueOptions {
                include_upcoming: true,
                ..QueueOptions::default()
            },
        )
        .unwrap();
        assert_eq!(visible.items.len(), 1);
        assert_eq!(
            visible.items[0].metrics.status,
            super::super::Status::Upcoming
        );
    }

    #[test]
    fn type_filters_select_the_requested_documents() {
        let directory = tempdir().unwrap();
        write_note(directory.path(), "note.md", 10, "");
        write_card(directory.path(), "card.md", 10);

        for (options, expected) in [
            (
                QueueOptions {
                    notes_only: true,
                    ..QueueOptions::default()
                },
                ElementType::Note,
            ),
            (
                QueueOptions {
                    cards_only: true,
                    ..QueueOptions::default()
                },
                ElementType::Card,
            ),
        ] {
            let result = build(directory.path(), date("2026-08-14"), options).unwrap();
            assert_eq!(result.items.len(), 1);
            assert_eq!(result.items[0].element_type, expected);
        }
    }

    #[test]
    fn ranks_by_priority_then_path_before_applying_limit() {
        let directory = tempdir().unwrap();
        write_note(directory.path(), "b.md", 20, "");
        write_note(directory.path(), "low.md", 80, "");
        write_note(directory.path(), "a.md", 20, "");

        let result = build(
            directory.path(),
            date("2026-08-14"),
            QueueOptions {
                limit: Some(2),
                ..QueueOptions::default()
            },
        )
        .unwrap();
        let paths: Vec<_> = result
            .items
            .iter()
            .map(|item| item.path.as_path())
            .collect();
        assert_eq!(paths, [Path::new("a.md"), Path::new("b.md")]);
    }

    #[test]
    fn skips_missing_documents_and_reports_invalid_utf8() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("missing.md"), "plain Markdown\n").unwrap();
        fs::write(directory.path().join("invalid.md"), [0xff]).unwrap();

        let result = build(
            directory.path(),
            date("2026-08-14"),
            QueueOptions::default(),
        )
        .unwrap();
        assert!(result.items.is_empty());
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "utf8-invalid");
    }
}
