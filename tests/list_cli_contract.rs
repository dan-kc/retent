#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::fd::OwnedFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::Path;
#[cfg(unix)]
use std::process::{Command, Stdio};

use assert_cmd::Command as AssertCommand;
use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

#[cfg(unix)]
fn write_file(root: &Path, relative_path: &str, contents: &str) {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn list_command(root: &Path) -> AssertCommand {
    let mut command = cargo_bin_cmd!("retent");
    command.arg("list").current_dir(root);
    command
}

#[test]
fn rejects_unsupported_column() {
    let directory = tempdir().unwrap();

    let assert = list_error(directory.path(), &["--col", "importance"]);
    assert.code(2).stdout("");
}

#[test]
fn rejects_unsupported_flag() {
    let directory = tempdir().unwrap();

    let assert = list_error(directory.path(), &["--no-path"]);
    assert.code(2).stdout("");
}

#[test]
fn list_help_displays_supported_columns() {
    let directory = tempdir().unwrap();

    let assert = list_command(directory.path())
        .arg("--help")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.contains("type"));
    assert!(stdout.contains("priority"));
    assert!(stdout.contains("desired retention"));
}

#[test]
fn list_help_displays_absolute_path_flag() {
    let directory = tempdir().unwrap();

    let assert = list_command(directory.path())
        .arg("--help")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.contains("--absolute-path"));
}

#[cfg(unix)]
#[test]
fn stdout_write_errors_exit_without_panicking() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "note.md", "");
    let (stdout, peer) = UnixStream::pair().unwrap();
    drop(peer);

    let output = Command::new(assert_cmd::cargo::cargo_bin!("retent"))
        .arg("list")
        .current_dir(directory.path())
        .stdout(Stdio::from(OwnedFd::from(stdout)))
        .stderr(Stdio::piped())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot write output"));
    assert!(!stderr.contains("panicked"));
}

fn list_error(root: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut command = list_command(root);
    command.args(args).assert()
}
