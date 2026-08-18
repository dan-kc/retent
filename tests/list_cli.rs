use std::fs;
use std::path::Path;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

fn write_file(root: &Path, relative_path: &str, contents: &str) {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn list_command(root: &Path) -> Command {
    let mut command = cargo_bin_cmd!("retent");
    command.arg("list").current_dir(root);
    command
}

#[test]
fn lists_markdown_files_recursively() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "nested/card.md", "");
    write_file(directory.path(), "note.md", "");

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./nested/card.md\n./note.md\n")
        .stderr("");
}

#[test]
fn sorts_paths_lexically() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "zeta.md", "");
    write_file(directory.path(), "alpha.md", "");
    write_file(directory.path(), "middle.md", "");

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./alpha.md\n./middle.md\n./zeta.md\n")
        .stderr("");
}

#[test]
fn matches_markdown_extensions_case_insensitively() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "lower.md", "");
    write_file(directory.path(), "mixed.Md", "");
    write_file(directory.path(), "upper.MD", "");

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./lower.md\n./mixed.Md\n./upper.MD\n")
        .stderr("");
}

#[test]
fn ignores_files_without_a_markdown_extension() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "card.md", "");
    write_file(directory.path(), "note.markdown", "");
    write_file(directory.path(), "note.txt", "");

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./card.md\n")
        .stderr("");
}

#[test]
fn includes_hidden_directories_other_than_git() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), ".notes/hidden.md", "");

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./.notes/hidden.md\n")
        .stderr("");
}

#[test]
fn skips_git_directories_at_any_depth() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "visible.md", "");
    write_file(directory.path(), ".git/root-internal.md", "");
    write_file(directory.path(), "nested/visible.md", "");
    write_file(directory.path(), "nested/.git/nested-internal.md", "");

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./nested/visible.md\n./visible.md\n")
        .stderr("");
}

#[cfg(unix)]
#[test]
fn excludes_file_symlinks() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    write_file(directory.path(), "real.md", "");
    symlink("real.md", directory.path().join("linked.md")).unwrap();

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./real.md\n")
        .stderr("");
}

#[cfg(unix)]
#[test]
fn does_not_traverse_directory_symlinks() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    write_file(directory.path(), "nested/inside.md", "");
    symlink("nested", directory.path().join("linked-directory")).unwrap();

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./nested/inside.md\n")
        .stderr("");
}

#[test]
fn absolute_path_replaces_the_relative_path_column() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "nested/card.md", "");
    let root = fs::canonicalize(directory.path()).unwrap();
    let expected = format!("{}\n", root.join("nested/card.md").display());

    list_command(directory.path())
        .arg("--absolute-path")
        .assert()
        .success()
        .stdout(expected)
        .stderr("");
}

#[test]
fn empty_tree_returns_nothing_successfully() {
    let directory = tempdir().unwrap();

    list_command(directory.path())
        .assert()
        .success()
        .stdout("")
        .stderr("");
}
