//! Stable human-readable command output.

use crate::scheduling::queue::{Details, QueueItem};

/// Render the ranked queue and its type-specific detail lines.
pub fn queue(items: &[QueueItem]) -> String {
    let mut output =
        String::from("RANK TYPE PRIO STATUS DUE AGE INTERVAL PRESSURE P-WEIGHT SCORE PATH\n");
    for (index, item) in items.iter().enumerate() {
        let interval = item
            .metrics
            .interval_days
            .map(|days| format!("{days}d"))
            .unwrap_or_else(|| "-".to_owned());
        output.push_str(&format!(
            "{} {} {} {} {} {}d {} {:.3} {:.3} {:.3} {}\n",
            index + 1,
            item.element_type,
            item.priority,
            item.metrics.status,
            item.metrics.due_date,
            item.metrics.age_days,
            interval,
            item.metrics.pressure,
            item.priority_weight,
            item.score,
            item.path.display()
        ));
        match &item.details {
            Details::Card(card) => {
                if let (Some(retrievability), Some(stability), Some(difficulty), Some(rating)) = (
                    card.retrievability,
                    card.stability,
                    card.difficulty,
                    card.last_rating,
                ) {
                    output.push_str(&format!(
                        "  card: R={retrievability:.2} S={stability:.1} D={difficulty:.1} last_rating={rating}\n"
                    ));
                } else {
                    output.push_str("  card: new\n");
                }
            }
            Details::Note(note) => {
                if let Some(pass) = note.pass {
                    output.push_str(&format!(
                        "  note: pass={pass} reads_in_pass={} recent_exposure={:.2} resume_line={} (non-ranking)\n",
                        note.reads_in_pass,
                        note.recent_exposure,
                        note.resume_line.unwrap_or(0)
                    ));
                } else {
                    output.push_str("  note: new\n");
                }
            }
        }
    }
    output
}
