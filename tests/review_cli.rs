mod common;

use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use common::write_file;
use tempfile::tempdir;

const CARD: &str = "---\ntype: card\ndesired retention: 50\n---\n<!-- FRONT:BEGIN -->\nQuestion\n<!-- FRONT:END -->\n";
const TABLE: &str = "<!-- HISTORY:BEGIN -->\n\n| Date       | Rating |\n| ---------- | -----: |\n\n<!-- HISTORY:END -->\n";
const NOTE: &str = "---\ntype: note\npriority: 5\n---\nText\n";

#[test]
fn appends_note_review_with_cli_fields() {
    let root = tempdir().unwrap();
    write_file(root.path(), "note.md", NOTE);
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "note.md", "--end-line", "12", "--pass", "1"])
        .assert()
        .success();
    let result = fs::read_to_string(root.path().join("note.md")).unwrap();
    assert!(result.contains(&format!("| {} |       12 | 1    |", today())));
    common::audit_command(root.path()).assert().success();
}

#[test]
fn rejects_decreasing_note_pass_without_writing() {
    let root = tempdir().unwrap();
    let source = format!(
        "{NOTE}<!-- HISTORY:BEGIN -->\n\n| Date | End Line | Pass |\n| ---- | -------- | ---- |\n| 2020-01-01 | 10 | 2 |\n\n<!-- HISTORY:END -->\n"
    );
    write_file(root.path(), "note.md", &source);
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "note.md", "--end-line", "12", "--pass", "1"])
        .assert()
        .failure();
    assert_eq!(
        fs::read_to_string(root.path().join("note.md")).unwrap(),
        source
    );
}

#[test]
fn rejects_mismatched_or_missing_note_arguments() {
    let root = tempdir().unwrap();
    write_file(root.path(), "note.md", NOTE);
    for arguments in [
        vec!["--rating", "3"],
        vec!["--end-line", "1"],
        vec!["--pass", "1"],
        vec!["--end-line", "-1", "--pass", "1"],
    ] {
        cargo_bin_cmd!("retent")
            .current_dir(root.path())
            .args(["review", "note.md"])
            .args(arguments)
            .assert()
            .failure();
        assert_eq!(
            fs::read_to_string(root.path().join("note.md")).unwrap(),
            NOTE
        );
    }
}

fn today() -> String {
    jiff::Zoned::now().date().to_string()
}

#[test]
fn appends_today_and_preserves_existing_content() {
    let root = tempdir().unwrap();
    let source = format!(
        "{CARD}{}Tail\n",
        TABLE.replace(
            "\n\n<!-- HISTORY:END",
            "\n| 2020-01-01 |      2 |\n\n<!-- HISTORY:END"
        )
    );
    write_file(root.path(), "card.md", &source);
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "card.md", "--rating", "3"])
        .assert()
        .success();
    let expected = source.replace(
        "\n\n<!-- HISTORY:END",
        &format!("\n| {} |      3 |\n\n<!-- HISTORY:END", today()),
    );
    assert_eq!(
        fs::read_to_string(root.path().join("card.md")).unwrap(),
        expected
    );
}

#[test]
fn creates_missing_history() {
    let root = tempdir().unwrap();
    write_file(root.path(), "card.md", CARD);
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "card.md", "--rating", "1"])
        .assert()
        .success();
    let result = fs::read_to_string(root.path().join("card.md")).unwrap();
    assert!(result.starts_with(CARD));
    assert!(result.contains(&format!("| {} |      1 |", today())));
    common::audit_command(root.path()).assert().success();
}

#[test]
fn permits_repeated_same_day_reviews() {
    let root = tempdir().unwrap();
    write_file(root.path(), "card.md", &format!("{CARD}{TABLE}"));
    for _ in 0..2 {
        cargo_bin_cmd!("retent")
            .current_dir(root.path())
            .args(["review", "card.md", "--rating", "4"])
            .assert()
            .success();
    }
    let result = fs::read_to_string(root.path().join("card.md")).unwrap();
    assert_eq!(
        result.matches(&format!("| {} |      4 |", today())).count(),
        2
    );
}

#[test]
fn uses_inherited_metadata() {
    let root = tempdir().unwrap();
    write_file(root.path(), ".git", "");
    write_file(
        root.path(),
        ".defaults.yaml",
        "type: card\ndesired retention: 50\n",
    );
    write_file(
        root.path(),
        "deck/card.md",
        "<!-- FRONT:BEGIN -->\nQ\n<!-- FRONT:END -->\n",
    );
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "deck/card.md", "--rating", "2"])
        .assert()
        .success();
    common::audit_command(root.path()).assert().success();
}

