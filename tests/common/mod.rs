use std::fs;
use std::path::Path;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;

pub fn write_file(root: &Path, relative_path: &str, contents: &str) {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

pub fn list_command(root: &Path) -> Command {
    let mut command = cargo_bin_cmd!("retent");
    command.arg("list").current_dir(root);
    command
}

pub fn columns_command(root: &Path, columns: &[&str]) -> Command {
    let mut command = list_command(root);
    for column in columns {
        command.args(["--col", column]);
    }
    command
}
