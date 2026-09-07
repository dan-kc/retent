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
        .stdout(b"10\t./maximum.md\x000\t./minimum.md\x00".as_slice())
        .stderr("");
}

#[test]
fn nested_metadata_merges_priority_with_an_inherited_type() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "metadata.yaml", "type: note\n");
    write_file(directory.path(), "nested/metadata.yaml", "priority: 8\n");
    write_file(directory.path(), "nested/note.md", "Body.\n");

    columns_command(directory.path(), &["type", "priority"])
        .assert()
        .success()
        .stdout(b"note\t8\t./nested/note.md\x00".as_slice())
        .stderr("");
}

#[test]
fn nearest_metadata_priority_overrides_its_ancestor() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "metadata.yaml",
        "type: note\npriority: 3\n",
    );
    write_file(directory.path(), "nested/metadata.yaml", "priority: 9\n");
    write_file(directory.path(), "nested/note.md", "Body.\n");

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout(b"9\t./nested/note.md\x00".as_slice())
        .stderr("");
}

#[test]
fn file_priority_overrides_directory_metadata() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "metadata.yaml",
        "type: note\npriority: 4\n",
    );
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            priority: 8
            ---
        "#},
    );

    columns_command(directory.path(), &["type", "priority"])
        .assert()
        .success()
        .stdout(b"note\t8\t./note.md\x00".as_slice())
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
        .stdout(b"-\t./card.md\x00".as_slice())
        .stderr("");
}
