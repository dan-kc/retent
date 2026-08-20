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

fn card_with_history(desired_retention: u8, rows: &str) -> String {
    format!(
        "---\ntype: card\ndesired retention: {desired_retention}\n---\n\n<!-- FRONT:BEGIN -->\n<!-- FRONT:END -->\n\n{}\n",
        history_block(rows)
    )
}

#[test]
fn new_cards_have_a_neutral_score() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "empty-history.md",
        &card_with_history(85, ""),
    );
    write_file(
        directory.path(),
        "no-history.md",
        "---\ntype: card\ndesired retention: 85\n---\n\n<!-- FRONT:BEGIN -->\n<!-- FRONT:END -->\n",
    );
    write_file(
        directory.path(),
        "not-ranked.md",
        "---\ntype: card\ndesired retention: 0\n---\n\n<!-- FRONT:BEGIN -->\n<!-- FRONT:END -->\n",
    );
    write_file(directory.path(), "other.md", "No frontmatter.\n");

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout(
            "./empty-history.md 0.500\n\
             ./no-history.md 0.500\n\
             ./not-ranked.md 0.000\n\
             ./other.md -\n",
        )
        .stderr("");
}

#[test]
fn a_reviewed_card_starts_with_a_zero_score() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "card.md",
        &card_with_history(85, &format!("| {today} | 3      |")),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("./card.md 0.000\n")
        .stderr("");
}

#[test]
fn score_uses_the_unrounded_fsrs_target_interval() {
    let directory = tempdir().unwrap();
    let review_date = date_from_today(-10);
    write_file(
        directory.path(),
        "card.md",
        &card_with_history(85, &format!("| {review_date} | 3      |")),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("./card.md 0.695\n")
        .stderr("");
}

#[test]
fn higher_desired_retention_produces_a_higher_score() {
    let directory = tempdir().unwrap();
    let review_date = date_from_today(-10);
    for desired_retention in [0, 85, 99] {
        write_file(
            directory.path(),
            &format!("retention-{desired_retention:02}.md"),
            &card_with_history(desired_retention, &format!("| {review_date} | 3      |")),
        );
    }

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout(
            "./retention-00.md 0.000\n\
             ./retention-85.md 0.695\n\
             ./retention-99.md 0.984\n",
        )
        .stderr("");
}

#[test]
fn score_and_memory_columns_share_the_full_history() {
    let directory = tempdir().unwrap();
    let first = date_from_today(-10);
    let second = date_from_today(-3);
    write_file(
        directory.path(),
        "card.md",
        &card_with_history(85, &format!("| {first} | 3      |\n| {second} | 2      |")),
    );

    columns_command(
        directory.path(),
        &["score", "predicted retention", "difficulty"],
    )
    .assert()
    .success()
    .stdout("./card.md 0.102 97 0.417\n")
    .stderr("");
}

#[test]
fn invalid_desired_retention_makes_the_score_invalid() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "full.md",
        "---\ntype: card\ndesired retention: 100\n---\n\n<!-- FRONT:BEGIN -->\n<!-- FRONT:END -->\n",
    );
    write_file(
        directory.path(),
        "missing.md",
        "---\ntype: card\n---\n\n<!-- FRONT:BEGIN -->\n<!-- FRONT:END -->\n",
    );
    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn zero_retention_card_score_requires_valid_history() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        &card_with_history(0, "| invalid | row |"),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn malformed_history_makes_the_score_invalid() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "card.md",
        &card_with_history(85, &format!("| {today} | 5      |")),
    );

    columns_command(directory.path(), &["score"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn cards_reject_note_history_schema() {
    let directory = tempdir().unwrap();
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "card.md",
        &format!(
            "---\ntype: card\ndesired retention: 85\n---\n\n\
             <!-- FRONT:BEGIN -->\n\n\
             Question\n\n\
             <!-- FRONT:END -->\n\n\
             <!-- HISTORY:BEGIN -->\n\n\
             | Date       | End Line | Pass |\n\
             | ---------- | -------: | ---- |\n\
             | {today} | 0        | 0    |\n\n\
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
fn cards_reject_noncontiguous_history_rows() {
    let directory = tempdir().unwrap();
    let yesterday = date_from_today(-1);
    let today = date_from_today(0);
    write_file(
        directory.path(),
        "card.md",
        &format!(
            "---\ntype: card\ndesired retention: 85\n---\n\n\
             <!-- FRONT:BEGIN -->\n\n\
             Question\n\n\
             <!-- FRONT:END -->\n\n\
             <!-- HISTORY:BEGIN -->\n\n\
             | Date       | Rating |\n\
             | ---------- | -----: |\n\
             | {yesterday} | 3      |\n\n\
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
