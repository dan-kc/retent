mod common;

use indoc::indoc;
use tempfile::tempdir;

use common::{columns_command, write_file};

#[test]
fn preserves_requested_column_order() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            type: note
            priority: 8
            ---
        "#},
    );

    columns_command(directory.path(), &["priority", "type"])
        .assert()
        .success()
        .stdout(b"8\tnote\t./note.md\x00".as_slice())
        .stderr("");
}

#[test]
fn preserves_duplicate_columns() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            type: note
            priority: 8
            ---
        "#},
    );

    columns_command(directory.path(), &["priority", "priority"])
        .assert()
        .success()
        .stdout(b"8\t8\t./note.md\x00".as_slice())
        .stderr("");
}

#[test]
fn continues_listing_after_an_invalid_document() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "invalid.md",
        indoc! {r#"
            ---
            type: note
            priority: 11
            ---
        "#},
    );
    write_file(
        directory.path(),
        "valid.md",
        indoc! {r#"
            ---
            type: note
            priority: 8
            ---
        "#},
    );

    columns_command(directory.path(), &["priority"])
        .assert()
        .success()
        .stdout(b"8\t./valid.md\x00".as_slice())
        .stderr("");
}

#[test]
fn unselected_invalid_values_still_skip_the_document() {
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

    columns_command(directory.path(), &["type"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}
