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
        .stdout("./maximum.md 99\n./minimum.md 0\n")
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
        .stdout("./note.md -\n")
        .stderr("");
}
