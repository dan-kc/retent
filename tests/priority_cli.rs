mod common;

use std::fs;

use indoc::indoc;
use tempfile::tempdir;

use common::{priority_command, write_file};

#[test]
fn increment_increases_an_existing_priority() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "document.md",
        indoc! {r#"
            ---
            priority: 4
            ---

            Body.
        "#},
    );

    priority_command(directory.path(), "increment", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        indoc! {r#"
            ---
            priority: 5
            ---

            Body.
        "#},
    );
}

#[test]
fn decrement_decreases_an_existing_priority_from_an_absolute_path() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "document.md",
        indoc! {r#"
            ---
            priority: 8
            ---
        "#},
    );
    let path = directory.path().join("document.md");
    let input = format!("{}\n", path.display());

    priority_command(directory.path(), "decrement", 5)
        .write_stdin(input)
        .assert()
        .success()
        .stdout(format!("{}\n", path.display()))
        .stderr("");

    assert_eq!(
        fs::read_to_string(path).unwrap(),
        indoc! {r#"
            ---
            priority: 3
            ---
        "#},
    );
}

#[test]
fn add_inserts_priority_into_existing_frontmatter() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "document.md",
        indoc! {r#"
            ---
            title: Example
            ---

            Body.
        "#},
    );

    priority_command(directory.path(), "add", 4)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        indoc! {r#"
            ---
            title: Example
            priority: 4
            ---

            Body.
        "#},
    );
}

#[test]
fn add_creates_frontmatter_for_a_non_note_file() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.txt", "Plain text.\n");

    priority_command(directory.path(), "add", 4)
        .write_stdin("document.txt\n")
        .assert()
        .success()
        .stdout("document.txt\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.txt")).unwrap(),
        indoc! {r#"
            ---
            priority: 4
            ---

            Plain text.
        "#},
    );
}

#[test]
fn upsert_updates_an_existing_priority() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "document.md",
        indoc! {r#"
            ---
            priority: 2
            ---
        "#},
    );

    priority_command(directory.path(), "upsert", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        indoc! {r#"
            ---
            priority: 5
            ---
        "#},
    );
}

#[test]
fn upsert_inserts_a_missing_priority() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "document.md",
        indoc! {r#"
            ---
            title: Example
            ---
        "#},
    );

    priority_command(directory.path(), "upsert", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        indoc! {r#"
            ---
            title: Example
            priority: 5
            ---
        "#},
    );
}
