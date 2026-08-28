# Brief: `extract move --relocate-mod` for Rust (issue move-rust-relocate-mod)

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md` (port rank 2),
`plans/2026-08-26-extract-move-rehome-trait.PLAN.md`,
`issues/move-rust-relocate-mod/item.md`, and PR #489 (`gh pr view 489`).

User decision (Chris): every language is its own impl. No `match`/`if` on
language anywhere in the move core. You add behaviour INSIDE the Rust impl.

## First action

```bash
git merge --ff-only afa481059   # STOP AND REPORT on failure
```

## Files you own

- `v6/sprefa-extract/src/lang/rust_rehome.rs`
- `v6/sprefa-extract/src/lang/rust.rs` (visibility widenings only)
- `v6/sprefa-extract/tests/3_move_rust.rs` and `tests/fixtures/rust_move/**`
- `issues/move-rust-relocate-mod/item.md`

FORBIDDEN: `src/0_move.rs`, `src/move_stage.rs`, `src/move_cx.rs`,
`src/types.rs`, `src/2_move_text.rs`, `tests/1_move.rs`. Another lane
owns `0_move.rs`/`move_stage.rs` right now. The flag must reach your impl
without touching them: read it from `MoveCx` if it already carries the raw
argv/options; if it does not, STOP and hail with the exact field you need.

## What to build

Today (`rust_rehome.rs`, PR #489): a `mod x;` whose file leaves its rustc
location gains `#[path = ".."]`; the module name and every `use crate::..`
path survive. Add the v5/v1 strategy as an opt-in:

`--relocate-mod`: when `src/a.rs` -> `src/util/a.rs` and `src/util/mod.rs`
(or `src/util.rs`) exists or is created by the same batch:
1. Remove `mod a;` (and its attributes/doc comments) from the old parent.
2. Insert `pub mod a;` (or `mod a;` if it was private and every user is
   inside `util`) into the new parent, sorted among existing `mod` items.
3. Respell every `use crate::a::...` / `crate::a::` path expression /
   `super::a` reference in the crate to `crate::util::a::...`, via syn
   (`UseTree` fold at `rust.rs:1327-1353`); never regex source.
4. If no parent module file exists at the destination and the batch does
   not create one: named error, no partial edit.

Precedent to read: v5 `src/rspath.rs` (module-path arithmetic, crate roots),
`src/lib.rs:1676 rust_mod_surgery`, `src/refactor.rs:64-104`; v1
`~/projects/sprefa-archive-20260428/crates/rs/src/lib.rs:270 rewrite_module_refs`.

## Fail-first tests (`tests/3_move_rust.rs`, fixture `rust_move/relocate/`)

- `relocate_mod_moves_the_decl_into_the_new_parent`
- `relocate_mod_respells_use_paths_crate_wide` (`use crate::a::f` -> `use crate::util::a::f`, and a `super::a::f` in a sibling)
- `relocate_mod_with_no_parent_module_is_a_named_error`
- `default_strategy_is_unchanged` (same fixture without the flag = #[path], byte-identical to #489 behaviour)
- oracle: `cargo check` green on the relocated fixture (cap 10 s; `#[ignore]` with measured time if over, run once by hand).

## Receipts (PR body)

- `cargo test -p sprefa-extract --features cli`: full battery 0 failures; `3_move_rust` count.
- `git diff afa481059 --stat` shows only the owned files.
- `git grep -n 'CorpusLang::\|ExtractLang::\|match .*lang' v6/sprefa-extract/src/0_move.rs v6/sprefa-extract/src/move_cx.rs v6/sprefa-extract/src/move_stage.rs` prints nothing.
- `cargo fmt`; no `eprintln!` in `src/**`; 10-second law on every test.

## Style

Comment budget: constraints only. Banned words: provenance, substrate,
load-bearing, regime, refusal, ground truth. Descriptive identifiers.

## Delivery

One PR against `origin/main`, title `extract move: --relocate-mod re-paths Rust use paths (rank 2)`.
Hail on post and on block:
`boop beep hail sprefa-coordinator --from <your-lane-name> --body "<PR#, test counts, cargo check result>"`.
Do not merge.
