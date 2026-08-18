This file defines all assumptions made about the obsidian vault being operated on.

There are many different files in the repo, but the only ones of importance are the .md files with

`type: note` or `type: card`

in the frontrunner.

Any .md documents that do not have a type in their frontrunner, or explicitly define a type that isnt `note` or `card` is not considered of importance and is not handled by anything this cli does currently.

### note

A note is a document to revise for using the Incremental Reading method.

It is defined with `type: note` in the frontrunner, and must also have a `priority` field e.g:

```yaml
type: note
priority: 1
tags: [Programming]
description: "Rust Pinning and it's relationship with Futures."
```

priority: 0..=10. When something is marked with priority 0 it is still valid, but it will be removed from all rankings. A priority of 10 is the highest rank for the most imprtant notes.

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
- 0 <= End Line // Uncapped. Do not check if it is <= total line count of file
- Date <= Now // monotonically increasing date

This crudely mimmicks SuperMemos incremental reading implementation with a few changes. These notes are supposed to be revised/re-read even after completion. The number of times you have read the note is shown by 'Pass' which tells you which pass you are on. Pass of 0 means you have not read it to completion even once yet.

The algorithm for ranking which reading material to show, should closely mimmick SuperMemos, however it should only encorporate priority, review dates, pass, randomness, and decayed prior exposure. `End Line` is resume-only state and never affects scheduling, treat it as a superficial field.

### card

A card is a flashcard I wish to revise for using the ANKI FSRS algorithm.

It is defined with `type: card` in the frontrunner, and must have a `desired retention` field e.g:

```yaml
type: card
desired retention: 85
tags: [System Design]
```

A card must have a 'front' block like so:

<!-- FRONT:BEGIN -->

Name two mechanisms used to preserve correctness under retries and concurrency.

<!-- FRONT:END -->

It does not need to have a back. A card may also have a history block like so:

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

Ratings are `1=Again`, `2=Hard`, `3=Good`, and `4=Easy`. Cards use FSRS with the configured desired retention. 
