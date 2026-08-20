mod common;

use indoc::indoc;
use tempfile::tempdir;

use common::{audit_command, list_command, write_file};

#[cfg(unix)]
#[test]
fn list_escapes_output_breaking_path_characters() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "line\nbreak\\tab\t.md", "Unmanaged.\n");

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./line\\nbreak\\\\tab\\t.md\n")
        .stderr("");
}

#[cfg(unix)]
#[test]
fn audit_escapes_output_breaking_path_characters() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "line\nbreak\\tab\t.md",
        indoc! {r#"
            ---
            type: note
            ---
        "#},
    );

    audit_command(directory.path())
        .assert()
        .code(1)
        .stdout("./line\\nbreak\\\\tab\\t.md\tpriority is missing\n")
        .stderr("");
}

#[cfg(unix)]
#[test]
fn paths_with_invalid_unicode_use_hexadecimal_byte_escapes() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let directory = tempdir().unwrap();
    let name = OsString::from_vec(b"invalid-\xff.md".to_vec());
    std::fs::write(directory.path().join(name), "Unmanaged.\n").unwrap();

    list_command(directory.path())
        .assert()
        .success()
        .stdout("./invalid-\\xff.md\n")
        .stderr("");
}
