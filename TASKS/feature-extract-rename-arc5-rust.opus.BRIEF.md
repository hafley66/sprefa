# Brief: extract rename, arc 5: the Rust arm over syn

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-rename.PLAN.md` in full (contract :269-521, arcs table :556 row 5 is yours, receipts :577), the landed TS arm `src/lang/ts_rename.rs` (#511/#514/#516) as the reference implementation of the `Rename` trait, `src/lang/rust.rs:57` (`build_line_starts`) and `:81` (`syn_span`), both `pub(crate)`, and `src/lang/rust_rehome.rs` for how the Rust move arm walks `use` paths (read only). User decision (Chris, 2026-08-27): Rust identifier spans come from `syn`, the crate already in `Cargo.toml:53`; no new crate. `tests/3_move_rust.rs` shows the self-copy oracle shape (crate copied to a temp dir, path deps re-aimed, `cargo check`).

## First action
```bash
git merge --ff-only c35ae28a7bff21820e4e0afbe930fd1b096f762b   # STOP AND REPORT on failure
```

## Files you own
- new: `v6/sprefa-extract/src/lang/rust_rename.rs`, `tests/5_rename_rust.rs`, `tests/fixtures/rust_rename/local/{before,after}/` (a two-file crate: `src/lib.rs` + `src/util.rs`)
- `src/lang/mod.rs`: the `renames()` roster line (`&[&TsSource, &RustSource]`) and the `pub mod rust_rename;` line, nothing else
- new issue: `issuectl new -t feature --slug extract-rename-arc5-rust --title "extract rename: arc 5, the Rust arm over syn" -a chris -p normal -l extract -l rename --description "plans/2026-08-27-extract-rename.PLAN.md arc 5"`; tick it as its own commit AFTER the code commit.
FORBIDDEN: `src/types.rs` (if a `Rename` method or `RenameStop` variant is missing for Rust, STOP and hail with the line), `src/0_rename.rs`, `src/rename_cx.rs`, `src/2_move_text.rs`, `src/lang/ts_rename.rs`, `src/lang/rust.rs`, `src/lang/rust_rehome.rs`, `tests/3_move_rust.rs`, `tests/4_rename_ts.rs`, `tests/6_kind_vocab.rs`, `tests/fixtures/kind_vocab/**`, `tests/fixtures/ts_rename/**`, everything under `v6/sprefa-engine-rs` and `v6/sprefa-store`. Never commit `Cargo.lock`. Another lane owns `0_rename.rs`/`2_move_text.rs`/`ts_rename.rs` concurrently (arc 4, `--text-refs`); do not touch them.

## Seats (plan row 5)
Definition: item idents (`rust.rs:266-292` shows the `ident.span()` seats for struct/enum/trait/type; add fn, const, static, mod). References inside the anchor file and every file that names it: the trailing segment of a `use` path (`use crate::m::Old` -> `Old` only; `use crate::m::{Old, Other}` -> the `Old` seat; `use crate::m::Old as Local` -> `Old` only, `Local` stays), the trailing segment of an `ExprPath`/`TypePath` (`m::Old::new()`, `Old { .. }`, `: Old`, `impl Old`, `&Old`), `ExprMethodCall::method` when the request names a method. A glob importer `use crate::m::*` that reaches the symbol is a `Dynamic` stop with the `use` span. Macro bodies (`macro_rules!`, attribute args, `format!` strings) are never rewritten: report them through `Dynamic` too. Module resolution: the anchor file's module path from its path (`src/a/mod.rs`, `src/a.rs`, `src/lib.rs`), matching `rust_rehome.rs`'s rules; a `use` whose resolved path is another module's same-named item is NOT a seat (test it).

## Fail-first tests (`tests/5_rename_rust.rs`)
1. `rust_rename_matches_the_hand_written_after`: `tests/fixtures/rust_rename/local/before` -> `extract rename src/util.rs#Helper Tool --commit` -> `diff -rq` vs `after/` = zero entries. The fixture holds: the struct def, an `impl Helper`, a `use crate::util::Helper` in `lib.rs`, a `Helper::new()` call, a `: Helper` type position, a `use crate::util::Helper as H` in a nested `mod tests` whose body uses `H` (stays), a `format!("Helper")` string (stays), and a second unrelated `mod other { pub struct Helper; }` whose `Helper` stays.
2. `glob_importer_is_a_dynamic_stop`: a `before` variant with `use crate::util::*;` -> exit 6, tree byte-identical.
3. `self_rename_is_judged_by_rustc`: the plan's oracle. Copy `v6/sprefa-extract` to a temp dir with path deps re-aimed exactly as `tests/3_move_rust.rs` does, run `extract rename src/rename_cx.rs#RenameCx SymbolCx --commit`, then `cargo check --features cli` in the copy exits 0. `#[ignore]` with the measured seconds in the attribute if over 10 s; run once by hand and paste the time.
Write each first, paste the failing line in the commit body, then implement.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background): 0 failures, count; `5_rename_rust` count; the self-rename time.
- `grep -n 'panic!\|unwrap()' src/lang/rust_rename.rs` = 0 lines.
- `git diff c35ae28a7bff21820e4e0afbe930fd1b096f762b --stat`: only owned files; `cargo fmt`; no new `eprintln!`; the `4_rename_ts` battery unchanged (10 passed).

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth, "support". Descriptive identifiers. Async banned. A stop is a `RenameStop`, never a panic. Issue tick as its own commit AFTER the code commit.

## Delivery
One PR against `origin/main`, title `extract rename: arc 5, the Rust arm over syn`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts, diff -rq and cargo check receipts>"`.
