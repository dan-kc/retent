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
        let details = match (element_type, document.history.as_ref()) {
            (ElementType::Note, Some(History::Note(events))) => Details::Note(
                super::note::schedule(events, priority, as_of, NoteSchedulerConfig::default()),
            ),
            (ElementType::Note, None) => Details::Note(super::note::schedule(
                &[],
                priority,
                as_of,
                NoteSchedulerConfig::default(),
            )),
            (ElementType::Card, Some(History::Card(events))) => Details::Card(
                super::card::schedule(events, as_of, CardSchedulerConfig::default())?,
            ),
            (ElementType::Card, None) => Details::Card(super::card::schedule(
                &[],
                as_of,
                CardSchedulerConfig::default(),
            )?),
            _ => continue,
        };
        let metrics = match &details {
            Details::Note(schedule) => schedule.metrics.clone(),
            Details::Card(schedule) => schedule.metrics.clone(),
        };
        if !options.include_upcoming && metrics.status == super::Status::Upcoming {
            continue;
        }
        let priority_weight = 10.0_f64.powf((50.0 - priority as f64) / 50.0);
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
    #[test]
    fn priority_weight_has_expected_midpoint() {
        assert_eq!(10.0_f64.powf((50.0 - 50.0) / 50.0), 1.0);
    }
}
