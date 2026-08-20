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
        "<!-- HISTORY:BEGIN -->\n\n| Date       | End Line | Pass |\n| ---------- | -------: | ---- |\n{rows}\n\n<!-- HISTORY:END -->"
    )
}

fn note_with_history(priority: u8, rows: &str) -> String {
    format!(
        "---\ntype: note\npriority: {priority}\n---\n\n{}\n",
        history_block(rows)
    )
}

#[test]
fn notes_without_exposure_score_from_priority() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "empty-history.md",
        &note_with_history(8, ""),
    );
    for priority in [0, 5, 10] {
        write_file(
            directory.path(),
            &format!("priority-{priority:02}.md"),
            &format!("---\ntype: note\npriority: {priority}\n---\n"),
        );
    }

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout(
            "./empty-history.md 0.800\n\
             ./priority-00.md 0.000\n\
             ./priority-05.md 0.500\n\
             ./priority-10.md 1.000\n",
        )
        .stderr("");
}

#[test]
fn every_recent_exposure_contributes_to_the_score() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "one.md",
        &note_with_history(10, &format!("| {today} | 10       | 0    |")),
    );
    write_file(
        directory.path(),
        "two.md",
        &note_with_history(
            10,
            &format!("| {today} | 10       | 0    |\n| {today} | 20       | 0    |"),
        ),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("./one.md 0.500\n./two.md 0.333\n")
        .stderr("");
}

#[test]
fn each_exposures_pass_controls_its_decay() {
    let directory = tempdir().unwrap();
    let review_date = date_from_today(-2);
    for pass in 0..=2 {
        write_file(
            directory.path(),
            &format!("pass-{pass}.md"),
            &note_with_history(10, &format!("| {review_date} | 10       | {pass}    |")),
        );
    }

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("./pass-0.md 0.800\n./pass-1.md 0.667\n./pass-2.md 0.586\n")
        .stderr("");
}

#[test]
fn score_uses_the_full_history_but_not_end_line() {
    let directory = tempdir().unwrap();
    let two_days_ago = date_from_today(-2);
    let yesterday = date_from_today(-1);
    let today = date_from_today(0);
    let dates_and_passes = format!(
        "| {two_days_ago} | {{first}} | 0 |\n\
         | {yesterday} | {{second}} | 0 |\n\
         | {today} | {{third}} | 2 |"
    );
    write_file(
        directory.path(),
        "large-end-lines.md",
        &note_with_history(
            10,
            &dates_and_passes
                .replace("{first}", "18446744073709551615")
                .replace("{second}", "999999999999")
                .replace("{third}", "500000000000"),
        ),
    );
    write_file(
        directory.path(),
        "small-end-lines.md",
        &note_with_history(
            10,
            &dates_and_passes
                .replace("{first}", "0")
                .replace("{second}", "1")
                .replace("{third}", "2"),
        ),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("./large-end-lines.md 0.364\n./small-end-lines.md 0.364\n")
        .stderr("");
}

#[test]
fn handles_a_very_large_pass_without_overflowing() {
    let directory = tempdir().unwrap();
    let yesterday = date_from_today(-1);
    write_file(
        directory.path(),
        "note.md",
        &note_with_history(
            10,
            &format!("| {yesterday} | 0        | 18446744073709551615 |"),
        ),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("./note.md 0.500\n")
        .stderr("");
}

#[test]
fn accepts_surrounding_whitespace_on_history_markers() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "note.md",
        &format!(
            "---\ntype: note\npriority: 10\n---\n\n\
             \t<!-- HISTORY:BEGIN -->  \n\n\
             | Date       | End Line | Pass |\n\
             | ---------- | -------: | ---- |\n\
             | {today} | 0        | 0    |\n\n\
             \t<!-- HISTORY:END -->  \n"
        ),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("./note.md 0.500\n")
        .stderr("");
}

#[test]
fn rejects_malformed_note_history_rows() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    let yesterday = date_from_today(-1);
    let two_days_ago = date_from_today(-2);
    let tomorrow = date_from_today(1);
    let mut cases = vec![
        (
            "decreasing-date.md",
            format!("| {yesterday} | 1 | 0 |\n| {two_days_ago} | 2 | 0 |"),
        ),
        (
            "decreasing-pass.md",
            format!("| {yesterday} | 1 | 1 |\n| {today} | 2 | 0 |"),
        ),
        ("end-line-float.md", format!("| {today} | 1.0 | 0 |")),
        ("end-line-negative.md", format!("| {today} | -1 | 0 |")),
        ("end-line-text.md", format!("| {today} | end | 0 |")),
        ("extra-cell.md", format!("| {today} | 1 | 0 | extra |")),
        ("future-date.md", format!("| {tomorrow} | 1 | 0 |")),
        ("invalid-date.md", "| 2025-02-29 | 1 | 0 |".to_owned()),
        ("missing-cell.md", format!("| {today} | 1 |")),
        ("pass-float.md", format!("| {today} | 1 | 0.0 |")),
        ("pass-negative.md", format!("| {today} | 1 | -1 |")),
        ("pass-text.md", format!("| {today} | 1 | first |")),
    ];
    cases.sort_by_key(|(name, _)| *name);
    for (name, rows) in &cases {
        write_file(directory.path(), name, &note_with_history(10, rows));
    }
    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_malformed_note_history_blocks() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    let block = history_block(&format!("| {today} | 1        | 0    |"));
    write_file(
        directory.path(),
        "multiple.md",
        &format!("---\ntype: note\npriority: 10\n---\n\n{block}\n\n{block}\n"),
    );
    write_file(
        directory.path(),
        "unclosed.md",
        &format!(
            "---\ntype: note\npriority: 10\n---\n\n\
             <!-- HISTORY:BEGIN -->\n\n\
             | Date       | End Line | Pass |\n\
             | ---------- | -------: | ---- |\n\
             | {today} | 1        | 0    |\n"
        ),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_nested_history_blocks() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "note.md",
        &format!(
            "---\ntype: note\npriority: 10\n---\n\n\
             <!-- HISTORY:BEGIN -->\n\
             <!-- HISTORY:BEGIN -->\n\n\
             | Date       | End Line | Pass |\n\
             | ---------- | -------: | ---- |\n\
             | {today} | 0        | 0    |\n\n\
             <!-- HISTORY:END -->\n\
             <!-- HISTORY:END -->\n"
        ),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn notes_reject_card_history_schema() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "note.md",
        &format!(
            "---\ntype: note\npriority: 10\n---\n\n\
             <!-- HISTORY:BEGIN -->\n\n\
             | Date       | Rating |\n\
             | ---------- | -----: |\n\
             | {today} | 3      |\n\n\
             <!-- HISTORY:END -->\n"
        ),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn notes_reject_noncontiguous_history_rows() {
    let directory = tempdir().unwrap();
    let yesterday = date_from_today(-1);
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "note.md",
        &note_with_history(
            10,
            &format!("| {yesterday} | 0 | 0 |\n\n| {today} | 1 | 1 |"),
        ),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn invalid_priority_makes_the_note_score_invalid() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "above.md",
        "---\ntype: note\npriority: 11\n---\n",
    );
    write_file(directory.path(), "missing.md", "---\ntype: note\n---\n");

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn zero_priority_note_score_requires_valid_history() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        &note_with_history(0, "| invalid | history | row |"),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn unselected_malformed_note_history_is_still_skipped() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        &note_with_history(8, "| not-a-date | impossible | invalid |"),
    );

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}
