mod common;

use jiff::{ToSpan, Zoned};
use tempfile::tempdir;

use common::{columns_command, write_file};

fn date_from_today(days: i64) -> String {
    Zoned::now()
        .date()
        .checked_add(days.days())
        .unwrap()
        .to_string()
}

fn history_block(rows: &str) -> String {
    format!(
        "<!-- HISTORY:BEGIN -->\n\n| Date       | Rating |\n| ---------- | -----: |\n{rows}\n\n<!-- HISTORY:END -->"
    )
}

fn card_with_history(rows: &str) -> String {
    format!(
        "---\ntype: card\ndesired retention: 85\n---\n\n{}\n",
        history_block(rows)
    )
}

#[test]
fn renders_dashes_without_a_card_memory_state() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card-empty-history.md",
        &card_with_history(""),
    );
    write_file(
        directory.path(),
        "card-no-history.md",
        "---\ntype: card\ndesired retention: 85\n---\n",
    );
    write_file(
        directory.path(),
        "note.md",
        "---\ntype: note\npriority: 8\n---\n",
    );

    columns_command(directory.path(), &["predicted retention", "difficulty"])
        .assert()
        .success()
        .stdout("./card-empty-history.md - -\n./card-no-history.md - -\n./note.md - -\n")
        .stderr("");
}

#[test]
fn derives_memory_columns_from_each_initial_rating() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    for (name, rating) in [("again", 1), ("hard", 2), ("good", 3), ("easy", 4)] {
        write_file(
            directory.path(),
            &format!("{name}.md"),
            &card_with_history(&format!("| {today} | {rating}      |")),
        );
    }

    columns_command(directory.path(), &["predicted retention", "difficulty"])
        .assert()
        .success()
        .stdout(
            "./again.md 1.000 0.601\n\
         ./easy.md 1.000 0.000\n\
         ./good.md 1.000 0.124\n\
         ./hard.md 1.000 0.457\n",
        )
        .stderr("");
}

#[test]
fn replays_the_entire_history_and_applies_current_decay() {
    let directory = tempdir().unwrap();
    let first = date_from_today(-10);
    let second = date_from_today(-3);
    write_file(
        directory.path(),
        "card.md",
        &card_with_history(&format!("| {first} | 3      |\n| {second} | 2      |")),
    );

    columns_command(directory.path(), &["predicted retention", "difficulty"])
        .assert()
        .success()
        .stdout("./card.md 0.971 0.417\n")
        .stderr("");
}

#[test]
fn accepts_same_day_reviews_in_table_order() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "card.md",
        &card_with_history(&format!("| {today} | 1      |\n| {today} | 4      |")),
    );

    columns_command(directory.path(), &["predicted retention", "difficulty"])
        .assert()
        .success()
        .stdout("./card.md 1.000 0.467\n")
        .stderr("");
}

#[test]
fn rejects_malformed_history_rows() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    let yesterday = date_from_today(-1);
    let two_days_ago = date_from_today(-2);
    let tomorrow = date_from_today(1);
    let mut cases = vec![
        ("bad-date.md", "| 2025-02-29 | 3      |".to_owned()),
        (
            "decreasing-date.md",
            format!("| {yesterday} | 3      |\n| {two_days_ago} | 3      |"),
        ),
        ("extra-cell.md", format!("| {today} | 3 | extra |")),
        ("future-date.md", format!("| {tomorrow} | 3      |")),
        ("missing-cell.md", format!("| {today} |")),
        ("rating-above.md", format!("| {today} | 5      |")),
        ("rating-below.md", format!("| {today} | 0      |")),
        ("rating-float.md", format!("| {today} | 3.0    |")),
        ("rating-text.md", format!("| {today} | good   |")),
    ];
    cases.sort_by_key(|(name, _)| *name);
    for (name, rows) in &cases {
        write_file(directory.path(), name, &card_with_history(rows));
    }
    let expected = cases
        .iter()
        .map(|(name, _)| format!("./{name} ? ?\n"))
        .collect::<String>();

    columns_command(directory.path(), &["predicted retention", "difficulty"])
        .assert()
        .code(1)
        .stdout(expected)
        .stderr("");
}

#[test]
fn rejects_multiple_history_blocks() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    let block = history_block(&format!("| {today} | 3      |"));
    write_file(
        directory.path(),
        "card.md",
        &format!("---\ntype: card\ndesired retention: 85\n---\n\n{block}\n\n{block}\n"),
    );

    columns_command(directory.path(), &["predicted retention", "difficulty"])
        .assert()
        .code(1)
        .stdout("./card.md ? ?\n")
        .stderr("");
}

#[test]
fn rejects_an_unclosed_history_block() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "card.md",
        &format!(
            "---\ntype: card\ndesired retention: 85\n---\n\n\
             <!-- HISTORY:BEGIN -->\n\n\
             | Date       | Rating |\n\
             | ---------- | -----: |\n\
             | {today} | 3      |\n"
        ),
    );

    columns_command(directory.path(), &["predicted retention", "difficulty"])
        .assert()
        .code(1)
        .stdout("./card.md ? ?\n")
        .stderr("");
}

#[test]
fn unselected_malformed_history_does_not_affect_exit_status() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        &card_with_history("| not-a-date | impossible |"),
    );

    columns_command(directory.path(), &["type"])
        .assert()
        .success()
        .stdout("./card.md card\n")
        .stderr("");
}
