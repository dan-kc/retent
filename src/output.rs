//! Human-readable and pipe-friendly command output.

use comfy_table::presets::UTF8_HORIZONTAL_ONLY;
use comfy_table::{
    Attribute, Cell, CellAlignment, Color, ColumnConstraint, ContentArrangement, Row, Table, Width,
};

use crate::scheduling::Status;
use crate::scheduling::queue::{Details, QueueItem};

/// Render scheduled list items as a terminal-width-aware table.
pub fn queue(items: &[QueueItem], wrap: bool) -> String {
    queue_with_width(items, wrap, None)
}

fn queue_with_width(items: &[QueueItem], wrap: bool, width: Option<u16>) -> String {
    let mut table = Table::new();
    table
        .load_style(UTF8_HORIZONTAL_ONLY)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header([
            heading("#", CellAlignment::Right),
            heading("Type", CellAlignment::Left),
            heading("Prio", CellAlignment::Right),
            heading("Status", CellAlignment::Left),
            heading("Due", CellAlignment::Left),
            heading("Age", CellAlignment::Right),
            heading("Int", CellAlignment::Right),
            heading("Score", CellAlignment::Right),
            heading("Path", CellAlignment::Left),
        ]);
    if let Some(width) = width {
        table.set_width(width);
    }
    for (index, column) in table.column_iter_mut().enumerate() {
        column.set_padding((0, 0));
        if index < 8 {
            column.set_constraint(ColumnConstraint::ContentWidth);
        } else {
            column.set_constraint(ColumnConstraint::LowerBoundary(Width::Fixed(20)));
        }
    }

    for (index, item) in items.iter().enumerate() {
        let interval = item
            .metrics
            .interval_days
            .map(|days| format!("{days}d"))
            .unwrap_or_else(|| "-".to_owned());
        let mut row = Row::from([
            right(index + 1),
            Cell::new(item.element_type),
            right(item.priority),
            status(item.metrics.status),
            Cell::new(item.metrics.due_date),
            right(format!("{}d", item.metrics.age_days)),
            right(interval),
            right(format!("{:.3}", item.score))
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold),
            Cell::new(item.path.display()),
        ]);
        if !wrap {
            row.max_height(1);
        }
        table.add_row(row);
    }

    let rank_width = format!("#{}", items.len()).len();
    let mut detail_output = String::new();
    for (index, item) in items.iter().enumerate() {
        detail_output.push_str(&format!(
            "{:>rank_width$} └─ {}\n",
            format!("#{}", index + 1),
            details(item),
        ));
    }

    if items.is_empty() {
        format!("{table}\n")
    } else {
        format!("{table}\n\n{detail_output}")
    }
}

/// Render one root-relative path per scheduled item.
pub fn paths(items: &[QueueItem]) -> String {
    let mut output = String::new();
    for item in items {
        output.push_str(&format!("{}\n", item.path.display()));
    }
    output
}

/// Render one tab-separated record per item for downstream programs.
///
/// Fields are: rank, type, priority, status, due date, age days, interval
/// days, score, and path. A missing interval is represented by an empty field.
pub fn queue_plain(items: &[QueueItem]) -> String {
    let mut output = String::new();
    for (index, item) in items.iter().enumerate() {
        let interval = item
            .metrics
            .interval_days
            .map(|days| days.to_string())
            .unwrap_or_default();
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{}\n",
            index + 1,
            item.element_type,
            item.priority,
            item.metrics.status,
            item.metrics.due_date,
            item.metrics.age_days,
            interval,
            item.score,
            item.path.display(),
        ));
    }
    output
}

fn heading(value: &str, alignment: CellAlignment) -> Cell {
    Cell::new(value)
        .set_alignment(alignment)
        .add_attribute(Attribute::Bold)
}

fn right(value: impl ToString) -> Cell {
    Cell::new(value).set_alignment(CellAlignment::Right)
}

fn status(value: Status) -> Cell {
    let color = match value {
        Status::New => Color::Green,
        Status::Upcoming => Color::Blue,
        Status::Due => Color::Yellow,
        Status::Overdue => Color::Red,
    };
    Cell::new(value).fg(color)
}

