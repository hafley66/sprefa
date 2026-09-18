# Prelude through macrotime brief

Lane branch `feat/prelude-macrotime`. Base `origin/main` d62c631746737f671497bb3df05db86c522a8539 (PR #794 merged).
First action `git merge --ff-only d62c631746737f671497bb3df05db86c522a8539`; failure = stop and report. Preset sonnet.

Decision (Chris 2026-09-18, `plans/v8/2026-09-18-re-edging-two-walks.design.md`
"macrotime is the inner comptime"): every unit passes through macrotime, the
prelude and the macro library included. Shape: staged bootstrap, no file
tiers, no frozen artifact.

## Goal

`[..]` and `^` work inside `prelude/*.dl7` and `macrotime/*.dl7`. Then PR
#794's stopped steps land: the prelude constructor clause pairs and
`caret_mint` use the list literal.

## The stage trace

```
step 0  L0 = compile(macrotime/*, prelude raw)       _8_driver/_2_macro.rs:18-33 standard_macro_program, today's call
step 1  P1 = expand(prelude/*, L0)                   NEW: the prelude unit goes through expand_units_with_macros
step 2  L1 = compile(macrotime/*, P1)                NEW: second library compile against the expanded prelude
step 3  program = expand(program + imports, L1)      today's call, library swapped for L1
step 4  lower(P1 + program)                          lib.rs:166 uses P1 instead of prelude.unit
```

Step 0 must tolerate prelude rules it cannot lower yet (a `[..]` inside a
rule body lowers as `undeclared_relation`, PR #794 receipt). Choose one and
state it in the PR: (i) step 0 compiles the library against the prelude with
those rules dropped and a count logged through `tracing`; or (ii) the macro
library's own compile needs only `prelude/0_constructors.dl7` and
`1_declarations.dl7` (verify by compiling it against only those two; if rc=0,
step 0 takes those two files and no rule is ever dropped). Prefer (ii) when
it holds. Macro library files expand in list order, each by the files before
it (`1_caret.dl7` by `0_standard`, `2_list.dl7` by both).

## Sites

| site | today | after |
|---|---|---|
| `src/_8_driver/_1_unit.rs:55-63` `prelude_unit`, `macrotime_unit` | `text_unit` only, never expanded | unchanged builders; the expansion happens in the driver, not here |
| `src/_8_driver/_2_macro.rs:18-33` `standard_macro_program` | one compile against the raw prelude | takes the prelude unit as a parameter; called twice (L0, L1) |
| `src/lib.rs:346` `expanded_units`, `:142-168` `compile`, `:191` `compile_units` | prelude bypasses `macro_phase` | the stage trace; one `Event` per stage so the phase line shows L0, P1, L1 |
| `prelude/2_constructor_rules.dl7` | clause pairs building lists by hand | `[..]` in every pair; delete a pair only when every caller in `3_derived_rules.dl7:140-170` reads inline `intern_snapshot` and the oracle diff is explained; HistoryV1 keeps its extra goals |
| `macrotime/1_caret.dl7:114-120` `caret_mint` | three `cons` goals | `[?Invocation ?OutputOrdinal ?TemplatePath]`; `2_list.dl7` must be loaded before `1_caret.dl7` for that, so rename to make the order true (`1_list.dl7`, `2_caret.dl7`) and update `_8_driver/_0_read.rs:17-21` |
| `oracle/compile/`, `oracle/macrotime/` | | refreeze in its own commit, row-count delta stated (AGENTS "The prelude grows" row) |
| `book/src/8_macros.md` | protocol section | one paragraph: the three stages, through `mdbook-cmdrun` of the phase line |

NOT touched: `src/_2_lower/**`, `src/_3_check/**`, `src/_6_eval/**`,
`src/_4_comptime/**`, `src/_9_runtime/**`, `std/**`, `src/_0_read/**`.

## Steps

| # | step | receipt |
|---|---|---|
| 1 | on base: put `[?Source]` in one prelude rule, `dl8 compile` any fixture; paste the `undeclared_relation` diagnostic; revert | pasted |
| 2 | test (ii): compile the macro library against only `0_constructors` + `1_declarations`; paste rc and diagnostics | pasted |
| 3 | stage trace in the driver; phase line shows three stages; every oracle byte-identical (no prelude edit yet) | `cargo test --test _8_compile_oracle --test _2_macrotime_oracle` PASS, no refreeze |
| 4 | step 1's rule kept with `[?Source]`; compile rc=0; same rows as before | pasted |
| 5 | prelude pairs with `[..]`; refreeze own commit with delta | stat + delta |
| 6 | pairs deleted where callers allow; refreeze again if rows move | stat |
| 7 | `caret_mint` with the literal, macro file rename, macrotime oracle refreeze | stat |
| 8 | book paragraph; `cargo test --test _22_book` | PASS |
| 9 | full gate; `dl8 compile` wall time on `fixtures/openapi/todo.dl7` before and after, three runs each | numbers |

Commit after every step; subject names the step. Post the PR yourself.

## Gate

```bash
cargo test --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked"
grep -rn 'eprintln!' src/ | wc -l    # 0
git diff --stat origin/main...HEAD
```

Known red on the base: `.github/CI-KNOWN-RED.md` dl8 battery section (6
nested-worktree legs). `_22_book::chapter_executors` passes alone at ~14s;
measure alone three times before calling it red.

## Laws

`plans/v8/2026-09-17-import-arc.brief.md` section 8. No `eprintln!`. Comments
state only what the code cannot show. dl variables descriptive. PR body = the
steps table with receipts, under 20 lines of prose. Never `cargo fmt` outside
ownership.

## Report

```bash
boop beep --no-wait --as feat-prelude-macrotime sprefa-coordinator "prelude-macrotime: PR #<n>, compile oracle <pass>/<total> (+/-<rows>), battery <pass>/<total>, compile wall before/after <s>/<s>, red: <list or none>"
```
