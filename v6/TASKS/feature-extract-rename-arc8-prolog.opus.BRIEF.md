# Brief: extract rename, arc 8: the Prolog arm over tree-sitter-prolog

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-rename.PLAN.md` in full (contract :269-521, receipts :577), the landed Rust arm `v6/sprefa-extract/src/lang/rust_rename.rs` (PR #518) as the reference `Rename` impl, the `Rename` trait at `src/types.rs:2184-2207` and `RenameStop` near `src/types.rs:2143`, `src/lang/prolog/_0_source.rs` (`PrologSource` :22, the name/arity resolution `tests/0_prolog.rs:70` pins, the variable scope walk :687-724) and `src/lang/prolog/_1_rehome.rs` (`use_module` / `module($NAME, $EXPORTS)` handling at :37, :45, :259; read only). Parser: `tree-sitter-prolog` already in `Cargo.toml:93`; no new crate.

## First action
```bash
git merge --ff-only e7cd9b883623acbbc19389dd4e2109e4ef1b235b   # STOP AND REPORT on failure
```

## Files you own
- new: `v6/sprefa-extract/src/lang/prolog/_2_rename.rs`, `tests/8_rename_prolog.rs`, `tests/fixtures/prolog_rename/local/{before,after}/` (two modules: `util.pl` with `:- module(util, [helper/2]).` and `main.pl` with `:- use_module(util).`)
- `src/lang/prolog/mod.rs`: the `mod _2_rename;` line.
- `src/lang/mod.rs`: ONLY adding `&PrologSource` to the `renames()` roster at :107-109. A concurrent lane adds `&KotlinSource` to the same line; the coordinator resolves that one-line conflict.
- new issue: `issuectl new -t feature --slug extract-rename-arc8-prolog --title "extract rename: arc 8, the Prolog arm" -a chris -p normal -l extract -l rename --description "plans/2026-08-27-extract-rename.PLAN.md, Prolog arm, brief shape of arc 5"`; tick it as its own commit AFTER the code commit.
FORBIDDEN: `src/types.rs` (a missing `Rename` method or `RenameStop` variant = STOP and hail with the line), `src/0_rename.rs`, `src/1_rename_verify.rs`, `src/rename_cx.rs`, `src/2_move_text.rs`, `src/lang/ts_rename.rs`, `src/lang/rust_rename.rs`, `src/lang/kotlin*.rs`, `src/lang/prolog/_0_source.rs`, `src/lang/prolog/_1_rehome.rs`, every existing `tests/*.rs` and `tests/fixtures/**` other than yours, everything under `v6/prolog`, `v6/sprefa-engine-rs`, `v6/sprefa-store`. Never commit `Cargo.lock`.

## Symbol law
A Prolog symbol is a predicate name at a fixed arity. `<file>#helper` renames `helper/N` for every arity the anchor file defines; when the anchor defines the name at more than one arity and no `--at` is given, that is `Ambiguous` (arc 2 table) and the `--at` byte picks the clause head. Variables are clause-local and never a rename target (stop `NotFound` when `<old>` matches only variables).

## Seats
Definition: every clause head functor of the name/arity in the anchor file, plus its entry in the `:- module(_, [...])` export list and any `:- dynamic`, `:- discontiguous`, `:- multifile`, `:- table` directive naming it. References: body goals with the name/arity in the anchor file and in every file that `use_module`s the anchor (or names it in a `use_module(util, [helper/2])` import list, which is also a seat), `module:helper(...)` qualified goals, `Name/Arity` indicators in `meta_predicate`, `maplist`/`call` first arguments naming it as an atom. A goal built through `=..`, `atom_concat`, or `call/N` with a variable functor is a `Dynamic` stop with the goal span. Atoms inside strings, comments, and `format/2` templates are never rewritten: report through `text_spellings` for `--text-refs`. A same-named predicate at a different arity, or in a module that is not imported, is NOT a seat (test both).

## Fail-first tests (`tests/8_rename_prolog.rs`)
1. `prolog_rename_matches_the_hand_written_after`: `before` -> `extract rename util.pl#helper tool --commit` -> `diff -rq` vs `after/` = zero entries. The fixture holds: two clauses of `helper/2`, the export list entry, a `:- dynamic helper/2.`, a `util:helper(A, B)` qualified goal and a bare `helper(A, B)` goal in `main.pl`, `maplist(helper(x), L)` (stays: partial application is `helper/1` at the call site, so it is a `Dynamic` stop in test 2, NOT part of this fixture), a `helper/3` in the same file (stays), a `format("helper ~w", [X])` (stays), and an `other.pl` module with its own `helper/2` that `main.pl` does not import (stays).
2. `variable_functor_is_a_dynamic_stop`: a `before` variant with `Goal =.. [helper, A, B], call(Goal)` -> exit 6, tree byte-identical.
3. `two_arities_need_at`: anchor defines `helper/2` and `helper/3`, no `--at` -> exit code of `Ambiguous` in the arc 2 table; with `--at <byte of the helper/2 head>` the /2 clauses alone rename.
4. `swipl_loads_the_after_tree`: `swipl -g halt -l after/main.pl` exits 0 (the oracle; swipl is on PATH).
Write each first, paste the failing line in the test header (the `5_rename_rust.rs:5-12` shape), then implement.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND: 0 failures, total count; `8_rename_prolog` count.
- `grep -n 'panic!\|unwrap()' src/lang/prolog/_2_rename.rs` = 0 lines.
- `git diff e7cd9b883623acbbc19389dd4e2109e4ef1b235b --stat`: only owned files; `cargo fmt`; no new `eprintln!`; `4_rename_ts`, `5_rename_rust`, `0_prolog`, `1a_prolog_refs` batteries unchanged.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth, "support". Descriptive identifiers. Async banned. A stop is a `RenameStop`, never a panic.

## Delivery
One PR against `origin/main`, title `extract rename: arc 8, the Prolog arm`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts, diff -rq and swipl receipts>"`.
