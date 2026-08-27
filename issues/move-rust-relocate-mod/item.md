---
created: 2026-08-27
updated: 2026-08-27
type: feature
assignee: chris
status: open
priority: high
epic: extract-move-parity
labels: [extract, refactor, rust]
---

# extract move --relocate-mod: Rust use-path re-pathing when a module changes parent

## Description

v5 src/rspath.rs + f859585ed mod surgery, v1 crates/rs/src/lib.rs:270. Second strategy in RustSource::respell behind a flag; default stays #[path]. Plan: plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md. Receipt: fixture: use crate::a::f -> use crate::util::a::f, cargo check green, tests/3_move_rust.rs

## Built

`--relocate-mod` lives entirely in `src/lang/rust_rehome.rs`; nothing in
`0_move.rs`, `move_cx.rs` or `move_stage.rs` names a language. The flag was
already on `MoveCx` (`relocate_mod()`, PR #491). The strategy lifts the `mod`
decl into the module owning the destination directory, inserts it sorted among
that file's own `mod` items, and respells every `crate::` / `super::` / `self::`
/ bare path in the crate through module-path arithmetic on the file layout
(read off syn, never regexed).

The missing-parent case answers through `Rehome::plan_errors` (PR #500), which
stops the run before `import_refs` is ever asked: `rust: --relocate-mod: ... has
no parent module file (expected src/nope.rs or src/nope/mod.rs)`, exit 2, tree
clean.

Receipts: `tests/3_move_rust.rs` 12 pass / 1 ignored; the `relocate/` fixture's
own `cargo check --offline` is green in-test.

## Left open

- A decl carrying `#[path]`, or one inside an inline `mod x { .. }`, falls back
  to the default `#[path]` arm: both spell a module tree the file layout does
  not, and the arithmetic reads layout.
- A `super::`/`self::`/bare path written INSIDE a file that is itself moving is
  left alone; only its `crate::` paths are respelled.
