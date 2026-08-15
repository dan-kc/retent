use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use prost::Message;
use tempfile::tempdir;
use zip::write::SimpleFileOptions;

fn write_file(root: &Path, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, contents).unwrap();
    path
}

fn filtered_paths(root: &Path, filter: &str) -> Vec<u8> {
    let output = cargo_bin_cmd!("retent")
        .args(["list", "--paths", "--filter", filter, "--root"])
        .arg(root)
        .output()
        .unwrap();
    assert!(output.status.success());
    output.stdout
}

fn format_list(root: &Path, paths: &str, field: &str, style: &str) -> assert_cmd::assert::Assert {
    let mut command = cargo_bin_cmd!("retent");
    command
        .args(["format-list", field, "--style", style, "--root"])
        .arg(root)
        .write_stdin(paths)
        .assert()
}

#[test]
fn formats_paths_from_stdin_and_reports_updated_files() {
    let directory = tempdir().unwrap();
    let first = write_file(
        directory.path(),
        "first.md",
        "---\ntitle: Example\ntags:\n  - Iterators\n  - Rust\n---\nBody\n",
    );
    let second = write_file(
        directory.path(),
        "second.md",
        "---\ntags:\n  - Other\n---\n",
    );
    format_list(directory.path(), "first.md\nsecond.md\n", "tags", "flow")
        .success()
        .stdout("updated 2 files\n");
    assert_eq!(
        fs::read_to_string(first).unwrap(),
        "---\ntitle: Example\ntags: [Iterators, Rust]\n---\nBody\n"
    );
    assert_eq!(
        fs::read_to_string(second).unwrap(),
        "---\ntags: [Other]\n---\n"
    );
}

#[test]
fn format_list_preflights_all_paths_before_writing() {
    let directory = tempdir().unwrap();
    let valid = write_file(directory.path(), "valid.md", "---\ntags:\n  - one\n---\n");
    write_file(directory.path(), "invalid.md", "no frontmatter\n");
    format_list(directory.path(), "valid.md\ninvalid.md\n", "tags", "flow")
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("invalid.md"));
    assert_eq!(
        fs::read_to_string(valid).unwrap(),
        "---\ntags:\n  - one\n---\n"
    );
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
        .arg("queue")
        .current_dir(directory.path())
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
        .arg("queue")
        .current_dir(directory.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("valid.md"))
        .stderr(predicate::str::contains("invalid.md:3 [priority-invalid]"))
        .stderr(predicate::str::contains(
            "1 invalid files skipped; run 'retent audit invalid'",
        ));
}

