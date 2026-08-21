mod common;

use indoc::indoc;
use tempfile::tempdir;

use common::{columns_command, write_file};

#[test]
fn renders_recognized_types() {
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

    columns_command(directory.path(), &["type"])
        .assert()
        .success()
        .stdout(b"card\t./card.md\x00note\t./note.md\x00".as_slice())
        .stderr("");
}

#[test]
fn missing_type_renders_dashes() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "misc.md",
        indoc! {r#"
            Some stuff.
        "#},
    );

    columns_command(directory.path(), &["type", "priority", "desired retention"])
        .assert()
        .success()
        .stdout(b"-\t-\t-\t./misc.md\x00".as_slice())
        .stderr("");
}

#[test]
fn unknown_string_type_renders_dashes() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "random.md",
        indoc! {r#"
            ---
            type: random
            priority: 400
            desired retention: 400
            ---
        "#},
    );

    columns_command(directory.path(), &["type", "priority", "desired retention"])
        .assert()
        .success()
        .stdout(b"-\t-\t-\t./random.md\x00".as_slice())
        .stderr("");
}

#[test]
fn non_string_type_renders_dashes() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "numeric.md",
        indoc! {r#"
            ---
            type: 4
            priority: 400
            desired retention: 400
            ---
        "#},
    );

    columns_command(directory.path(), &["type", "priority", "desired retention"])
        .assert()
        .success()
        .stdout(b"-\t-\t-\t./numeric.md\x00".as_slice())
        .stderr("");
}
