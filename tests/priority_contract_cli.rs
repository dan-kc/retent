use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

#[test]
fn top_level_help_does_not_list_a_priority_command() {
    let directory = tempdir().unwrap();
    let mut command = cargo_bin_cmd!("retent");

    let assert = command
        .arg("--help")
        .current_dir(directory.path())
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(!stdout.contains("priority"));
}

#[test]
fn priority_is_rejected_as_an_unknown_command() {
    let directory = tempdir().unwrap();
    let mut command = cargo_bin_cmd!("retent");

    let assert = command
        .arg("priority")
        .current_dir(directory.path())
        .assert()
        .code(2)
        .stdout("");
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(stderr.contains("unrecognized subcommand 'priority'"));
}
