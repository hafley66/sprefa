---
created: 2026-08-27
updated: 2026-08-27
type: feature
assignee: chris
status: done
priority: low
epic: extract-move-parity
labels: [extract, refactor, core]
closed: 2026-08-27
---

# extract move --root repeatable: one MoveCx per root

## Description

v5 --repo '*' fan-out. Core only. Plan: plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md. Receipt: two roots in one run, each rewritten, one receipt per root

## Receipt

- [x] `--root` repeatable: every move row falls under exactly one root; a row
  under none or under two is a named error before any stage
  (`tests/1_move.rs::a_move_under_no_root_is_a_named_error_with_zero_edits`).
- [x] One `MoveCx`, one Plan, one soopy StageRequest per root, in root order;
  `--verify` runs once after every root committed and rolls every root back
  last-root-first (`tests/1_move.rs::verify_failure_rolls_back_every_root`,
  `::two_roots_each_rewrite_their_own_importers`).
- [x] Zero roots is byte-identical to before
  (`tests/1_move.rs::no_root_flag_is_byte_identical_to_before`).
- Gate: `cargo test -p sprefa-extract --features cli`, 0 failures, `1_move` 20.
  Commit 8d7c7bd8f.
