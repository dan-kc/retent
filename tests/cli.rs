use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::tempdir;

fn write_file(root: &Path, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn audits_missing_and_invalid_separately() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "missing.md", "ordinary\n");
    write_file(
        directory.path(),
        "empty.md",
        "---\n# no scheduler fields\n---\n",
    );
    write_file(
        directory.path(),
        "invalid.md",
        "---\ntype: article\npriority: 1\n---\n",
    );
    write_file(directory.path(), "invalid-utf8.md", [0xff]);

    cargo_bin_cmd!("retent")
        .args(["audit", "missing", "--root"])
        .arg(directory.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "missing.md missing: type, priority",
        ))
        .stdout(predicate::str::contains("empty.md missing: type, priority"))
        .stdout(predicate::str::contains("invalid.md").not());

    cargo_bin_cmd!("retent")
        .args(["audit", "invalid", "--root"])
        .arg(directory.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("invalid.md:2 [type-invalid]"))
        .stdout(predicate::str::contains("invalid-utf8.md [utf8-invalid]"))
        .stdout(predicate::str::contains("missing.md").not());
}

#[test]
fn queue_interleaves_types_and_next_reuses_limit() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        "---\ntype: note\npriority: 10\n---\n# Note\n",
    );
    write_file(
        directory.path(),
        "card.md",
        "---\ntype: card\npriority: 20\n---\n## Front\nQ\n## Back\nA\n",
    );

    cargo_bin_cmd!("retent")
        .args(["queue", "--root"])
        .arg(directory.path())
        .args(["--as-of", "2026-08-14"])
        .assert()
        .success()
        .stdout(predicate::str::contains(" note "))
        .stdout(predicate::str::contains(" card "));

    cargo_bin_cmd!("retent")
        .args(["next", "--root"])
        .arg(directory.path())
        .args(["--as-of", "2026-08-14"])
        .assert()
        .success()
        .stdout(predicate::str::contains("note.md"))
        .stdout(predicate::str::contains("card.md").not());
}

#[test]
fn queue_prints_valid_rows_but_fails_for_invalid_files() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "valid.md",
        "---\ntype: note\npriority: 10\n---\n",
    );
    write_file(
        directory.path(),
        "invalid.md",
        "---\ntype: note\npriority: 101\n---\n",
    );

    cargo_bin_cmd!("retent")
        .args(["queue", "--root"])
        .arg(directory.path())
        .args(["--as-of", "2026-08-14"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("valid.md"))
        .stderr(predicate::str::contains("invalid.md:3 [priority-invalid]"))
        .stderr(predicate::str::contains(
            "1 invalid files skipped; run 'retent audit invalid'",
        ));
}

#[test]
fn queue_filters_conflict() {
    cargo_bin_cmd!("retent")
        .args(["queue", "--notes-only", "--cards-only"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn queue_plain_is_headerless_tsv() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        "---\ntype: note\npriority: 10\n---\n# Note\n",
    );

    let expected = "1\tnote\t10\tnew\t2026-08-14\t0\t\t6.310\tnote.md\n";

    cargo_bin_cmd!("retent")
        .args(["queue", "--plain", "--root"])
        .arg(directory.path())
        .args(["--as-of", "2026-08-14"])
        .assert()
        .success()
        .stdout(expected);

    cargo_bin_cmd!("retent")
        .args(["next", "--plain", "--root"])
        .arg(directory.path())
        .args(["--as-of", "2026-08-14"])
        .assert()
        .success()
        .stdout(expected);
}

#[test]
fn position_and_rate_append_replayable_history() {
    let directory = tempdir().unwrap();
    let note = write_file(
        directory.path(),
        "note.md",
        "---\ntype: note\n---\n# Note\nbody\n",
    );
    let card = write_file(
        directory.path(),
        "card.md",
        "---\ntype: card\n---\n## Front\nQ\n## Back\nA\n",
    );

    cargo_bin_cmd!("retent")
        .arg("position")
        .arg(&note)
        .args(["5", "--date", "2026-08-14"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "date=2026-08-14 end_line=5 pass=1",
        ));
    let note_source = fs::read_to_string(&note).unwrap();
    assert!(note_source.contains("| 2026-08-14 |        5 |    1 |"));

    cargo_bin_cmd!("retent")
        .arg("rate")
        .arg(&card)
        .args(["3", "--date", "2026-08-14"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rating=3 next="))
        .stdout(predicate::str::contains("interval="));
    let card_source = fs::read_to_string(&card).unwrap();
    assert!(card_source.contains("| 2026-08-14 |      3 |"));
}

#[test]
fn cli_rejects_invalid_dates_and_ratings_before_editing() {
    let directory = tempdir().unwrap();
    let card = write_file(
        directory.path(),
        "card.md",
        "---\ntype: card\n---\n## Front\nQ\n## Back\nA\n",
    );
    let original = fs::read_to_string(&card).unwrap();

    cargo_bin_cmd!("retent")
        .arg("rate")
        .arg(&card)
        .args(["5", "--date", "2026-08-14"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("1..=4"));
    assert_eq!(fs::read_to_string(&card).unwrap(), original);

    cargo_bin_cmd!("retent")
        .args(["queue", "--as-of", "2026-02-30", "--root"])
        .arg(directory.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid calendar date"));
}

#[test]
fn queue_all_includes_upcoming_items() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "upcoming.md",
        concat!(
            "---\ntype: note\npriority: 100\n---\n",
            "<!-- HISTORY:BEGIN -->\n",
            "| Date | End Line | Pass |\n",
            "| --- | --- | --- |\n",
            "| 2026-08-14 | 1 | 1 |\n",
            "<!-- HISTORY:END -->\n",
        ),
    );

    cargo_bin_cmd!("retent")
        .args(["queue", "--plain", "--as-of", "2026-08-14", "--root"])
        .arg(directory.path())
        .assert()
        .success()
        .stdout("");

    cargo_bin_cmd!("retent")
        .args([
            "queue",
            "--plain",
            "--all",
            "--as-of",
            "2026-08-14",
            "--root",
        ])
        .arg(directory.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("upcoming.md"));
}
