mod common;

use indoc::indoc;
use tempfile::tempdir;

use common::{columns_command, write_file};

#[test]
fn accepts_priority_boundaries() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "maximum.md",
        indoc! {r#"
            ---
            type: note
            priority: 10
            ---
        "#},
    );
    write_file(
        directory.path(),
        "minimum.md",
        indoc! {r#"
            ---
            type: note
            priority: 0
            ---
        "#},
    );

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout("./maximum.md 10\n./minimum.md 0\n")
        .stderr("");
}

#[test]
fn rejects_missing_priority() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            type: note
            ---
        "#},
    );

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_priority_below_minimum() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            type: note
            priority: -1
            ---
        "#},
    );

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_priority_above_maximum() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            type: note
            priority: 11
            ---
        "#},
    );

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_quoted_priority() {
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

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn rejects_floating_point_priority() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            type: note
            priority: 8.0
            ---
        "#},
    );

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn card_priority_always_renders_a_dash() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            priority: 400
            desired retention: 85
            ---

            <!-- FRONT:BEGIN -->
            <!-- FRONT:END -->
        "#},
    );

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout("./card.md -\n")
        .stderr("");
}
