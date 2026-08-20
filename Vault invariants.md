# Vault invariants

This is the normative vault format and scheduling specification.

## General

- Managed documents are Markdown files with YAML frontmatter `type: note` or `type: card`.
- Dates are valid `YYYY-MM-DD` local calendar dates, no later than today.
- Scores are deterministic, range from 0 to 1, and rank larger values first. Calculations retain full precision until output.
- Queue, filtering, and rotation policies belong to downstream tools.

### History blocks

A document may have at most one history block:

```md
<!-- HISTORY:BEGIN -->

...

<!-- HISTORY:END -->
```

- Markers and the type-specific table schema must be exact.
- Rows must be contiguous and dates non-decreasing; same-day rows are valid.
- No block, or a valid table without data rows, means no history.
- Multiple, nested, unclosed, or malformed blocks are invalid.
- Validate history only for selected columns that need it. Report required invalid history as `?`.

## List columns

| Column                | Note output               | Card output                                       |
| --------------------- | ------------------------- | ------------------------------------------------- |
| `type`                | `note`                    | `card`                                            |
| `priority`            | Integer `0..=10`          | `-`                                               |
| `desired retention`   | `-`                       | Integer `0..=99`                                  |
| `predicted retention` | `-`                       | Integer `0..=99`, or `-` without history          |
| `difficulty`          | `-`                       | `0..=1` to three decimals, or `-` without history |
| `score`               | `0..=1` to three decimals | `0..=1` to three decimals                         |

An invalid value required by a selected column outputs `?`.

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
