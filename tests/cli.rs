use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn audits_missing_and_invalid_separately() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("missing.md"), "ordinary\n").unwrap();
    fs::write(
        directory.path().join("invalid.md"),
        "---\ntype: article\npriority: 1\n---\n",
    )
    .unwrap();

    cargo_bin_cmd!("retent")
        .args(["audit", "missing", "--root"])
        .arg(directory.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "missing.md missing: type, priority",
        ))
        .stdout(predicate::str::contains("invalid.md").not());

    cargo_bin_cmd!("retent")
        .args(["audit", "invalid", "--root"])
        .arg(directory.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("invalid.md:2 [type-invalid]"))
        .stdout(predicate::str::contains("missing.md").not());
}

#[test]
fn queue_interleaves_types_and_next_reuses_limit() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("note.md"),
        "---\ntype: note\npriority: 10\n---\n# Note\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("card.md"),
        "---\ntype: card\npriority: 20\n---\n## Front\nQ\n## Back\nA\n",
    )
    .unwrap();

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
    fs::write(
        directory.path().join("valid.md"),
        "---\ntype: note\npriority: 10\n---\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("invalid.md"),
        "---\ntype: note\npriority: 101\n---\n",
    )
    .unwrap();

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
