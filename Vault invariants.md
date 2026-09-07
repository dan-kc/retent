# Vault invariants

This is the normative vault format and scheduling specification.

## General

- Managed documents are Markdown files whose effective YAML mapping has `type: note` or `type: card`.
- A missing frontmatter block contributes no file-specific values. Unclosed, malformed, and non-mapping frontmatter makes a Markdown file unmanaged even when directory metadata would otherwise manage it.
- A valid frontmatter mapping overrides directory metadata by key. An explicit invalid value or unrecognized type does not fall back to a directory default.
- Managed documents are validated completely before use, independently of requested list columns.
- An unreadable Markdown file or one that is not valid UTF-8 is an invalid entry even when its managed status cannot be determined.
- Dates are valid `YYYY-MM-DD` local calendar dates, no later than today.
- Scores are deterministic, range from 0 to 1, and rank larger values first. Calculations retain full precision until output.
- Queue, filtering, and rotation policies belong to downstream tools.

### Directory metadata

- A regular, non-symlink file named exactly `metadata.yaml` supplies defaults to Markdown files in its directory and descendants. Other names, extensions, case variants, and symlinks are ignored.
- The whole metadata file is one UTF-8 YAML mapping, optionally preceded by a UTF-8 BOM. Frontmatter delimiters are not required. An explicit empty mapping is valid; empty, comment-only, malformed, and non-mapping YAML is invalid metadata.
- Metadata mappings may be partial and may contain unknown keys. Recognized field values are validated only when they are effective for a managed Markdown document.
- Mappings merge by key from the inheritance root through each descendant directory. The nearest value for a key wins, and valid file frontmatter is the most specific mapping.
- An invalid metadata file contributes no values. Valid ancestor and descendant layers continue to apply.
- The nearest ancestor containing a regular-file or directory `.git` is the inheritance root. Without one, inheritance begins at the current directory. Markdown discovery remains limited to the current-directory subtree.

### Commands

`list` discovers Markdown files recursively, excludes `.git` directories and symlinks, and sorts paths lexically.

- Valid managed documents and unmanaged Markdown files are listed. Invalid entries are skipped without changing the successful exit status.
- Invalid metadata files produce no output and do not change the successful exit status.
- Every managed document is fully validated even when no columns, or unrelated columns, are selected.
- Unmanaged files output `-` for every selected column.
- Each output record contains the selected metadata values in requested order, followed by the path. Fields are separated by tabs and records are terminated by NUL. With no selected columns, a record contains only the path and its NUL terminator.
- Paths are output as raw filesystem bytes. Spaces, tabs, newlines, backslashes, and non-Unicode bytes are not escaped. Because the path is the final field, tabs within it do not make the selected metadata fields ambiguous.
- `--absolute-path` replaces the default `./`-prefixed relative path.

`audit` applies the same discovery and validation rules.

- Valid metadata, valid documents, and unmanaged Markdown files produce no output.
- Each unreadable, invalid-UTF-8, malformed, or non-mapping metadata file produces one finding, including participating metadata above the current directory and metadata without Markdown descendants.
- Each invalid entry produces one stdout row: `<path>\t<reason>; <reason>`.
- Independently detectable reasons are combined in field, front-block, then history order as applicable.
- Metadata and document findings are sorted together by path. `--absolute-path` replaces relative paths; inherited metadata above the current directory otherwise uses the appropriate `../` prefix.
- No findings exits successfully. Any finding exits with status 1. Operational failures are written to stderr and also exit with status 1.

`audit` output paths and reasons escape backslashes and output-breaking control characters. Paths that are not valid Unicode use lowercase `\xNN` byte escapes.

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

A note requires an effective unquoted YAML integer `priority: 0..=10`. Ten is highest; zero is valid but scores zero.

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

A card requires an effective unquoted YAML integer `desired retention: 0..=99`. Zero means “not considered now” and scores `0.000`; 1..=99 are percentages.

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
