mod common;

use std::fs;
#[cfg(unix)]
use std::io::Write;
use std::path::Path;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

use common::write_file;

fn priority_command(root: &Path, args: &[&str]) -> Command {
    let mut command = cargo_bin_cmd!("retent");
    command.arg("priority").args(args).current_dir(root);
    command
}

#[test]
fn priority_help_lists_every_operation() {
    let directory = tempdir().unwrap();

    let assert = priority_command(directory.path(), &["--help"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.contains("increment"));
    assert!(stdout.contains("decrement"));
    assert!(stdout.contains("add"));
    assert!(stdout.contains("upsert"));
}

#[test]
fn increment_rejects_zero_without_reading_stdin() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.md", "---\npriority: 4\n---\n");

    priority_command(directory.path(), &["increment", "0"])
        .write_stdin("document.md\n")
        .assert()
        .code(2)
        .stdout("");

    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        "---\npriority: 4\n---\n",
    );
}

#[test]
fn decrement_rejects_zero() {
    let directory = tempdir().unwrap();

    priority_command(directory.path(), &["decrement", "0"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn increment_rejects_an_amount_above_ten() {
    let directory = tempdir().unwrap();

    priority_command(directory.path(), &["increment", "11"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn add_rejects_a_priority_above_ten() {
    let directory = tempdir().unwrap();

    priority_command(directory.path(), &["add", "11"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn upsert_rejects_a_priority_above_ten() {
    let directory = tempdir().unwrap();

    priority_command(directory.path(), &["upsert", "11"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn an_operation_rejects_a_non_integer_argument() {
    let directory = tempdir().unwrap();

    priority_command(directory.path(), &["add", "five"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn an_operation_rejects_a_missing_argument() {
    let directory = tempdir().unwrap();

    priority_command(directory.path(), &["upsert"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn priority_rejects_an_unknown_operation() {
    let directory = tempdir().unwrap();

    priority_command(directory.path(), &["set", "5"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn an_operation_rejects_an_extra_argument() {
    let directory = tempdir().unwrap();

    priority_command(directory.path(), &["add", "5", "extra"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn invalid_utf8_stdin_fails_before_any_file_is_edited() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.md", "---\npriority: 4\n---\n");
    let mut input = b"document.md\n".to_vec();
    input.push(0xff);

    let assert = priority_command(directory.path(), &["increment", "1"])
        .write_stdin(input)
        .assert()
        .code(1)
        .stdout("");
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(stderr.contains("cannot read standard input"));
    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        "---\npriority: 4\n---\n",
    );
}

#[cfg(unix)]
#[test]
fn stdin_read_errors_fail_before_any_file_is_edited() {
    use std::fs::File;
    use std::process::{Command as ProcessCommand, Stdio};

    use assert_cmd::cargo::cargo_bin;

    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.md", "---\npriority: 4\n---\n");
    let stdin = File::open(directory.path()).unwrap();

    let output = ProcessCommand::new(cargo_bin!("retent"))
        .args(["priority", "increment", "1"])
        .current_dir(directory.path())
        .stdin(Stdio::from(stdin))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read standard input"));
    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        "---\npriority: 4\n---\n",
    );
}

#[cfg(unix)]
#[test]
fn stdout_write_errors_fail_after_completed_edits_without_panicking() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::{Command as ProcessCommand, Stdio};

    use assert_cmd::cargo::cargo_bin;

    let directory = tempdir().unwrap();
    write_file(directory.path(), "document.md", "---\npriority: 4\n---\n");
    let (stdout, peer) = UnixStream::pair().unwrap();
    drop(peer);

    let mut child = ProcessCommand::new(cargo_bin!("retent"))
        .args(["priority", "increment", "1"])
        .current_dir(directory.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::from(OwnedFd::from(stdout)))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"document.md\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot write output"));
    assert!(!stderr.contains("panicked"));
    assert_eq!(
        fs::read_to_string(directory.path().join("document.md")).unwrap(),
        "---\npriority: 5\n---\n",
    );
}

#[test]
fn a_concurrent_source_change_is_not_overwritten() {
    use std::thread;
    use std::time::{Duration, Instant};

    let directory = tempdir().unwrap();
    let path = directory.path().join("document.md");
    let mut source = "---\npriority: 4\n---\n".to_owned();
    source.push_str(&"body\n".repeat(4 * 1024 * 1024));
    write_file(directory.path(), "document.md", &source);

    let watched_directory = directory.path().to_path_buf();
    let watched_path = path.clone();
    let watcher = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let temporary_exists = fs::read_dir(&watched_directory)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name() != "document.md");
            if temporary_exists {
                fs::write(&watched_path, "external change\n").unwrap();
                return true;
            }
            thread::yield_now();
        }
        false
    });

    priority_command(directory.path(), &["increment", "1"])
        .write_stdin("document.md\n")
        .assert()
        .success()
        .stdout("document.md\tfile changed while it was being edited\n")
        .stderr("");

    assert!(watcher.join().unwrap());
    assert_eq!(fs::read_to_string(path).unwrap(), "external change\n");
}
