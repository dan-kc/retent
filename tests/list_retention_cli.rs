mod common;

use indoc::indoc;
use tempfile::tempdir;

use common::{columns_command, write_file};

#[test]
fn accepts_desired_retention_boundaries() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "maximum.md",
        indoc! {r#"
            ---
            type: card
            desired retention: 99
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );
    write_file(
        directory.path(),
        "minimum.md",
        indoc! {r#"
            ---
            type: card
            desired retention: 0
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );

    columns_command(directory.path(), &["desired retention"])
        .assert()
        .success()
        .stdout(b"99\t./maximum.md\x000\t./minimum.md\x00".as_slice())
        .stderr("");
}

#[test]
fn bare_card_uses_inherited_type_and_desired_retention() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        ".defaults.yaml",
        "type: card\ndesired retention: 75\n",
    );
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            <!-- FRONT:BEGIN -->
            Question.
            <!-- FRONT:END -->
        "#},
    );

    columns_command(directory.path(), &["type", "desired retention", "score"])
        .assert()
        .success()
        .stdout(b"card\t75\t0.500\t./card.md\x00".as_slice())
        .stderr("");
}

#[test]
fn file_type_override_keeps_other_inherited_fields() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        ".defaults.yaml",
        "type: note\npriority: 6\ndesired retention: 80\n",
    );
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );

    columns_command(directory.path(), &["type", "priority", "desired retention"])
        .assert()
        .success()
        .stdout(b"card\t-\t80\t./card.md\x00".as_slice())
        .stderr("");
}

#[test]
fn rejects_full_desired_retention() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            desired retention: 100
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );

    columns_command(directory.path(), &["desired retention"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_missing_desired_retention() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );

    columns_command(directory.path(), &["desired retention"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_desired_retention_below_minimum() {
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

    columns_command(directory.path(), &["desired retention"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_desired_retention_above_maximum() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            desired retention: 101
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );

    columns_command(directory.path(), &["desired retention"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_quoted_desired_retention() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            desired retention: "85"
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );

    columns_command(directory.path(), &["desired retention"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_floating_point_desired_retention() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            desired retention: 85.0
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );

    columns_command(directory.path(), &["desired retention"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn note_desired_retention_always_renders_a_dash() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            type: note
            priority: 8
            desired retention: 400
            ---
        "#},
    );

    columns_command(directory.path(), &["desired retention"])
        .assert()
        .success()
        .stdout(b"-\t./note.md\x00".as_slice())
        .stderr("");
}
