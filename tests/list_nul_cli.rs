mod common;

use indoc::indoc;
use tempfile::tempdir;

use common::{columns_command, list_command, write_file};

#[test]
fn default_output_terminates_each_path_with_nul() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "first.md", "Unmanaged.\n");
    write_file(directory.path(), "second.md", "Unmanaged.\n");

    list_command(directory.path())
        .assert()
        .success()
        .stdout(b"./first.md\x00./second.md\x00".as_slice())
        .stderr("");
}

#[test]
fn default_output_preserves_spaces_in_paths() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "file with spaces.md", "Unmanaged.\n");

    list_command(directory.path())
        .assert()
        .success()
        .stdout(b"./file with spaces.md\x00".as_slice())
        .stderr("");
}

#[cfg(unix)]
#[test]
fn default_output_preserves_control_characters_and_backslashes_in_paths() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "line\nbreak\\tab\t.md", "Unmanaged.\n");

    list_command(directory.path())
        .assert()
        .success()
        .stdout(b"./line\nbreak\\tab\t.md\x00".as_slice())
        .stderr("");
}

#[cfg(target_os = "linux")]
#[test]
fn default_output_preserves_non_utf8_path_bytes() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let directory = tempdir().unwrap();
    let name = OsString::from_vec(b"invalid-\xff.md".to_vec());
    std::fs::write(directory.path().join(name), "Unmanaged.\n").unwrap();

    list_command(directory.path())
        .assert()
        .success()
        .stdout(b"./invalid-\xff.md\x00".as_slice())
        .stderr("");
}

#[test]
fn columns_precede_the_raw_path_in_requested_order() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note with spaces.md",
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
        .stdout(b"8\tnote\t./note with spaces.md\x00".as_slice())
        .stderr("");
}

#[test]
fn duplicate_columns_each_precede_the_raw_path() {
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
fn absolute_paths_are_raw_and_nul_terminated() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "nested/card.md", "Unmanaged.\n");
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let mut expected = root
        .join("nested/card.md")
        .as_os_str()
        .as_encoded_bytes()
        .to_vec();
    expected.push(0);

    list_command(directory.path())
        .arg("--absolute-path")
        .assert()
        .success()
        .stdout(expected)
        .stderr("");
}
