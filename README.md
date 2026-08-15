# retent

`retent` is a Markdown-native incremental-learning queue. Run it in a vault; only `.md` files whose leading YAML front matter contains `type: note|card` and integer `priority: 1..=10` participate. All state is replayed from the marked history table—no database or scheduler fields are written.

```console
retent audit missing
retent audit invalid
retent position notes/article.md 241 --date 2026-08-16
retent rate cards/question.md 3 --date 2026-08-16
retent import anki collection.colpkg [--output my-vault]
retent list --paths [--filter FILTER] | retent format-list FIELD --style flow|block|toggle
retent list [OPTIONS]
retent queue
retent next [OPTIONS]
retent update priority PRIORITY --files-from FILE [--root ROOT]
retent update tags add TAG... --files-from FILE [--existing keep|overwrite] [--root ROOT]
retent update tags rename FROM TO --files-from FILE [--root ROOT]
retent update tags remove TAG... --files-from FILE [--root ROOT]
```

`format-list` reads newline-delimited Markdown paths from standard input and
updates them after preflighting the complete selection. It changes only the
named top-level frontmatter sequence; use `--style flow` for `tags: [one, two]`,
`--style block` for YAML dash-list syntax, or `--style toggle` to switch between
them. For example:

```console
retent list --paths --filter 'tags.any(rust)' | retent format-list tags --style flow
```

An empty sequence remains `[]`, because YAML has no lossless block-list syntax
for an empty sequence. Flow conversion rejects per-item comments and multiline
block items rather than dropping or relocating their content.

`import anki` creates a flat vault of `type: card` Markdown files from an Anki
collection package. If `--output` is omitted, the vault is created beside the
archive using its filename without `.colpkg`. Anki HTML is converted to plain
Markdown, review-log ratings become Retent history, and every component of a
nested deck name becomes a card tag. Package media is copied into `images/` and
references are rewritten to stable UUID-like filenames.

Imports are resumable: card and media names are deterministically derived from
their Anki identities, files are created atomically without overwriting existing
ones, and every skipped file is reported. Item-level copy errors are shown while
the remaining items continue; fix the error and run the same command again to
resume.

Ratings are `1=Again`, `2=Hard`, `3=Good`, `4=Easy`. Cards use default FSRS parameters at 85% desired retention. Notes use a topic cadence derived from priority, review dates, pass, and presentations in the current pass: `ceil(clamp(2^(3p) × (1.10+0.15p)^(n-1) × 4^(pass-1) × (1+0.5 ln(1+exposure)), 1, 3650))`, where `p = priority / 10` and prior exposure has a 30-day half-life. `End Line` is resume-only state and never affects scheduling.

`list` includes upcoming items. `queue` shows items due today from the current
directory and takes no options. `next` shows the first due item. All three use the
same table. Rows are truncated to one line; use `--wrap` with `list` or `next` to
show full cells.

`list --plain` and `next --plain` emit tab-separated fields: rank, type, priority,
status, due date, age days, interval days, score, and path. A new item has an empty
interval. `list --paths` emits root-relative paths only.

`list` and `next` accept metadata filters:

```console
retent list --filter 'priority >= 5'
retent list --filter 'tags.any(foo, bar) & tags.none(baz)'
retent list --filter '(tags.all(foo, bar) or priority = 10) and not tags.any(archived)'
```

Scalar operators are `=`, `!=`, `<`, `<=`, `>`, and `>=`. Tag operations are
`tags.all(...)`, `tags.any(...)`, `tags.none(...)`, and `tags.exact(...)`.
Combine them with `and`/`&`, `or`/`|`, `not`/`!`, and parentheses. Quote values
containing spaces or filter punctuation, such as `tags.any("machine learning")`.

Bulk metadata updates read newline-delimited paths from `--files-from`. Use `-`
to read standard input, making `list --paths` and other path-producing tools
composable with every update operation:

```console
retent list --paths --filter 'tags.any(machine-learning)' |
  retent update priority 3 --files-from -
retent list --paths --filter 'priority <= 3' |
  retent update tags add reviewed important --files-from -
retent list --paths --filter 'tags.any(old)' |
  retent update tags add replacement --existing overwrite --files-from -
retent list --paths --filter 'tags.any(old) & priority <= 5' |
  retent update tags rename old new --files-from -
retent list --paths --filter 'tags.any(archived, stale)' |
  retent update tags remove archived stale --files-from -
```

Paths emitted by `list --paths` are relative to its root. When using `--root`,
pass the same root to `update`; relative input paths are resolved beneath it.
Absolute paths are accepted only when they are inside the root. Blank lines and
duplicate paths are ignored. A regular file can also be passed to
`--files-from` instead of standard input.

Tag addition defaults to `--existing keep`, retaining existing tags and
appending only tags that are not already present. `--existing overwrite`
replaces the existing tag list. Add, overwrite, and rename all deduplicate tag
collisions while retaining first-seen order. Every changed file is validated
and replaced atomically; an invalid selected file aborts the batch before
changes are written.

History blocks use `<!-- HISTORY:BEGIN -->` and `<!-- HISTORY:END -->` around a `Date | End Line | Pass` note table or `Date | Rating` card table. `position` and `rate` atomically splice that block while preserving the rest of the file.

Develop and verify with `nix develop`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `nix build`, and `nix flake check`.
