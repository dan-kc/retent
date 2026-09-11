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
fn inherited_priority_satisfies_a_note_requirement() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), ".defaults.yaml", "priority: 7\n");
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
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn invalid_file_priority_overrides_a_valid_metadata_priority() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        ".defaults.yaml",
        "type: note\npriority: 4\n",
    );
    write_file(
        directory.path(),
        "note.md",
        indoc! {r#"
            ---
            priority: 11
            ---
        "#},
    );

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./note.md\tpriority must be from 0 to 10\n")
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
             ./zeta.md\tpriority must be from 0 to 10\n",
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

#[test]
fn malformed_metadata_is_reported_without_markdown_descendants() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "nested/.defaults.yaml", "type: [note\n");

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./nested/.defaults.yaml\tmetadata is malformed\n")
        .stderr("");
}

#[test]
fn non_mapping_metadata_is_reported() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), ".defaults.yaml", "- note\n- card\n");

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./.defaults.yaml\tmetadata is not a YAML mapping\n")
        .stderr("");
}

#[test]
fn empty_metadata_is_reported_as_non_mapping_yaml() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), ".defaults.yaml", "# Defaults go here.\n");

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./.defaults.yaml\tmetadata is not a YAML mapping\n")
        .stderr("");
}

#[test]
fn explicit_empty_mapping_is_valid_metadata() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), ".defaults.yaml", "{}\n");

    audit_command(directory.path())
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn invalid_utf8_metadata_is_reported() {
    let directory = tempdir().unwrap();
    write_bytes(directory.path(), ".defaults.yaml", [0xff]);

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./.defaults.yaml\tfile is not valid UTF-8\n")
        .stderr("");
}

#[test]
fn invalid_metadata_field_is_reported_on_an_affected_document() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        ".defaults.yaml",
        "type: note\npriority: high\n",
    );
    write_file(directory.path(), "note.md", "Body.\n");

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./note.md\tpriority must be an unquoted integer\n")
        .stderr("");
}

#[test]
fn overridden_invalid_metadata_field_produces_no_finding() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        ".defaults.yaml",
        "type: note\npriority: 11\n",
    );
    write_file(directory.path(), "nested/.defaults.yaml", "priority: 7\n");
    write_file(directory.path(), "nested/note.md", "Body.\n");

    audit_command(directory.path())
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn invalid_layer_is_reported_once_while_valid_descendant_layer_applies() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        ".defaults.yaml",
        "type: note\npriority: 2\n",
    );
    write_file(
        directory.path(),
        "nested/.defaults.yaml",
        "priority: [invalid\n",
    );
    write_file(
        directory.path(),
        "nested/deep/.defaults.yaml",
        "priority: 8\n",
    );
    write_file(directory.path(), "nested/deep/note.md", "Body.\n");

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./nested/.defaults.yaml\tmetadata is malformed\n")
        .stderr("");
}

#[test]
fn metadata_and_document_findings_are_sorted_together() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "alpha.md",
        indoc! {r#"
            ---
            type: note
            ---
        "#},
    );
    write_file(directory.path(), ".defaults.yaml", "not: [closed\n");

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout(
            "./.defaults.yaml\tmetadata is malformed\n\
             ./alpha.md\tpriority is missing\n",
        )
        .stderr("");
}

#[test]
fn inherited_invalid_metadata_above_cwd_uses_a_parent_relative_path() {
    let directory = tempdir().unwrap();
    fs::create_dir(directory.path().join(".git")).unwrap();
    write_file(directory.path(), ".defaults.yaml", "type: [note\n");
    fs::create_dir(directory.path().join("nested")).unwrap();

    audit_command(&directory.path().join("nested"))
        .assert()
        .code(1)
        .stdout("../.defaults.yaml\tmetadata is malformed\n")
        .stderr("");
}

#[test]
fn absolute_path_applies_to_inherited_metadata_above_cwd() {
    let directory = tempdir().unwrap();
    fs::create_dir(directory.path().join(".git")).unwrap();
    write_file(directory.path(), ".defaults.yaml", "type: [note\n");
    fs::create_dir(directory.path().join("nested")).unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    let expected = format!(
        "{}\tmetadata is malformed\n",
        root.join(".defaults.yaml").display()
    );

    audit_command(&directory.path().join("nested"))
        .arg("--absolute-path")
        .assert()
        .code(1)
        .stdout(expected)
        .stderr("");
}
