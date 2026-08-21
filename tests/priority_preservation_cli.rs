mod common;

use std::fs;

use tempfile::tempdir;

use common::{priority_command, write_bytes, write_file};

#[test]
fn increment_preserves_key_spacing_and_an_inline_comment() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "document.md",
        "---\ntitle: Example\npriority : 4  # ranking\n---\n",
    );

    priority_command(directory.path(), "increment", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        "---\ntitle: Example\npriority : 5  # ranking\n---\n",
    );
}

#[test]
fn decrement_skips_a_quoted_priority_key() {
    let directory = tempdir().unwrap();
    let original = "---\n\"priority\": 4\n---\n";
    write_file(directory.path(), "document.md", original);

    priority_command(directory.path(), "decrement", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tfrontmatter cannot be safely edited without rewriting\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        original,
    );
}

#[test]
fn upsert_repairs_a_quoted_value_without_removing_its_comment() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "document.md",
        "---\npriority: \"invalid value\"  # repair this\n---\n",
    );

    priority_command(directory.path(), "upsert", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        "---\npriority: 5  # repair this\n---\n",
    );
}

#[test]
fn increment_preserves_a_utf8_bom() {
    let directory = tempdir().unwrap();
    write_bytes(
        directory.path(),
        "document.md",
        b"\xef\xbb\xbf---\npriority: 4\n---\n",
    );

    priority_command(directory.path(), "increment", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read(directory.path().join("document.md")).unwrap(),
        b"\xef\xbb\xbf---\npriority: 5\n---\n",
    );
}

#[test]
fn add_keeps_a_utf8_bom_before_new_frontmatter() {
    let directory = tempdir().unwrap();
    write_bytes(directory.path(), "document.txt", b"\xef\xbb\xbfBody.\n");

    priority_command(directory.path(), "add", 5)
        .write_stdin("document.txt\n")
        .assert()
        .success()
        .stdout("document.txt\n")
        .stderr("");

    assert_eq!(
        fs::read(directory.path().join("document.txt")).unwrap(),
        b"\xef\xbb\xbf---\npriority: 5\n---\n\nBody.\n",
    );
}

#[test]
fn add_uses_and_preserves_crlf_line_endings() {
    let directory = tempdir().unwrap();
    write_bytes(
        directory.path(),
        "document.md",
        b"---\r\ntitle: Example\r\n---\r\n\r\nBody.\r\n",
    );

    priority_command(directory.path(), "add", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read(directory.path().join("document.md")).unwrap(),
        b"---\r\ntitle: Example\r\npriority: 5\r\n---\r\n\r\nBody.\r\n",
    );
}

#[test]
fn increment_preserves_a_missing_final_newline() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.md", "---\npriority: 4\n---");

    priority_command(directory.path(), "increment", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        "---\npriority: 5\n---",
    );
}

#[test]
fn add_creates_canonical_frontmatter_for_an_empty_file() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.md", "");

    priority_command(directory.path(), "add", 0)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        "---\npriority: 0\n---\n",
    );
}

#[test]
fn increment_skips_a_flow_mapping_that_requires_rewriting() {
    let directory = tempdir().unwrap();
    let original = "---\n{title: Example, priority: 4}\n---\n";
    write_file(directory.path(), "document.md", original);

    priority_command(directory.path(), "increment", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tfrontmatter cannot be safely edited without rewriting\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        original,
    );
}

#[test]
fn upsert_skips_a_multiline_priority_value() {
    let directory = tempdir().unwrap();
    let original = "---\npriority:\n  nested: value\n---\n";
    write_file(directory.path(), "document.md", original);

    priority_command(directory.path(), "upsert", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tfrontmatter cannot be safely edited without rewriting\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        original,
    );
}

#[cfg(unix)]
#[test]
fn an_idempotent_upsert_does_not_replace_the_file() {
    use std::os::unix::fs::MetadataExt;

    let directory = tempdir().unwrap();
    let path = directory.path().join("document.md");
    write_file(directory.path(), "document.md", "---\npriority: 5\n---\n");
    let inode = fs::metadata(&path).unwrap().ino();

    priority_command(directory.path(), "upsert", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(fs::metadata(path).unwrap().ino(), inode);
}
