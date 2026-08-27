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
/ bare path in the crate through module-path arithmetic on the file layout.

Receipts: `tests/3_move_rust.rs` 12 pass / 1 ignored; the fixture's own
`cargo check` is green in-test; `cargo check --features cli` green after
relocating this crate's `src/lang/ts_resolve.rs` into `src/lang/prolog/`.

## Left open

- **The named error is a `panic!`.** `Rehome::respell` returns
  `Option<Respell>` and `import_refs` returns `Vec<ImportRef>`, so an arm has
  no channel to end a run. The missing-parent case ends the process before any
  stage is built (no partial edit), but the message wears a panic frame.
  `respell -> Result<Option<Respell>, String>` (or a `Rehome::plan_errors`
  method) in `types.rs` would make it a plain `Plan::build` error.
- A decl carrying `#[path]`, or one inside an inline `mod x { .. }`, falls back
  to the default `#[path]` arm: both spell a module tree the file layout does
  not, and the arithmetic reads layout.
- A `super::`/`self::`/bare path written INSIDE a file that is itself moving is
  left alone; only its `crate::` paths are respelled.
