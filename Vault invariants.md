# Vault invariants

This is the normative vault format and scheduling specification.

## General

- Managed documents are Markdown files with YAML frontmatter `type: note` or `type: card`.
- A Markdown file is unmanaged unless its frontmatter parses as a YAML mapping with one of those types. This includes missing, unclosed, malformed, and non-mapping frontmatter.
- Managed documents are validated completely before use, independently of requested list columns.
- An unreadable Markdown file or one that is not valid UTF-8 is an invalid entry even when its managed status cannot be determined.
- Dates are valid `YYYY-MM-DD` local calendar dates, no later than today.
- Scores are deterministic, range from 0 to 1, and rank larger values first. Calculations retain full precision until output.
- Queue, filtering, and rotation policies belong to downstream tools.

### Commands

`list` discovers Markdown files recursively, excludes `.git` directories and symlinks, and sorts paths lexically.

- Valid managed documents and unmanaged Markdown files are listed. Invalid entries are skipped without changing the successful exit status.
- Every managed document is fully validated even when no columns, or unrelated columns, are selected.
- Unmanaged files output `-` for every selected column.
- `--absolute-path` replaces the default `./`-prefixed relative path.

`audit` applies the same discovery and validation rules.

- Valid and unmanaged files produce no output.
- Each invalid entry produces one stdout row: `<path>\t<reason>; <reason>`.
- Independently detectable reasons are combined in field, front-block, then history order as applicable.
- Findings are sorted by path. `--absolute-path` replaces the default `./`-prefixed relative path.
- No findings exits successfully. Any finding exits with status 1. Operational failures are written to stderr and also exit with status 1.

`priority` reads newline-separated file paths from standard input. Paths may be relative to the current directory or absolute.

- `increment 1..=10` and `decrement 1..=10` require a YAML frontmatter mapping with an unquoted integer `priority: 0..=10`. A result outside that range is skipped.
- `add 0..=10` adds priority to a frontmatter mapping without priority, or creates frontmatter when none exists. Any existing priority key is skipped, including one with an invalid value.
- `upsert 0..=10` creates missing frontmatter, inserts missing priority, or replaces a unique existing priority value. An already-equal priority succeeds without rewriting the file.
- Document type, extension, body, and managed-document validation do not affect priority editing. Targets must be regular, non-symlink files with valid UTF-8 contents.
- Existing frontmatter must parse as a YAML mapping. Malformed, unclosed, and non-mapping frontmatter is skipped rather than repaired or replaced.
- Edits preserve the UTF-8 BOM, newline style, mapping order, comments, and all text outside the priority scalar. A priority key must be a plain top-level `priority` key with an ordinary same-line scalar to be changed. Quoted keys, flow mappings, multiline or compound values, tags, anchors, and ambiguous representations are skipped when changing them would require rewriting frontmatter.
- New frontmatter contains only priority and is separated from non-empty original content by one blank line.
- Each canonical file path is processed once. Later relative or absolute aliases are skipped; distinct hard-link names remain separate inputs. Blank input rows are ignored.
- Successfully edited or already-satisfied paths are output first, followed by skipped paths. Order within each group follows standard input. Successful rows contain the supplied path; skipped rows contain `<path>\t<reason>`.
- Per-file skips do not change the successful exit status. Command-line, standard-input, current-directory, and standard-output failures exit unsuccessfully.
- Changed files are replaced atomically with their permission bits preserved. A source changed after reading is skipped rather than overwritten. Atomic replacement does not preserve hard-link identity, ACLs, or extended attributes.

Paths and reasons escape backslashes and output-breaking control characters. Paths that are not valid Unicode use lowercase `\xNN` byte escapes.

### History blocks

A document may have at most one history block:

```md
<!-- HISTORY:BEGIN -->

...

<!-- HISTORY:END -->
```

- History marker lines may have surrounding whitespace, but their trimmed text must exactly match the markers above. The type-specific table schema must be exact.
- Rows must be contiguous and dates non-decreasing; same-day rows are valid.
- No block, or a valid table without data rows, means no history.
- Multiple, nested, unclosed, or malformed blocks are invalid.
- History is always validated for managed documents, including when priority or desired retention is zero.

## List columns

| Column                | Note output               | Card output                                       |
| --------------------- | ------------------------- | ------------------------------------------------- |
| `type`                | `note`                    | `card`                                            |
| `priority`            | Integer `0..=10`          | `-`                                               |
| `desired retention`   | `-`                       | Integer `0..=99`                                  |
| `predicted retention` | `-`                       | Integer `0..=99`, or `-` without history          |
| `difficulty`          | `-`                       | `0..=1` to three decimals, or `-` without history |
| `score`               | `0..=1` to three decimals | `0..=1` to three decimals                         |

## Notes

A note requires an unquoted YAML integer `priority: 0..=10`. Ten is highest; zero is valid but scores zero.

Note history uses this schema:

```md
| Date       | End Line | Pass |
| ---------- | -------: | ---- |
| 2026-07-27 |       11 | 0    |
```

- `End Line`: uncapped non-negative integer used only for resuming; it need not fit the file's current length.
- `Pass`: non-negative, non-decreasing integer. Repeats and skips are valid; zero means no completed reading yet.

### Note score

For priority `p`, `P = p / 10`. For each row `i`, let `age_i` be its age in whole days:

```text
H_i = (11 - p) * 2^pass_i
E_i = 2^(-age_i / H_i)
score = P / (1 + sum(E_i))
```

Sum every row, including repeated dates and passes. `End Line` is ignored. No history scores `P`. Output uses three decimal places.

## Cards

A card requires an unquoted YAML integer `desired retention: 0..=99`. Zero means “not considered now” and scores `0.000`; 1..=99 are percentages.

A card requires a front block; its back block is optional.

```md
<!-- FRONT:BEGIN -->

Question text

<!-- FRONT:END -->
```

- Front marker lines may have surrounding whitespace, but their trimmed text must exactly match the markers above.
- A card has exactly one non-nested, closed front block. An empty front block is valid.
- Back-block structure is not validated.

Card history uses this schema:

```md
| Date       | Rating |
| ---------- | -----: |
| 2026-07-27 |      3 |
```

`Rating` is `1=Again`, `2=Hard`, `3=Good`, or `4=Easy`. Replay all rows through FSRS 6.6.1 with default parameters and whole-day intervals.

### Card calculations

- **Predicted retention:** internal FSRS retrievability `R` remains in 0..=1; output is `min(99, round(100R))`, an integer percentage. No history outputs `-`.
- **Difficulty:** normalized FSRS difficulty `(D - 1) / 9` in 0..=1, output to three decimals. No history outputs `-`.
- **Score:** let `t` be whole days since the latest review and `I` the unrounded FSRS target interval for current stability and desired retention. Output `t / (t + I)` to three decimals. No history with non-zero desired retention scores `0.500`.
