# retent

`retent` is a Markdown-native incremental-learning queue. A vault contains `.md`
files whose leading YAML front matter has `type: note|card` and an integer
`priority: 1..=10`. Learning state is replayed from Markdown history tables; no
database or cached scheduler fields are written.

```console
retent [--vault PATH] [--config FILE | --no-config] COMMAND

retent audit missing|invalid
retent config show
retent progress FILE --end-line LINE [--date YYYY-MM-DD]
retent rate FILE 1|2|3|4 [--date YYYY-MM-DD]
retent queue [VIEW OPTIONS]
retent next [VIEW OPTIONS]
retent list [VIEW OPTIONS]
retent format-list FIELD --style flow|block|toggle --files-from FILE
retent update priority PRIORITY --files-from FILE
retent update tags add TAG... --files-from FILE
retent update tags set TAG... --files-from FILE
retent update tags rename FROM TO --files-from FILE
retent update tags remove TAG... --files-from FILE
retent import anki COLLECTION.colpkg [--output VAULT]
```

`--vault` is global and may appear before or after a subcommand. Relative
document paths and paths read from `--files-from` are resolved beneath the
effective vault. `progress` is the canonical name for recording note progress;
`position` remains available as an alias.

## Configuration

Retent searches from the current directory upward for `.retent.toml`. The
directory containing the discovered file becomes the default vault. Use
`--config FILE` to load one exact TOML file or `--no-config` to disable discovery.
An explicit `--vault` always wins over the config file's directory.

```toml
version = 1

[scheduling.card]
desired-retention = 0.85

[scheduling.note]
maximum-interval-days = 3650
exposure-half-life-days = 30
pass-multiplier = 4

[queue]
limit = 5
filter = "tags.none(archived)"
type = "all"
format = "table"
wrap = false

[list]
limit = "none"
type = "all"
format = "table"
wrap = false
```

Precedence is built-in defaults, then TOML, then CLI flags. Every configurable
view value has an explicit CLI reset:

- `--no-limit` removes a configured limit.
- `--no-filter` removes a configured filter.
- `--type all` removes a configured type restriction.
- `--no-wrap` disables configured wrapping.
- `--format table|tsv|paths|json` replaces the configured output format.

`limit` must be a positive integer or the string `"none"`. Config parsing is
strict: unknown keys, unsupported versions, invalid filters, and out-of-range
scheduler values are errors. `retent config show` prints every effective value
and whether it came from a built-in, the TOML file, or the CLI.

Scheduling values can also be overridden for one invocation:

```console
retent queue --card-retention 0.90
retent list --note-max-interval 1800 \
  --note-exposure-half-life 45 --note-pass-multiplier 3
```

## Queue views

`queue` displays due and new items and defaults to the first five. `next` uses
the same queue settings and filter but always selects the first item. `list`
also includes upcoming items and has no built-in limit.

All three support:

```console
--as-of YYYY-MM-DD
--filter EXPR | --no-filter
--type all|note|card
--format table|tsv|paths|json
--wrap | --no-wrap
--allow-invalid
```

`queue` and `list` additionally support `--limit COUNT | --no-limit`. Wrapping
only applies to table output. TSV fields are rank, type, priority, status, due
date, age days, interval days, score, and path. Path output contains one
vault-relative path per line. JSON includes those fields plus last-review and
type-specific scheduling details.

Views preflight the complete vault. If any invalid files are found, nothing is
written to stdout and the command fails. `--allow-invalid` explicitly opts into
partial output and reports skipped files on stderr. This keeps path-producing
pipelines from silently mutating an incomplete selection.

Metadata filters support scalar comparisons and tag set operations:

```console
retent list --filter 'priority >= 5'
retent list --filter 'tags.any(foo, bar) & tags.none(baz)'
retent list --filter '(tags.all(foo, bar) or priority = 10) and not tags.any(archived)'
```

Scalar operators are `=`, `!=`, `<`, `<=`, `>`, and `>=`. Tag operations are
`tags.all(...)`, `tags.any(...)`, `tags.none(...)`, and `tags.exact(...)`.
Combine expressions with `and`/`&`, `or`/`|`, `not`/`!`, and parentheses. Quote
values containing spaces or punctuation.

## Bulk edits and formatting

Bulk commands read newline-delimited paths from `--files-from`. Use `-` for
standard input:

```console
retent list --format paths --filter 'tags.any(machine-learning)' |
  retent update priority 3 --files-from -
retent list --format paths --filter 'priority <= 3' |
  retent update tags add reviewed important --files-from -
retent list --format paths --filter 'tags.any(old)' |
  retent update tags set replacement reviewed --files-from -
retent list --format paths --filter 'tags.any(old)' |
  retent update tags rename old new --files-from -
retent list --format paths --filter 'tags.any(archived, stale)' |
  retent update tags remove archived stale --files-from -
```

Blank lines and duplicate paths are ignored. Absolute paths are accepted only
when they are inside the vault. Every selected file is validated before any
write, and replacements are atomic. `tags add` retains existing tags; `tags set`
replaces the complete list. Tag operations deduplicate while retaining
first-seen order.

`format-list` changes only the named top-level frontmatter sequence. `flow`
produces `tags: [one, two]`, `block` produces YAML dash-list syntax, and `toggle`
switches between them:

```console
retent list --format paths --filter 'tags.any(rust)' |
  retent format-list tags --style flow --files-from -
```

An empty sequence remains `[]`. Flow conversion rejects per-item comments and
multiline block items rather than dropping or relocating content.

## Scheduling and import

Ratings are `1=Again`, `2=Hard`, `3=Good`, and `4=Easy`. Cards use FSRS with the
configured desired retention. Notes use priority, review dates, pass,
presentations in the current pass, and decayed prior exposure. `End Line` is
resume-only state and never affects scheduling.

`import anki` creates or resumes a flat Markdown vault from an Anki collection
package. Names are deterministic, existing files are not overwritten, media is
copied into `images/`, and item-level errors can be fixed before rerunning the
same command.

History blocks use `<!-- HISTORY:BEGIN -->` and `<!-- HISTORY:END -->` around a
`Date | End Line | Pass` note table or `Date | Rating` card table. `progress` and
`rate` atomically splice those blocks while preserving the rest of the file.

Develop and verify with `nix develop`, `cargo test`,
`cargo clippy --all-targets --all-features -- -D warnings`, `nix build`, and
`nix flake check`.
