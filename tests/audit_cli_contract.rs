mod common;

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::fd::OwnedFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::process::{Command, Stdio};

#[cfg(unix)]
use assert_cmd::cargo::cargo_bin;
#[cfg(unix)]
use indoc::indoc;
#[cfg(unix)]
use tempfile::tempdir;

#[cfg(unix)]
use common::write_file;

#[cfg(unix)]
#[test]
fn stdout_write_errors_exit_without_panicking() {
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
    let (stdout, peer) = UnixStream::pair().unwrap();
    drop(peer);

    let output = Command::new(cargo_bin!("retent"))
        .arg("audit")
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

#[cfg(unix)]
#[test]
fn unreadable_files_are_reported_without_stopping_the_audit() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let unreadable = directory.path().join("unreadable.md");
    fs::write(&unreadable, "Unmanaged.\n").unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
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

    let assert = common::audit_command(directory.path()).assert().code(1);
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();

    let output = assert.get_output();
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("./unreadable.md\tcannot read file: "));
    assert_eq!(stdout.lines().count(), 1);
}
