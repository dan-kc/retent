mod common;

use std::fs;

use tempfile::tempdir;

use common::{priority_command, write_bytes, write_file};

#[test]
fn empty_and_blank_input_produces_no_output() {
    let directory = tempdir().unwrap();

    priority_command(directory.path(), "add", 5)
        .write_stdin("\n\r\n\n")
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn crlf_terminated_input_names_a_file_without_a_carriage_return() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.md", "---\npriority: 4\n---\n");

    priority_command(directory.path(), "increment", 1)
        .write_stdin("document.md\r\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");
}

#[test]
fn a_missing_path_is_skipped() {
    let directory = tempdir().unwrap();

    let assert = priority_command(directory.path(), "add", 5)
        .write_stdin("missing.md\n")
        .assert()
        .success();
    let output = assert.get_output();
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("missing.md\tcannot inspect file: "));
    assert_eq!(stdout.lines().count(), 1);
}

#[test]
fn a_directory_is_skipped() {
    let directory = tempdir().unwrap();
    fs::create_dir(directory.path().join("nested")).unwrap();

    priority_command(directory.path(), "add", 5)
        .write_stdin("nested\n")
        .assert()
        .success()
        .stdout("nested\tpath is not a regular file\n")
        .stderr("");
}

#[test]
fn invalid_utf8_contents_are_skipped() {
    let directory = tempdir().unwrap();
    write_bytes(directory.path(), "document.md", [0xff]);

    priority_command(directory.path(), "add", 5)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tfile is not valid UTF-8\n")
        .stderr("");

    assert_eq!(
        fs::read(directory.path().join("document.md")).unwrap(),
        [0xff]
    );
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_is_skipped_without_editing_its_target() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    write_file(directory.path(), "target.md", "---\npriority: 4\n---\n");
    symlink("target.md", directory.path().join("link.md")).unwrap();

    priority_command(directory.path(), "increment", 1)
        .write_stdin("link.md\n")
        .assert()
        .success()
        .stdout("link.md\tpath is a symbolic link\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("target.md")).unwrap(),
        "---\npriority: 4\n---\n",
    );
}

#[cfg(unix)]
#[test]
fn a_unix_socket_is_skipped_as_a_non_regular_file() {
    use std::os::unix::net::UnixListener;

    let directory = tempdir().unwrap();
    let _listener = UnixListener::bind(directory.path().join("socket")).unwrap();

    priority_command(directory.path(), "add", 5)
        .write_stdin("socket\n")
        .assert()
        .success()
        .stdout("socket\tpath is not a regular file\n")
        .stderr("");
}

#[cfg(unix)]
#[test]
fn a_fifo_is_skipped_without_trying_to_read_it() {
    use std::process::Command;

    let directory = tempdir().unwrap();
    let fifo = directory.path().join("fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );

    priority_command(directory.path(), "add", 5)
        .write_stdin("fifo\n")
        .assert()
        .success()
        .stdout("fifo\tpath is not a regular file\n")
        .stderr("");
}

#[cfg(unix)]
#[test]
fn separate_hard_link_names_are_each_edited() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "first.md", "---\npriority: 4\n---\n");
    fs::hard_link(
        directory.path().join("first.md"),
        directory.path().join("second.md"),
    )
    .unwrap();

    priority_command(directory.path(), "increment", 1)
        .write_stdin("first.md\nsecond.md\n")
        .assert()
        .success()
        .stdout("first.md\nsecond.md\n")
        .stderr("");

    assert_eq!(
        fs::read_to_string(directory.path().join("first.md")).unwrap(),
        "---\npriority: 5\n---\n",
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("second.md")).unwrap(),
        "---\npriority: 5\n---\n",
    );
}

#[cfg(unix)]
#[test]
fn atomic_replacement_preserves_permission_bits() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let directory = tempdir().unwrap();
    let path = directory.path().join("document.md");
    write_file(directory.path(), "document.md", "---\npriority: 4\n---\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

    priority_command(directory.path(), "increment", 1)
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\n")
        .stderr("");

    assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o640);
}

#[cfg(unix)]
#[test]
fn an_unwritable_directory_skips_an_atomic_edit() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let parent = directory.path().join("nested");
    fs::create_dir(&parent).unwrap();
    write_file(&parent, "document.md", "---\npriority: 4\n---\n");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();

    let assert = priority_command(directory.path(), "increment", 1)
        .write_stdin("nested/document.md\n")
        .assert();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();

    let assert = assert.success();
    let output = assert.get_output();
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("nested/document.md\tcannot create temporary file: "));
    assert_eq!(stdout.lines().count(), 1);
    assert_eq!(
        fs::read_to_string(parent.join("document.md")).unwrap(),
        "---\npriority: 4\n---\n",
    );
}

#[cfg(unix)]
#[test]
fn output_breaking_path_characters_are_escaped() {
    let directory = tempdir().unwrap();
    write_bytes(directory.path(), "tab\tback\\slash.md", [0xff]);

    priority_command(directory.path(), "add", 5)
        .write_stdin("tab\tback\\slash.md\n")
        .assert()
        .success()
        .stdout("tab\\tback\\\\slash.md\tfile is not valid UTF-8\n")
        .stderr("");
}