#[test]
fn rejects_invalid_ratings_without_writing() {
    let root = tempdir().unwrap();
    write_file(root.path(), "card.md", CARD);
    for rating in ["0", "5", "abc", "-1"] {
        cargo_bin_cmd!("retent")
            .current_dir(root.path())
            .args(["review", "card.md", "--rating", rating])
            .assert()
            .failure();
        assert_eq!(
            fs::read_to_string(root.path().join("card.md")).unwrap(),
            CARD
        );
    }
}

#[test]
fn rejects_invalid_or_unmanaged_documents_without_writing() {
    let root = tempdir().unwrap();
    for source in [
        "unmanaged\n".to_owned(),
        format!("{CARD}<!-- HISTORY:BEGIN -->\nbroken\n<!-- HISTORY:END -->\n"),
        CARD.replace("<!-- FRONT:END -->", ""),
    ] {
        write_file(root.path(), "card.md", &source);
        cargo_bin_cmd!("retent")
            .current_dir(root.path())
            .args(["review", "card.md", "--rating", "3"])
            .assert()
            .failure();
        assert_eq!(
            fs::read_to_string(root.path().join("card.md")).unwrap(),
            source
        );
    }
}

#[test]
fn preserves_crlf_and_missing_final_newline() {
    let root = tempdir().unwrap();
    let source = format!("{CARD}{TABLE}").trim_end().replace('\n', "\r\n");
    write_file(root.path(), "card.md", &source);
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "card.md", "--rating", "2"])
        .assert()
        .success();
    let expected = source.replace(
        "\r\n\r\n<!-- HISTORY:END",
        &format!("\r\n| {} |      2 |\r\n\r\n<!-- HISTORY:END", today()),
    );
    assert_eq!(
        fs::read_to_string(root.path().join("card.md")).unwrap(),
        expected
    );
}

#[test]
fn rejects_missing_file() {
    let root = tempdir().unwrap();
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "missing.md", "--rating", "2"])
        .assert()
        .failure();
    assert!(!root.path().join("missing.md").exists());
}

#[test]
fn rejects_wrong_schema_arguments() {
    let root = tempdir().unwrap();
    write_file(root.path(), "card.md", CARD);
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "card.md"])
        .assert()
        .failure();
    assert_eq!(
        fs::read_to_string(root.path().join("card.md")).unwrap(),
        CARD
    );
}

#[test]
fn rejects_invalid_utf8_without_writing() {
    let root = tempdir().unwrap();
    let source = b"\xff\xfe";
    common::write_bytes(root.path(), "card.md", source);
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "card.md", "--rating", "2"])
        .assert()
        .failure();
    assert_eq!(fs::read(root.path().join("card.md")).unwrap(), source);
}

#[test]
fn preserves_permissions() {
    let root = tempdir().unwrap();
    write_file(root.path(), "card.md", CARD);
    let before = fs::metadata(root.path().join("card.md"))
        .unwrap()
        .permissions();
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "card.md", "--rating", "2"])
        .assert()
        .success();
    assert_eq!(
        fs::metadata(root.path().join("card.md"))
            .unwrap()
            .permissions(),
        before
    );
}

#[test]
fn ignores_history_markers_inside_frontmatter() {
    let root = tempdir().unwrap();
    let source = CARD.replacen("---\n", "---\nexample: |\n  <!-- HISTORY:BEGIN -->\n  not a history table\n  <!-- HISTORY:END -->\n", 1);
    write_file(root.path(), "card.md", &source);
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "card.md", "--rating", "3"])
        .assert()
        .success();
    let result = fs::read_to_string(root.path().join("card.md")).unwrap();
    assert!(result.starts_with(&source));
    assert!(result.contains(&format!("| {} |      3 |", today())));
}

#[cfg(unix)]
#[test]
fn rejects_symlink_without_changing_target() {
    let root = tempdir().unwrap();
    write_file(root.path(), "card.md", CARD);
    std::os::unix::fs::symlink("card.md", root.path().join("link.md")).unwrap();
    cargo_bin_cmd!("retent")
        .current_dir(root.path())
        .args(["review", "link.md", "--rating", "2"])
        .assert()
        .failure();
    assert_eq!(
        fs::read_to_string(root.path().join("card.md")).unwrap(),
        CARD
    );
}
