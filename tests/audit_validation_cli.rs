mod common;

use indoc::{formatdoc, indoc};
use jiff::{SignedDuration, Zoned};
use tempfile::tempdir;

use common::{audit_command, write_file};

fn assert_note_history_error(table: &str, reason: &str) {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        &formatdoc! {r#"
            ---
            type: note
            priority: 8
            ---

            <!-- HISTORY:BEGIN -->

            {table}

            <!-- HISTORY:END -->
        "#},
    );

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout(format!("./note.md\t{reason}\n"))
        .stderr("");
}

fn assert_card_error(body: &str, reason: &str) {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        &formatdoc! {r#"
            ---
            type: card
            desired retention: 85
            ---

            {body}
        "#},
    );

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout(format!("./card.md\t{reason}\n"))
        .stderr("");
}

#[test]
fn quoted_priority_reports_its_required_yaml_type() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            type: note
            priority: "8"
            ---
        "#},
    );

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./note.md\tpriority must be an unquoted integer\n")
        .stderr("");
}

#[test]
fn negative_retention_reports_its_allowed_range() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            desired retention: -1
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./card.md\tdesired retention must be from 0 to 99\n")
        .stderr("");
}

#[test]
fn note_history_requires_a_table() {
    assert_note_history_error("", "note history table is missing");
}

#[test]
fn note_history_requires_the_exact_header() {
    assert_note_history_error(
        "| Date | Pass |\n| ---- | ---: |",
        "note history header must be Date | End Line | Pass",
    );
}

#[test]
fn note_history_requires_a_valid_separator() {
    assert_note_history_error(
        "| Date | End Line | Pass |\n| text | -------- | ---- |",
        "note history separator is invalid",
    );
}

#[test]
fn note_history_rows_must_be_contiguous() {
    assert_note_history_error(
        "| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| 2026-01-01 | 1 | 0 |\n\n| 2026-01-02 | 1 | 0 |",
        "note history rows must be contiguous",
    );
}

#[test]
fn note_history_row_requires_the_exact_cell_count() {
    assert_note_history_error(
        "| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| 2026-01-01 | 0 |",
        "note history row 1 must contain Date, End Line, and Pass",
    );
}

#[test]
fn note_history_date_requires_the_exact_format() {
    assert_note_history_error(
        "| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| 01/01/2026 | 1 | 0 |",
        "note history row 1 date must use YYYY-MM-DD",
    );
}

#[test]
fn note_history_date_must_be_a_real_calendar_date() {
    assert_note_history_error(
        "| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| 2025-02-29 | 1 | 0 |",
        "note history row 1 date is not a valid calendar date",
    );
}

#[test]
fn note_history_date_must_not_be_in_the_future() {
    let tomorrow = (Zoned::now() + SignedDuration::from_hours(24))
        .date()
        .to_string();
    assert_note_history_error(
        &format!("| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| {tomorrow} | 1 | 0 |"),
        "note history row 1 date must not be after today",
    );
}

#[test]
fn note_history_dates_must_be_non_decreasing() {
    assert_note_history_error(
        "| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| 2026-01-02 | 1 | 0 |\n| 2026-01-01 | 1 | 0 |",
        "note history dates must be non-decreasing",
    );
}

#[test]
fn note_history_end_line_must_be_a_non_negative_integer() {
    assert_note_history_error(
        "| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| 2026-01-01 | -1 | 0 |",
        "note history row 1 End Line must be a non-negative integer",
    );
}

#[test]
fn note_history_pass_must_be_a_non_negative_integer() {
    assert_note_history_error(
        "| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| 2026-01-01 | 1 | no |",
        "note history row 1 Pass must be a non-negative integer",
    );
}

#[test]
fn note_history_passes_must_be_non_decreasing() {
    assert_note_history_error(
        "| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| 2026-01-01 | 1 | 2 |\n| 2026-01-02 | 1 | 1 |",
        "note history passes must be non-decreasing",
    );
}

#[test]
fn card_history_requires_the_exact_header() {
    assert_card_error(
        indoc! {r#"
            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->

            <!-- HISTORY:BEGIN -->
            | Date | Score |
            | ---- | ----- |
            <!-- HISTORY:END -->
        "#},
        "card history header must be Date | Rating",
    );
}

#[test]
fn card_history_rating_must_be_an_integer_in_range() {
    assert_card_error(
        indoc! {r#"
            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->

            <!-- HISTORY:BEGIN -->
            | Date | Rating |
            | ---- | ------ |
            | 2026-01-01 | 5 |
            <!-- HISTORY:END -->
        "#},
        "card history row 1 Rating must be an integer from 1 to 4",
    );
}

#[test]
fn a_front_block_may_be_empty_and_have_surrounding_whitespace() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            desired retention: 85
            ---

               <!-- FRONT:BEGIN -->
               <!-- FRONT:END -->
        "#},
    );

    audit_command(directory.path())
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn nested_front_blocks_are_invalid() {
    assert_card_error(
        indoc! {r#"
            <!-- FRONT:BEGIN -->
            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
            <!-- FRONT:END -->
        "#},
        "card front block is nested",
    );
}

#[test]
fn multiple_front_blocks_are_invalid() {
    assert_card_error(
        indoc! {r#"
            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
        "card has multiple front blocks",
    );
}

#[test]
fn optional_back_block_structure_is_ignored() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            desired retention: 85
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
            <!-- BACK:END -->
        "#},
    );

    audit_command(directory.path())
        .assert()
        .success()
        .stdout("")
        .stderr("");
}
