mod common;

use indoc::indoc;
use tempfile::tempdir;

use common::{columns_command, list_command, write_bytes, write_file};

#[test]
fn malformed_yaml_is_unmanaged_for_every_selected_column() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "malformed.md",
        indoc! {r#"
            ---
            type: [note
            ---
        "#},
    );

    columns_command(directory.path(), &["type", "priority", "desired retention"])
        .assert()
        .success()
        .stdout("./malformed.md - - -\n")
        .stderr("");
}

#[test]
fn unclosed_frontmatter_is_unmanaged() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "unclosed.md",
        indoc! {r#"
            ---
            type: note
            priority: 8
        "#},
    );

    columns_command(directory.path(), &["type"])
        .assert()
        .success()
        .stdout("./unclosed.md -\n")
        .stderr("");
}

#[test]
fn non_mapping_frontmatter_renders_a_dash() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "sequence.md",
        indoc! {r#"
            ---
            - note
            - card
            ---
        "#},
    );

    columns_command(directory.path(), &["type"])
        .assert()
        .success()
        .stdout("./sequence.md -\n")
        .stderr("");
}

#[test]
fn column_listing_skips_invalid_utf8() {
    let directory = tempdir().unwrap();
    write_bytes(directory.path(), "invalid.md", [0xff]);

    columns_command(directory.path(), &["type"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn path_only_listing_skips_invalid_utf8() {
    let directory = tempdir().unwrap();
    write_bytes(directory.path(), "invalid.md", [0xff]);

    list_command(directory.path())
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn continues_listing_after_malformed_frontmatter() {
    let directory = tempdir().unwrap();
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
        "valid.md",
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
        .stdout("./malformed.md -\n./valid.md note\n")
        .stderr("");
}