#[test]
fn list_type_filters_conflict() {
    cargo_bin_cmd!("retent")
        .args(["list", "--notes-only", "--cards-only"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn list_and_next_plain_are_headerless_tsv() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "note.md",
        "---\ntype: note\npriority: 10\n---\n# Note\n",
    );

    let expected = "1\tnote\t10\tnew\t2026-08-14\t0\t\t6.310\tnote.md\n";

    cargo_bin_cmd!("retent")
        .args(["list", "--plain", "--root"])
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
        .args(["list", "--as-of", "2026-02-30", "--root"])
        .arg(directory.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid calendar date"));
}

#[test]
fn queue_rejects_view_options() {
    for option in [
        "--all",
        "--plain",
        "--paths",
        "--wrap",
        "--notes-only",
        "--cards-only",
    ] {
        cargo_bin_cmd!("retent")
            .args(["queue", option])
            .assert()
            .failure()
            .stderr(predicate::str::contains("unexpected argument"));
    }

    for (option, value) in [
        ("--root", "."),
        ("--as-of", "2026-08-14"),
        ("--filter", "priority = 50"),
        ("--limit", "1"),
    ] {
        cargo_bin_cmd!("retent")
            .args(["queue", option, value])
            .assert()
            .failure()
            .stderr(predicate::str::contains("unexpected argument"));
    }
}

#[test]
fn list_shows_the_full_scheduled_table_and_supports_composed_filters() {
    let directory = tempdir().unwrap();
    write_file(directory.path(), "plain.md", "ordinary Markdown\n");
    write_file(
        directory.path(),
        "foo.md",
        "---\ntype: note\npriority: 50\ntags: [foo, bar]\n---\n",
    );
    write_file(
        directory.path(),
        "baz.md",
        "---\ntype: note\npriority: 100\ntags: [foo, baz]\n---\n",
    );
    write_file(
        directory.path(),
        "upcoming.md",
        concat!(
            "---\ntype: note\npriority: 100\ntags: [future]\n---\n",
            "<!-- HISTORY:BEGIN -->\n",
            "| Date | End Line | Pass |\n",
            "| --- | --- | --- |\n",
            "| 2026-08-14 | 1 | 1 |\n",
            "<!-- HISTORY:END -->\n",
        ),
    );

    cargo_bin_cmd!("retent")
        .args(["list", "--as-of", "2026-08-14", "--root"])
        .arg(directory.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Type"))
        .stdout(predicate::str::contains("Prio"))
        .stdout(predicate::str::contains("Status"))
        .stdout(predicate::str::contains("Due"))
        .stdout(predicate::str::contains("Age"))
        .stdout(predicate::str::contains("Int"))
        .stdout(predicate::str::contains("Score"))
        .stdout(predicate::str::contains("Path"))
        .stdout(predicate::str::contains("foo.md"))
        .stdout(predicate::str::contains("baz.md"))
        .stdout(predicate::str::contains("upcoming.md"))
        .stdout(predicate::str::contains("plain.md").not());

    cargo_bin_cmd!("retent")
        .args(["list", "--plain", "--as-of", "2026-08-14", "--root"])
        .arg(directory.path())
        .args([
            "--filter",
            "priority >= 50 and tags.any(foo, bar) & tags.none(baz)",
        ])
        .assert()
        .success()
        .stdout("1\tnote\t50\tnew\t2026-08-14\t0\t\t1.000\tfoo.md\n");

    cargo_bin_cmd!("retent")
        .args([
            "list",
            "--paths",
            "--as-of",
            "2026-08-14",
            "--filter",
            "tags.exact(bar, foo)",
            "--root",
        ])
        .arg(directory.path())
        .assert()
        .success()
        .stdout("foo.md\n");

    cargo_bin_cmd!("retent")
        .args([
            "list",
            "--plain",
            "--as-of",
            "2026-08-14",
            "--filter",
            "tags.exact(bar, foo)",
            "--root",
        ])
        .arg(directory.path())
        .assert()
        .success()
        .stdout("1\tnote\t50\tnew\t2026-08-14\t0\t\t1.000\tfoo.md\n");
}

#[test]
fn list_and_next_apply_filters_before_ranking() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "foo.md",
        "---\ntype: note\npriority: 10\ntags: [foo]\n---\n",
    );
    write_file(
        directory.path(),
        "bar.md",
        "---\ntype: note\npriority: 90\ntags: [bar]\n---\n",
    );

    for command in ["list", "next"] {
        cargo_bin_cmd!("retent")
            .arg(command)
            .args(["--plain", "--filter", "tags.any(bar)", "--root"])
            .arg(directory.path())
            .args(["--as-of", "2026-08-14"])
            .assert()
            .success()
            .stdout(predicate::str::contains("bar.md"))
            .stdout(predicate::str::contains("foo.md").not());
    }
}

#[test]
fn commands_reject_invalid_filter_syntax() {
    for command in ["list", "next"] {
        cargo_bin_cmd!("retent")
            .arg(command)
            .args(["--filter", "tags.any(foo"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("filter syntax error at byte"));
    }
}

#[test]
fn filters_do_not_hide_invalid_files() {
    let directory = tempdir().unwrap();
    write_file(
        directory.path(),
        "invalid.md",
        "---\ntype: note\npriority: 101\ntags: [other]\n---\n",
    );

    for command in ["list", "next"] {
        cargo_bin_cmd!("retent")
            .arg(command)
            .args(["--filter", "tags.any(wanted)", "--root"])
            .arg(directory.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid.md:3 [priority-invalid]"));
    }
}

#[test]
fn list_rejects_conflicting_machine_readable_formats() {
    for arguments in [
        ["list", "--plain", "--paths"],
        ["list", "--plain", "--wrap"],
        ["list", "--paths", "--wrap"],
        ["next", "--plain", "--wrap"],
    ] {
        cargo_bin_cmd!("retent")
            .args(arguments)
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot be used with"));
    }
}

#[test]
fn update_priority_only_changes_documents_matching_the_filter() {
    let directory = tempdir().unwrap();
    let wanted = write_file(
        directory.path(),
        "wanted.md",
        "---\ntype: note\npriority: 10\ntags: [wanted]\n---\n# Wanted\n",
    );
    let other = write_file(
        directory.path(),
        "other.md",
        "---\ntype: note\npriority: 20\ntags: [other]\n---\n# Other\n",
    );
    let other_original = fs::read_to_string(&other).unwrap();

    let listed = filtered_paths(directory.path(), "tags.any(wanted)");

    cargo_bin_cmd!("retent")
        .args(["update", "priority", "75", "--files-from", "-", "--root"])
        .arg(directory.path())
        .write_stdin(listed)
        .assert()
        .success()
        .stdout("updated 1 file\n");

    let updated = fs::read_to_string(wanted).unwrap();
    assert!(updated.contains("priority: 75"));
    assert!(updated.ends_with("---\n# Wanted\n"));
    assert_eq!(fs::read_to_string(other).unwrap(), other_original);
}

#[test]
fn update_tags_add_can_keep_or_overwrite_existing_tags_and_deduplicates() {
    let keep_directory = tempdir().unwrap();
    let keep = write_file(
        keep_directory.path(),
        "keep.md",
        "---\ntype: note\npriority: 10\ntags: [old, shared]\n---\n",
    );

    let paths = filtered_paths(keep_directory.path(), "priority = 10");
    cargo_bin_cmd!("retent")
        .args([
            "update",
            "tags",
            "add",
            "shared",
            "new",
            "new",
            "--existing",
            "keep",
            "--files-from",
            "-",
            "--root",
        ])
        .arg(keep_directory.path())
        .write_stdin(paths)
        .assert()
        .success()
        .stdout("updated 1 file\n");
    let kept = retent::document::read(&keep).unwrap();
    assert_eq!(kept.metadata.tags, ["old", "shared", "new"]);

    let overwrite_directory = tempdir().unwrap();
    let overwrite = write_file(
        overwrite_directory.path(),
        "overwrite.md",
        "---\ntype: note\npriority: 10\ntags: [old, shared]\n---\n",
    );

    let paths = filtered_paths(overwrite_directory.path(), "priority = 10");
    cargo_bin_cmd!("retent")
        .args([
            "update",
            "tags",
            "add",
            "shared",
            "new",
            "new",
            "--existing",
            "overwrite",
            "--files-from",
            "-",
            "--root",
        ])
        .arg(overwrite_directory.path())
        .write_stdin(paths)
        .assert()
        .success();
    let overwritten = retent::document::read(&overwrite).unwrap();
    assert_eq!(overwritten.metadata.tags, ["shared", "new"]);
}

#[test]
fn update_tags_rename_is_filtered_and_deduplicates_collisions() {
    let directory = tempdir().unwrap();
    let wanted = write_file(
        directory.path(),
        "wanted.md",
        "---\ntype: note\npriority: 10\ntags: [old, new, other]\n---\n",
    );
    let untouched = write_file(
        directory.path(),
        "untouched.md",
        "---\ntype: note\npriority: 20\ntags: [old]\n---\n",
    );

    let paths = filtered_paths(directory.path(), "priority = 10");
    cargo_bin_cmd!("retent")
        .args([
            "update",
            "tags",
            "rename",
            "old",
            "new",
            "--files-from",
            "-",
            "--root",
        ])
        .arg(directory.path())
        .write_stdin(paths)
        .assert()
        .success()
        .stdout("updated 1 file\n");

    assert_eq!(
        retent::document::read(&wanted).unwrap().metadata.tags,
        ["new", "other"]
    );
    assert_eq!(
        retent::document::read(&untouched).unwrap().metadata.tags,
        ["old"]
    );
}

#[test]
fn update_tags_remove_only_removes_requested_tags_from_filtered_documents() {
    let directory = tempdir().unwrap();
    let wanted = write_file(
        directory.path(),
        "wanted.md",
        "---\ntype: note\npriority: 10\ntags: [keep, remove-one, remove-two]\n---\n",
    );
    let untouched = write_file(
        directory.path(),
        "untouched.md",
        "---\ntype: note\npriority: 20\ntags: [remove-one]\n---\n",
    );

    let paths = filtered_paths(directory.path(), "priority = 10");
    cargo_bin_cmd!("retent")
        .args([
            "update",
            "tags",
            "remove",
            "remove-one",
            "remove-two",
            "absent",
            "--files-from",
            "-",
            "--root",
        ])
        .arg(directory.path())
        .write_stdin(paths)
        .assert()
        .success()
        .stdout("updated 1 file\n");

    assert_eq!(
        retent::document::read(&wanted).unwrap().metadata.tags,
        ["keep"]
    );
    assert_eq!(
        retent::document::read(&untouched).unwrap().metadata.tags,
        ["remove-one"]
    );
}

#[test]
fn update_preflights_selected_invalid_files_before_writing_any_changes() {
    let directory = tempdir().unwrap();
    let valid = write_file(
        directory.path(),
        "valid.md",
        "---\ntype: note\npriority: 10\ntags: [wanted]\n---\n",
    );
    write_file(
        directory.path(),
        "invalid.md",
        "---\ntype: note\npriority: 101\ntags: [other]\n---\n",
    );
    let original = fs::read_to_string(&valid).unwrap();

    cargo_bin_cmd!("retent")
        .args(["update", "priority", "75", "--files-from", "-", "--root"])
        .arg(directory.path())
        .write_stdin("valid.md\ninvalid.md\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid.md:3 [priority-invalid]"))
        .stderr(predicate::str::contains("no changes made"));

    assert_eq!(fs::read_to_string(valid).unwrap(), original);
}

#[test]
fn update_accepts_a_named_file_list_and_ignores_duplicate_paths() {
    let directory = tempdir().unwrap();
    let document = write_file(
        directory.path(),
        "selected.md",
        "---\ntype: note\npriority: 10\n---\n",
    );
    let paths = write_file(
        directory.path(),
        "selection.txt",
        "selected.md\nselected.md\n",
    );

    cargo_bin_cmd!("retent")
        .args(["update", "priority", "30", "--files-from"])
        .arg(paths)
        .args(["--root"])
        .arg(directory.path())
        .assert()
        .success()
        .stdout("updated 1 file\n");

    assert_eq!(
        retent::document::read(&document).unwrap().metadata.priority,
        Some(30)
    );
}

#[test]
fn update_rejects_selected_paths_outside_root() {
    let directory = tempdir().unwrap();
    let root = directory.path().join("vault");
    fs::create_dir(&root).unwrap();
    let outside = write_file(
        directory.path(),
        "outside.md",
        "---\ntype: note\npriority: 10\n---\n",
    );
    let original = fs::read_to_string(&outside).unwrap();

    cargo_bin_cmd!("retent")
        .args(["update", "priority", "30", "--files-from", "-", "--root"])
        .arg(&root)
        .write_stdin("../outside.md\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("path is outside root"));

    assert_eq!(fs::read_to_string(outside).unwrap(), original);
}

#[test]
fn update_requires_files_from_and_does_not_accept_a_filter() {
    cargo_bin_cmd!("retent")
        .args(["update", "priority", "30"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--files-from"));

    cargo_bin_cmd!("retent")
        .args(["update", "priority", "30", "--filter", "priority = 10"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '--filter'"));
}

#[test]
fn anki_import_resumes_after_missing_media_and_reports_skips() {
    let directory = tempdir().unwrap();
    let archive = directory.path().join("biology.colpkg");
    let vault = directory.path().join("biology");
    write_colpkg_fixture(&archive, false);

    cargo_bin_cmd!("retent")
        .args(["import", "anki"])
        .arg(&archive)
        .args(["--output"])
        .arg(&vault)
        .assert()
        .failure()
        .stdout(predicate::str::contains("created card "))
        .stdout(predicate::str::contains(
            "imported 1 cards and 0 media files",
        ))
        .stderr(predicate::str::contains(
            "media \"picture one.PNG\" (0): cannot read archive entry",
        ))
        .stderr(predicate::str::contains(
            "fix the errors and rerun the same command to resume",
        ));

    let cards = markdown_children(&vault);
    assert_eq!(cards.len(), 1);
    let card = fs::read_to_string(&cards[0]).unwrap();
    assert!(card.contains("type: card"));
    assert!(card.contains("tags: [\"Study\",\"Biology\",\"Cells\"]"));
    assert!(card.contains("What is **ATP**?"));
    assert!(card.contains("## Back\n\nAdenosine triphosphate"));
    assert!(card.contains("| 2026-08-12 |      2 |"));
    assert!(card.contains("| 2026-08-13 |      4 |"));
    assert!(!card.contains("<div>"));
    assert!(!card.contains("<b>"));
    let image_reference = card
        .split("![](./images/")
        .nth(1)
        .and_then(|value| value.split(')').next())
        .unwrap();
    let (image_stem, image_extension) = image_reference.rsplit_once('.').unwrap();
    assert_eq!(image_stem.len(), 32);
    assert!(image_stem.bytes().all(|byte| byte.is_ascii_alphanumeric()));
    assert_eq!(image_extension, "png");
    assert!(!vault.join("images").join(image_reference).exists());

    write_colpkg_fixture(&archive, true);
    cargo_bin_cmd!("retent")
        .args(["import", "anki"])
        .arg(&archive)
        .args(["--output"])
        .arg(&vault)
        .assert()
        .success()
        .stdout(predicate::str::contains("copied media \"picture one.PNG\""))
        .stdout(predicate::str::contains("skipped card "))
        .stdout(predicate::str::contains(
            "imported 0 cards and 1 media files",
        ));
    assert_eq!(
        fs::read(vault.join("images").join(image_reference)).unwrap(),
        b"not really a png"
    );

    cargo_bin_cmd!("retent")
        .args(["import", "anki"])
        .arg(&archive)
        .args(["--output"])
        .arg(&vault)
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped media images/"))
        .stdout(predicate::str::contains("skipped card "))
        .stdout(predicate::str::contains(
            "skipped 1 cards and 1 media files",
        ));
    assert_eq!(markdown_children(&vault), cards);
}

#[test]
fn anki_import_defaults_to_a_sibling_vault() {
    let directory = tempdir().unwrap();
    let archive = directory.path().join("my-export.colpkg");
    write_colpkg_fixture(&archive, true);

    cargo_bin_cmd!("retent")
        .args(["import", "anki"])
        .arg(&archive)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            directory.path().join("my-export").display().to_string(),
        ));
    assert_eq!(
        markdown_children(&directory.path().join("my-export")).len(),
        1
    );
}

#[test]
fn anki_import_supports_current_zstd_protobuf_packages() {
    let directory = tempdir().unwrap();
    let archive = directory.path().join("modern.colpkg");
    let vault = directory.path().join("modern-vault");
    write_modern_colpkg_fixture(&archive);

    cargo_bin_cmd!("retent")
        .args(["import", "anki"])
        .arg(&archive)
        .args(["--output"])
        .arg(&vault)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "imported 1 cards and 1 media files",
        ));

    let cards = markdown_children(&vault);
    assert_eq!(cards.len(), 1);
    let card = fs::read_to_string(&cards[0]).unwrap();
    assert!(card.contains("tags: [\"Modern\",\"Nested\"]"));
    assert!(card.contains("Modern **front**"));
    assert!(card.contains("Modern back"));
    let image_reference = card
        .split("![](./images/")
        .nth(1)
        .and_then(|value| value.split(')').next())
        .unwrap();
    assert_eq!(
        fs::read(vault.join("images").join(image_reference)).unwrap(),
        b"modern media"
    );
}

fn markdown_children(root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    paths.sort();
    paths
}

fn write_colpkg_fixture(path: &Path, include_media: bool) {
    let database = path.with_extension("anki21");
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "
            CREATE TABLE col (models TEXT NOT NULL, decks TEXT NOT NULL, conf TEXT NOT NULL);
            CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, flds TEXT NOT NULL);
            CREATE TABLE cards (
                id INTEGER PRIMARY KEY,
                nid INTEGER NOT NULL,
                did INTEGER NOT NULL,
                ord INTEGER NOT NULL,
                odid INTEGER NOT NULL
            );
            CREATE TABLE revlog (id INTEGER PRIMARY KEY, cid INTEGER NOT NULL, ease INTEGER NOT NULL);
            ",
        )
        .unwrap();
    let models = serde_json::json!({
        "20": {
            "id": 20,
            "flds": [{"name": "Front"}, {"name": "Back"}],
            "tmpls": [{
                "qfmt": "{{Front}}",
                "afmt": "{{FrontSide}}<hr id=answer>{{Back}}"
            }]
        }
    })
    .to_string();
    let decks = serde_json::json!({
        "10": {"id": 10, "name": "Study::Biology::Cells"}
    })
    .to_string();
    connection
        .execute(
            "INSERT INTO col (models, decks, conf) VALUES (?1, ?2, ?3)",
            rusqlite::params![models, decks, r#"{"creationOffset":-60}"#],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO notes (id, mid, flds) VALUES (?1, ?2, ?3)",
            rusqlite::params![30_i64, 20_i64, "<div>What is <b>ATP</b>?<br><img src=\"picture one.PNG\"></div>\u{1f}<div>Adenosine triphosphate</div>"],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO cards (id, nid, did, ord, odid) VALUES (40, 30, 10, 0, 0)",
            [],
        )
        .unwrap();
    for (timestamp, ease) in [
        ("2026-08-11T23:30:00Z", 2_i64),
        ("2026-08-13T12:00:00Z", 4_i64),
    ] {
        let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp)
            .unwrap()
            .timestamp_millis();
        connection
            .execute(
                "INSERT INTO revlog (id, cid, ease) VALUES (?1, 40, ?2)",
                rusqlite::params![timestamp, ease],
            )
            .unwrap();
    }
    drop(connection);

    let file = fs::File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    archive.start_file("collection.anki21", options).unwrap();
    let mut database_file = fs::File::open(&database).unwrap();
    let mut bytes = Vec::new();
    database_file.read_to_end(&mut bytes).unwrap();
    archive.write_all(&bytes).unwrap();
    archive.start_file("media", options).unwrap();
    archive
        .write_all(
            serde_json::json!({"0": "picture one.PNG"})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    if include_media {
        archive.start_file("0", options).unwrap();
        archive.write_all(b"not really a png").unwrap();
    }
    archive.finish().unwrap();
    fs::remove_file(database).unwrap();
}

#[derive(Clone, PartialEq, Message)]
struct FixtureTemplateConfig {
    #[prost(string, tag = "1")]
    question: String,
    #[prost(string, tag = "2")]
    answer: String,
}

#[derive(Clone, PartialEq, Message)]
struct FixtureMediaEntries {
    #[prost(message, repeated, tag = "1")]
    entries: Vec<FixtureMediaEntry>,
}

#[derive(Clone, PartialEq, Message)]
struct FixtureMediaEntry {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(uint32, tag = "2")]
    size: u32,
    #[prost(bytes = "vec", tag = "3")]
    sha1: Vec<u8>,
}

fn write_modern_colpkg_fixture(path: &Path) {
    let database = path.with_extension("anki21b.sqlite");
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "
            CREATE TABLE col (models TEXT NOT NULL, decks TEXT NOT NULL, conf TEXT NOT NULL);
            CREATE TABLE config (key TEXT PRIMARY KEY, val BLOB NOT NULL);
            CREATE TABLE decks (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE fields (ntid INTEGER NOT NULL, ord INTEGER NOT NULL, name TEXT NOT NULL);
            CREATE TABLE templates (ntid INTEGER NOT NULL, ord INTEGER NOT NULL, config BLOB NOT NULL);
            CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, flds TEXT NOT NULL);
            CREATE TABLE cards (
                id INTEGER PRIMARY KEY,
                nid INTEGER NOT NULL,
                did INTEGER NOT NULL,
                ord INTEGER NOT NULL,
                odid INTEGER NOT NULL
            );
            CREATE TABLE revlog (id INTEGER PRIMARY KEY, cid INTEGER NOT NULL, ease INTEGER NOT NULL);
            INSERT INTO col (models, decks, conf) VALUES ('{}', '{}', '{}');
            INSERT INTO config (key, val) VALUES ('creationOffset', '-60');
            INSERT INTO decks (id, name) VALUES (10, 'Modern::Nested');
            INSERT INTO fields (ntid, ord, name) VALUES (20, 0, 'Front'), (20, 1, 'Back');
            INSERT INTO notes (id, mid, flds) VALUES (
                30, 20,
                '<div>Modern <b>front</b><img src=modern.jpg></div>\u{001f}<div>Modern back</div>'
            );
            INSERT INTO cards (id, nid, did, ord, odid) VALUES (40, 30, 10, 0, 0);
            INSERT INTO revlog (id, cid, ease) VALUES (1786647600000, 40, 3);
            ",
        )
        .unwrap();
    let template = FixtureTemplateConfig {
        question: "{{Front}}".to_owned(),
        answer: "{{FrontSide}}<hr id=answer>{{Back}}".to_owned(),
    }
    .encode_to_vec();
    connection
        .execute(
            "INSERT INTO templates (ntid, ord, config) VALUES (20, 0, ?1)",
            [&template],
        )
        .unwrap();
    drop(connection);

    let database_bytes = fs::read(&database).unwrap();
    let manifest = FixtureMediaEntries {
        entries: vec![FixtureMediaEntry {
            name: "modern.jpg".to_owned(),
            size: 12,
            sha1: vec![0; 20],
        }],
    }
    .encode_to_vec();
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    archive.start_file("meta", options).unwrap();
    archive.write_all(&[8, 3]).unwrap();
    archive.start_file("collection.anki21b", options).unwrap();
    archive
        .write_all(&zstd::stream::encode_all(database_bytes.as_slice(), 0).unwrap())
        .unwrap();
    archive.start_file("media", options).unwrap();
    archive
        .write_all(&zstd::stream::encode_all(manifest.as_slice(), 0).unwrap())
        .unwrap();
    archive.start_file("0", options).unwrap();
    archive
        .write_all(&zstd::stream::encode_all(&b"modern media"[..], 0).unwrap())
        .unwrap();
    archive.finish().unwrap();
    fs::remove_file(database).unwrap();
}
