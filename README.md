# retent

`retent` is a Markdown-native incremental-learning queue. Run it in a vault; only `.md` files whose leading YAML front matter contains `type: note|card` and integer `priority: 0..=100` participate. All state is replayed from the marked history table—no database or scheduler fields are written.

```console
retent audit missing
retent audit invalid
retent position notes/article.md 241 --date 2026-08-16
retent rate cards/question.md 3 --date 2026-08-16
retent queue [--all] [--notes-only|--cards-only] [--limit N]
retent next
```

Ratings are `1=Again`, `2=Hard`, `3=Good`, `4=Easy`. Cards use default FSRS parameters at 85% desired retention. Notes use a topic cadence derived from priority, review dates, pass, and presentations in the current pass: `ceil(clamp(2^(3p) × (1.10+0.15p)^(n-1) × 4^(pass-1) × (1+0.5 ln(1+exposure)), 1, 3650))`, where prior exposure has a 30-day half-life. `End Line` is resume-only state and never affects scheduling.

History blocks use `<!-- HISTORY:BEGIN -->` and `<!-- HISTORY:END -->` around a `Date | End Line | Pass` note table or `Date | Rating` card table. `position` and `rate` atomically splice that block while preserving the rest of the file.

Develop and verify with `nix develop`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `nix build`, and `nix flake check`.
