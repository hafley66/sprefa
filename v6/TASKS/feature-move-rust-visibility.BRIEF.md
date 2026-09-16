# Brief: private -> pub(crate) promotion on Rust move (issue move-rust-visibility-promotion, rank 3)

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md` (rank 3),
`issues/move-rust-visibility-promotion/item.md`, and the merged rank 2 PR
(`gh pr view 499`) for `--relocate-mod` in `src/lang/rust_rehome.rs`.

User decision (Chris): every language is its own impl; no `match`/`if` on
language anywhere in the move core. You change the Rust impl only.

## First action
```bash
git merge --ff-only b913779ff   # STOP AND REPORT on failure
```

## Files you own
- `v6/sprefa-extract/src/lang/rust_rehome.rs`, `src/lang/rust.rs` (visibility widenings only)
- `tests/3_move_rust.rs` (append), fixtures `tests/fixtures/rust_move/visibility/**`
- `issues/move-rust-visibility-promotion/item.md` (tick AC as its OWN commit, subject `issues: ...`)
FORBIDDEN: `src/0_move.rs`, `src/move_cx.rs`, `src/move_stage.rs`, `src/types.rs`, every other `src/lang/*`. If the trait needs a method, STOP and hail with the exact signature; `Rehome::plan_errors` exists for named errors.

## What to build (v5 precedent: repo root commit `f859585ed`, `src/lib.rs:1676 rust_mod_surgery`, `src/refactor.rs:64-104`)
Under `--relocate-mod` only: when a moved module's item (fn, struct, enum, const, static, type, trait, mod) is private (no `pub`) and after the relocation it is referenced from a module that is no longer a descendant of its new parent, widen the item to `pub(crate)`. Never widen to `pub`. Never touch items that stay reachable. Items already `pub(crate)`/`pub(super)`/`pub` untouched. Reference detection uses the same syn `use` walk rank 2 built (`use crate::..`, `super::`, `self::` paths that name the item) plus path expressions; a private item used only inside its own module stays private. Emit each widening as a `Respell` on the `fn`/`struct`/... keyword span with text `pub(crate) <keyword>`, reported on dry run as `widen <file>:<line> <item> -> pub(crate)`.

## Fail-first tests (`tests/3_move_rust.rs`, fixture `rust_move/visibility/`)
1. `a_private_fn_used_by_a_sibling_after_relocation_becomes_pub_crate`
2. `a_private_fn_used_only_inside_its_module_stays_private`
3. `an_already_pub_item_is_untouched`
4. `without_relocate_mod_nothing_is_widened`
5. oracle: `cargo check` green on the fixture after `--relocate-mod --commit` (cap 10 s; `#[ignore]` with measured time if over; run once by hand for the PR body)

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background): full battery 0 failures; `3_move_rust` count.
- `git diff b913779ff --stat` shows only owned files.
- `git grep -n 'CorpusLang::\|ExtractLang::\|match .*lang' v6/sprefa-extract/src/0_move.rs v6/sprefa-extract/src/move_cx.rs v6/sprefa-extract/src/move_stage.rs` prints nothing.
- `cargo fmt`; no `eprintln!` in `src/**`; 10-second law on every test.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth. Descriptive identifiers.

## Delivery
One PR against `origin/main`, title `extract move: pub(crate) promotion under --relocate-mod (rank 3)`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts>"`.
