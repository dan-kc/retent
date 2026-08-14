# retent

`retent` is a Markdown-native incremental-learning queue. Run it in a vault; only `.md` files whose leading YAML front matter contains `type: note|card` and integer `priority: 0..=100` participate. All state is replayed from the marked history table—no database or scheduler fields are written.

```console
retent audit missing
retent audit invalid
retent position notes/article.md 241 --date 2026-08-16
retent rate cards/question.md 3 --date 2026-08-16
retent import anki collection.colpkg [--output my-vault]
retent list [--filter EXPRESSION] [--notes-only|--cards-only] [--limit N] [--as-of YYYY-MM-DD] [--plain|--paths] [--wrap]
retent queue
retent next [--filter EXPRESSION] [--plain] [--wrap]
```

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

Ratings are `1=Again`, `2=Hard`, `3=Good`, `4=Easy`. Cards use default FSRS parameters at 85% desired retention. Notes use a topic cadence derived from priority, review dates, pass, and presentations in the current pass: `ceil(clamp(2^(3p) × (1.10+0.15p)^(n-1) × 4^(pass-1) × (1+0.5 ln(1+exposure)), 1, 3650))`, where prior exposure has a 30-day half-life. `End Line` is resume-only state and never affects scheduling.

`list`, `queue`, and `next` render the same responsive terminal table by default.
`list` includes all scheduled entries, including upcoming ones. `queue` is the
zero-option due-only list view for the current directory and date, while `next`
is that view limited to its first item. Use `list` whenever you need filtering,
type selection, limits, a different date or root, or alternate output. `--plain`
emits headerless, tab-separated records for pipelines: rank, type, priority,
status, due date, age days, interval days, score, and path. A new item's missing
interval is an empty field.
Use `list --paths` to emit only root-relative file paths, one per line.
Table rows stay on one line by default and truncate long cells with an ellipsis.
Pass `--wrap` to preserve complete cell contents across as many physical lines as
the current terminal width requires.

`--filter` narrows entries using metadata expressions; `list` and `next` apply it
before scheduling. Scalar comparisons and tag-set operations
compose with either word or symbolic boolean operators:

```console
retent list --filter 'priority >= 50'
retent list --filter 'tags.any(foo, bar) & tags.none(baz)'
retent list --filter '(tags.all(foo, bar) or priority = 100) and not tags.any(archived)'
```

Scalar operators are `=`, `!=`, `<`, `<=`, `>`, and `>=`. Tag operations are
`tags.all(...)`, `tags.any(...)`, `tags.none(...)`, and `tags.exact(...)`.
Composition accepts `and`/`&`, `or`/`|`, `not`/`!`, and parentheses. Quote tag
values containing spaces, for example `tags.any("machine learning")`.

History blocks use `<!-- HISTORY:BEGIN -->` and `<!-- HISTORY:END -->` around a `Date | End Line | Pass` note table or `Date | Rating` card table. `position` and `rate` atomically splice that block while preserving the rest of the file.

Develop and verify with `nix develop`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `nix build`, and `nix flake check`.
