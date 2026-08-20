mod common;

use indoc::indoc;
use tempfile::tempdir;

use common::{columns_command, list_command, write_bytes, write_file};

#[test]
fn path_only_listing_skips_an_invalid_managed_document() {
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

    list_command(directory.path())
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn column_listing_skips_an_invalid_managed_document() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "invalid.md",
        indoc! {r#"
            ---
            type: note
            priority: missing
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
fn listing_validates_history_when_no_history_column_is_selected() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "invalid.md",
        indoc! {r#"
            ---
            type: note
            priority: 8
            ---

            <!-- HISTORY:BEGIN -->

            | not | a | note history |

            <!-- HISTORY:END -->
        "#},
    );

    columns_command(directory.path(), &["type"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn listing_skips_a_card_without_a_front_block() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "invalid.md",
        indoc! {r#"
            ---
            type: card
            desired retention: 85
            ---
        "#},
    );

    columns_command(directory.path(), &["type"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn listing_continues_with_valid_and_unmanaged_documents() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "invalid.md",
        indoc! {r#"
            ---
            type: note
            priority: 99
            ---
        "#},
    );
    write_file(directory.path(), "misc.md", "Unmanaged Markdown.\n");
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

    columns_command(directory.path(), &["type", "priority"])
        .assert()
        .success()
        .stdout("./misc.md - -\n./valid.md note 8\n")
        .stderr("");
}

#[test]
fn malformed_yaml_frontmatter_is_unmanaged() {
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

    columns_command(directory.path(), &["type", "priority"])
        .assert()
        .success()
        .stdout("./malformed.md - -\n")
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
fn column_listing_skips_invalid_utf8() {
    let directory = tempdir().unwrap();
    write_bytes(directory.path(), "invalid.md", [0xff]);

    columns_command(directory.path(), &["type"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
}
