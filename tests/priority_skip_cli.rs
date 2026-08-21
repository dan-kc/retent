mod common;

use std::fs;

use indoc::indoc;
use tempfile::tempdir;

use common::{priority_command, write_file};

#[test]
fn increment_skips_a_result_above_ten() {
    let directory = tempdir().unwrap();
    let original = "---\npriority: 8\n---\n";
    write_file(directory.path(), "document.md", original);

    priority_command(directory.path(), "increment", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tincrement would raise priority above 10\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        original,
    );
}

#[test]
fn decrement_skips_a_result_below_zero() {
    let directory = tempdir().unwrap();
    let original = "---\npriority: 3\n---\n";
    write_file(directory.path(), "document.md", original);

    priority_command(directory.path(), "decrement", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tdecrement would lower priority below 0\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        original,
    );
}

#[test]
fn increment_skips_a_file_without_frontmatter() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.md", "Body.\n");

    priority_command(directory.path(), "increment", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tfrontmatter is missing\n")
        .stderr("");
}

#[test]
fn decrement_skips_frontmatter_without_priority() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "document.md",
        "---\ntitle: Example\n---\n",
    );

    priority_command(directory.path(), "decrement", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tpriority is missing\n")
        .stderr("");
}

#[test]
fn increment_skips_an_invalid_priority() {
    let directory = tempdir().unwrap();
    let original = "---\npriority: \"4\"\n---\n";
    write_file(directory.path(), "document.md", original);

    priority_command(directory.path(), "increment", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tpriority must be an unquoted integer from 0 to 10\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        original,
    );
}

#[test]
fn add_skips_an_existing_valid_priority() {
    let directory = tempdir().unwrap();
    let original = "---\npriority: 4\n---\n";
    write_file(directory.path(), "document.md", original);

    priority_command(directory.path(), "add", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tpriority already exists\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        original,
    );
}

#[test]
fn add_skips_an_existing_invalid_priority() {
    let directory = tempdir().unwrap();
    let original = "---\npriority: null\n---\n";
    write_file(directory.path(), "document.md", original);

    priority_command(directory.path(), "add", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tpriority already exists\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        original,
    );
}

#[test]
fn add_skips_malformed_frontmatter() {
    let directory = tempdir().unwrap();
    let original = "---\npriority: [\n---\n";
    write_file(directory.path(), "document.md", original);

    priority_command(directory.path(), "add", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tfrontmatter is malformed\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        original,
    );
}

#[test]
fn upsert_repairs_an_invalid_priority() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "document.md",
        "---\npriority: \"invalid\"\n---\n",
    );

    priority_command(directory.path(), "upsert", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        "---\npriority: 5\n---\n",
    );
}

#[test]
fn a_canonical_path_alias_is_skipped_as_a_duplicate() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.md", "---\npriority: 4\n---\n");
    let absolute = directory.path().join("document.md");

    priority_command(directory.path(), "increment", 1)
        .write_stdin(format!("document.md\n{}\n", absolute.display()))
        .assert()
        .success()
        .stdout(format!(
            "document.md\n{}\tfile was already provided\n",
            absolute.display()
        ))
        .stderr("");

    assert_eq!(
        fs::read_to_string(absolute).unwrap(),
        "---\npriority: 5\n---\n",
    );
}

#[test]
fn edited_files_are_printed_before_skipped_files() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "bounded.md", "---\npriority: 10\n---\n");
    write_file(directory.path(), "first.md", "---\npriority: 1\n---\n");
    write_file(directory.path(), "missing.md", "Body.\n");
    write_file(directory.path(), "second.md", "---\npriority: 4\n---\n");

    priority_command(directory.path(), "increment", 1)
        .write_stdin("bounded.md\nfirst.md\nmissing.md\nsecond.md\n")
        .assert()
        .success()
        .stdout(indoc! {"
            first.md
            second.md
            bounded.md\tincrement would raise priority above 10
            missing.md\tfrontmatter is missing
        "})
        .stderr("");
}
