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
        .stdout("./card.md card\n./note.md note\n")
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
        .stdout("./misc.md - - -\n")
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
        .stdout("./random.md - - -\n")
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
        .stdout("./numeric.md - - -\n")
        .stderr("");
}
