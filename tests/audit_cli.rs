mod common;

use std::fs;

use indoc::indoc;
use tempfile::tempdir;

use common::{audit_command, write_bytes, write_file};

#[test]
fn a_vault_without_invalid_entries_passes_without_output() {
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
    write_file(directory.path(), "misc.md", "Unmanaged Markdown.\n");
    write_file(
        directory.path(),
        "malformed.md",
        indoc! {r#"
            ---
            type: [note
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

    audit_command(directory.path())
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn an_invalid_entry_is_reported_and_fails_the_audit() {
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

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./note.md\tpriority is missing\n")
        .stderr("");
}

#[test]
fn independently_detectable_reasons_share_one_row() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "card.md",
        indoc! {r#"
            ---
            type: card
            ---

            <!-- HISTORY:BEGIN -->
        "#},
    );

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout(
            "./card.md\tdesired retention is missing; card front block is missing; history block is unclosed\n",
        )
        .stderr("");
}

#[test]
fn invalid_entries_are_sorted_by_relative_path() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "zeta.md",
        indoc! {r#"
            ---
            type: note
            priority: 12
            ---
        "#},
    );
    write_bytes(directory.path(), "nested/alpha.md", [0xff]);

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout(
            "./nested/alpha.md\tfile is not valid UTF-8\n\
             ./zeta.md\tpriority must be an unquoted integer from 0 to 10\n",
        )
        .stderr("");
}

#[test]
fn absolute_path_replaces_the_relative_audit_path() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "nested/note.md",
        indoc! {r#"
            ---
            type: note
            ---
        "#},
    );
    let root = fs::canonicalize(directory.path()).unwrap();
    let expected = format!(
        "{}\tpriority is missing\n",
        root.join("nested/note.md").display()
    );

    audit_command(directory.path())
        .arg("--absolute-path")
        .assert()
        .code(1)
        .stdout(expected)
        .stderr("");
}

#[test]
fn audit_help_displays_the_absolute_path_flag() {
    let directory = tempdir().unwrap();

    let assert = audit_command(directory.path())
        .arg("--help")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.contains("--absolute-path"));
}