fn details(item: &QueueItem) -> String {
    match &item.details {
        Details::Card(card) => {
            if let (Some(retrievability), Some(stability), Some(difficulty), Some(rating)) = (
                card.retrievability,
                card.stability,
                card.difficulty,
                card.last_rating,
            ) {
                format!(
                    "Retrievability {retrievability:.2} · stability {stability:.1}d · \
                     difficulty {difficulty:.1} · last rating {rating}"
                )
            } else {
                "Not reviewed yet".to_owned()
            }
        }
        Details::Note(note) => {
            if let Some(pass) = note.pass {
                let reads = note.reads_in_pass;
                let noun = if reads == 1 { "read" } else { "reads" };
                let mut detail = format!(
                    "Pass {pass} · {reads} {noun} this pass · recent exposure {:.2}",
                    note.recent_exposure
                );
                if let Some(line) = note.resume_line {
                    detail.push_str(&format!(" · resume at line {line}"));
                }
                detail
            } else {
                "Not read yet".to_owned()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::NaiveDate;

    use crate::document::ElementType;
    use crate::scheduling::note::NoteSchedule;
    use crate::scheduling::{ScheduleMetrics, Status};

    use super::*;

    #[test]
    fn queue_uses_comfy_table_and_only_displays_the_final_score() {
        let item = item();
        let output = queue(std::slice::from_ref(&item), false);

        assert!(output.contains("═"));
        assert!(output.contains("Prio"));
        assert!(output.contains("Score"));
        assert!(output.contains("2296.685"));
        assert!(!output.contains("Pressure"));
        assert!(!output.contains("P-weight"));
        assert_eq!(
            details(&item),
            "Pass 1 · 1 read this pass · recent exposure 0.00 · resume at line 4"
        );
    }

    #[test]
    fn plain_queue_is_headerless_tsv_with_one_line_per_item() {
        assert_eq!(
            queue_plain(&[item()]),
            "1\tnote\t1\toverdue\t2024-08-18\t728\t2\t2296.685\t\
             example-vault/study-note.md\n"
        );
    }

    #[test]
    fn paths_only_contains_one_relative_path_per_item() {
        let mut second = item();
        second.path = PathBuf::from("another/card.md");
        assert_eq!(
            paths(&[item(), second]),
            "example-vault/study-note.md\nanother/card.md\n"
        );
    }

    #[test]
    fn table_formats_more_than_five_hundred_items_without_truncation() {
        let items = (1..=501)
            .map(|index| {
                let mut item = item();
                item.path = PathBuf::from(format!("item-{index}.md"));
                item
            })
            .collect::<Vec<_>>();

        let output = queue(&items, false);
        assert_eq!(output.matches(".md").count(), 501);
        assert!(
            output
                .lines()
                .any(|line| line.contains("501") && line.contains("item-501.md"))
        );
        assert!(output.contains("#501"));
    }

    #[test]
    fn narrow_tables_truncate_long_paths_instead_of_wrapping_rows() {
        let mut item = item();
        item.path =
            PathBuf::from("collection-2026-08-14@19-02-55/23521b46828533e15dd5424fda38a94a.md");

        let output = queue_with_width(&[item], false, Some(80));
        let table = output.split_once("\n\n").unwrap().0;
        assert_eq!(table.lines().count(), 5);
        assert!(table.contains('…'));
    }

    #[test]
    fn wrap_option_preserves_long_cells_across_physical_lines() {
        let mut item = item();
        let path = "collection-2026-08-14@19-02-55/23521b46828533e15dd5424fda38a94a.md";
        item.path = PathBuf::from(path);

        let output = queue_with_width(&[item], true, Some(80));
        let table = output.split_once("\n\n").unwrap().0;
        let collapsed = table
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        assert!(table.lines().count() > 5);
        assert!(!table.contains('…'));
        assert!(collapsed.contains(path));
    }

    fn item() -> QueueItem {
        let metrics = ScheduleMetrics {
            status: Status::Overdue,
            last_date: Some(date("2024-08-16")),
            interval_days: Some(2),
            due_date: date("2024-08-18"),
            age_days: 728,
            pressure: 364.0,
        };
        QueueItem {
            path: PathBuf::from("example-vault/study-note.md"),
            element_type: ElementType::Note,
            priority: 1,
            metrics: metrics.clone(),
            priority_weight: 6.310,
            score: 2296.685,
            details: Details::Note(NoteSchedule {
                metrics,
                pass: Some(1),
                reads_in_pass: 1,
                recent_exposure: 0.0,
                resume_line: Some(4),
            }),
        }
    }

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }
}
