This file defines all assumptions made about the obsidian vault being operated on.

There are many different files in the repo, but the only ones of importance are the .md files with

`type: note` or `type: card`

in the frontrunner.

Any .md docuemtns that do not have a type in their frontrunner, or they explicitly define a type that isnt `note` or `card` is not considered of importance and is not handled by anything this cli does.

### note

A note is a document I wish to create and also revise for using the Incremental Reading method.

It is defined with `type: note` in the frontrunner, and must also have a `priority` field e.g:

```yaml
type: note
priority: 1
tags: [Programming]
description: "Rust Pinning and it's relationship with Futures."
```

A note may also have a history block like so:

```md
<!-- HISTORY:BEGIN -->

| Date       | End Line | Pass |
| ---------- | -------: | ---- |
| 2026-07-27 |       11 | 0    |

<!-- HISTORY:END -->
```

however it is possible for it to have no history block if no history has been recorded yet.

- Pass >= 0 // Monotonically increasing int
- 0 <= End Line <= line_count_of_file // int
- Date <= Now // monotonically increasing date

This crudely mimmicks SuperMemos incremental reading implementation with a few changes. These notes are supposed to be revised/re-read even after completion. The number of times you have read the note is shown by 'Pass' which tells you which pass you are on. Pass of 0 means you have not read it to completion even once yet.

The algorithm for ranking which reading material to show, should closely mimmick SuperMemos, but should also encorporate `priority` and the table rows. The algorithm must not use 'End Line' in any calculations.

This should enable a workflow where I can gradually read/rewrite/edit a document. Then once I have completed one pass, (such that there exists an entry with Pass > 0), then it should reasonably reduce the ranking. When it pops up again, I will revise and possibly rewrite again. Gradually, in-line with SuperMemos Incremental Reading strategy. When I mark something with a new entry in the table, I expect to immediately move onto something else for a bit and return to this later.

### card

A card is a flashcard I wish to revise for using the ANKI FSRS algorithm.

It is defined with `type: card` in the frontrunner, and must also have a `priority` field e.g:

```yaml
type: card
priority: 5
tags: [System Design]
```

A card must have a 'front' block like so:

<!-- FRONT:BEGIN -->

Name two mechanisms used to preserve correctness under retries and concurrency.

<!-- FRONT:END -->

A card may also have a history block like so:

```md
<!-- HISTORY:BEGIN -->

| Date       | Rating |
| ---------- | -----: |
| 2026-07-27 |      1 |
| 2026-07-27 |      4 |
| 2026-07-31 |      4 |

<!-- HISTORY:END -->
```

however it is possible for it to have no history block if no history has been recorded yet.

- Rating: 1..=4 // int
- Date <= Now // monotonically increasing date

## General

priority: 0..=10. When something is marked with priority 0 it is still valid, but it will be removed from all rankings. A priority of 10 is the highest rank.

## Requirements

I want a cli tool to:

- List items. Searches the entire repository and returns a newline separated list of filenames. You can optionally add a flags to display the following:
  - type
  - Priority
  - Score
  - Due date
  - Status
  - Age
  - Difficulty
  - Retreivability

all decimal alues rounded to 2.dp

Do not support any tabular formatting. It should not even show the column names.

- Audit. Takes a list of newline separated filepaths as stdin, then checks all the files that they satisfy the invariants listed, inclusing all correct value domains and required features.
- Update the priority of files
- Add / remove tags from files
