mod common;

use indoc::indoc;
use tempfile::tempdir;

use common::{columns_command, list_command, write_bytes, write_file};

#[test]
fn malformed_yaml_is_unmanaged_despite_inherited_metadata() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "metadata.yaml",
        "type: note\npriority: 8\n",
    );
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
        .stdout(b"-\t-\t-\t./malformed.md\x00".as_slice())
        .stderr("");
}

#[test]
fn unclosed_frontmatter_is_unmanaged_despite_inherited_metadata() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "metadata.yaml",
        "type: note\npriority: 8\n",
    );
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
        .stdout(b"-\t./unclosed.md\x00".as_slice())
        .stderr("");
}

#[test]
fn non_mapping_frontmatter_is_unmanaged_despite_inherited_metadata() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "metadata.yaml",
        "type: note\npriority: 8\n",
    );
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
        .stdout(b"-\t./sequence.md\x00".as_slice())
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
        .stdout(b"-\t./malformed.md\x00note\t./valid.md\x00".as_slice())
        .stderr("");
}

#[test]
fn list_ignores_invalid_metadata_and_uses_valid_ancestor_defaults() {
    let directory = tempdir().unwrap();
    std::fs::create_dir(directory.path().join(".git")).unwrap();
    write_file(
        directory.path(),
        "metadata.yaml",
        "type: note\npriority: 6\n",
    );
    write_file(
        directory.path(),
        "nested/metadata.yaml",
        "priority: [invalid\n",
    );
    write_file(directory.path(), "nested/note.md", "Body.\n");

    columns_command(&directory.path().join("nested"), &["type", "priority"])
        .assert()
        .success()
        .stdout(b"note\t6\t./note.md\x00".as_slice())
        .stderr("");
}
