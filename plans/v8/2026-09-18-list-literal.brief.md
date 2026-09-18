# List literal brief

Lane branch `feat/list-literal`. Base `origin/main` 4fd74cebc1ce9456fee182e0fe2c5681d3cbccd8 (PR #793 merged).
First action `git merge --ff-only 4fd74cebc1ce9456fee182e0fe2c5681d3cbccd8`; failure = stop and report. Preset sonnet.

Decision row, read first (`AGENTS.md`, grep "list literal"): `[a b]` is a
bracket form the reader emits, rewritten at macrotime to
`(cons a (cons b (nil)))`. Kernel unchanged. Probe with the motivating use:
`plans/v8/probes/2026-09-18-generic-result.dl7` (compiles on base with
`nil`/`cons` spelled out; its header pastes the rows).

## Goal

`[?Ok ?Err]` in any argument position means the kernel list `(cons ?Ok (cons ?Err (nil)))`.
The reader gives it a form; one macro rewrites it; lower sees only `cons`/`nil`.
The prelude constructor clause pairs that exist only to build a list from
`nil`/`cons` shrink to inline goals.

## Before / after

Source: `(<- (: ?N ok ?Ok 0) (intern_snapshot Result [?Ok ?Err] ?N))`

`dl8 read`: `(<- (: ?N ok ?Ok 0) (intern_snapshot Result (list ?Ok ?Err) ?N))`
(the bracket form reads as a form whose head is the atom `list`, the way `a.b`
reads as `(. a b)`).

`dl8 expand`: `(<- (: ?N ok ?Ok 0) (intern_snapshot Result (cons ?Ok (cons ?Err (nil))) ?N))`

`[]` reads `(list)`, expands `(nil)`. `[a [b c]]` nests. `[a.b ^c]` composes
with the dot and caret arms, each item read by `read_term`.

## Sites

| site | today | after |
|---|---|---|
| `src/_0_read/_1_tokens.rs` | `[` `]` are atom characters (`:60` mentions only a closing bracket ending an atom) | `[` opens a form, `]` closes it; an unbalanced one is a reader diagnostic like `unterminated_form` |
| `src/_0_read/_2_reader.rs:258` `read_form` | `(` only | sibling `read_list`: items via `read_term`, payload the form `(list <items>)`, the atom `list` synthesized at the bracket's own position (the `read_path` `:426` and `read_caret` `:444` precedents) |
| `macrotime/2_list.dl7` | absent | claims any form whose head atom is `list`; mints `(cons Item Rest)` right-nested, `(nil)` at the end; the `caret_mint` / `GeneratedSyntax` pattern from `1_caret.dl7:114-145`; TypeScript-shaped signature comment like `0_standard.dl7:4-16` |
| `src/_8_driver/_0_read.rs:17-21` `MACROTIME` | two files | three |
| `prelude/2_constructor_rules.dl7` | clause pairs for Partial, Option, Key, Pick, Exclude, HistoryV1, each building the argument list by hand | each pair becomes what it is: the build clause is `(intern Ctor [..] ?Result)`, the read clause `(intern_snapshot Ctor [..] ?Result)`. Delete a pair outright only when every caller in `3_derived_rules.dl7:140-170` reads through `intern_snapshot` inline and the oracle diff is explained; HistoryV1 keeps its `default`/`Conforms` goals |
| `macrotime/1_caret.dl7:114-120` `caret_mint` | three `cons` goals | `[?Invocation ?OutputOrdinal ?TemplatePath]`, once the macro file order lets `2_list.dl7` run first; if ordering is a problem, leave it and report the site |
| `oracle/macrotime/`, `oracle/compile/` | | new expansion cases; prelude change refreezes compile goldens in its own commit with the row-count delta stated (AGENTS "The prelude grows" row) |
| `book/src/5_terms.md:31-32` | "no list literal" | the literal, one before/after through `mdbook-cmdrun` of `dl8 expand` |

NOT touched: `src/_2_lower/**`, `src/_3_check/**`, `src/_6_eval/**`,
`src/_4_comptime/**`, `src/_9_runtime/**`, `std/**`. A value-position
`cons` inside an argument must already lower to goals with fresh variables
(the value-position call law). If it does not, that is a stop-and-report row
with the lower site cited, never a lower edit.

## Steps

| # | step | receipt |
|---|---|---|
| 1 | on base: `dl8 read` on a file containing `[a b]`; paste what the reader does today | pasted |
| 2 | tokenizer + `read_list`; `dl8 read` shows `(list a b)`, `(list)`, `(list a (list b c))`, `(list (. a b) (^ c))` | pasted forms |
| 3 | `2_list.dl7`; `dl8 expand` shows the cons chains for the four | pasted |
| 4 | probe: copy `2026-09-18-generic-result.dl7` to `2026-09-18-generic-result-literal.dl7` with `[..]` in every rule; `dl8 compile` rc=0, same four rows as the original's header | pasted rows, diff empty |
| 5 | prelude pairs rewritten with `[..]`; `cargo test --test _8_compile_oracle` diff explained; refreeze in its own commit with the delta | stat + delta |
| 6 | pairs deleted where callers allow; oracle refrozen again if rows move | stat |
| 7 | `caret_mint` with the literal, or the ordering stop row | pasted expansion |
| 8 | macrotime oracle cases + refreeze | stat |
| 9 | book section; `mdbook build book`; `cargo test --test _22_book` | PASS |
| 10 | full gate | summary |

Commit after every step; subject names the step. Post the PR yourself
(`gh pr create`); a lane that retires without a PR is graded incomplete.

## Gate

```bash
cargo test --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked"
cargo test --test _1_read_oracle --test _2_macrotime_oracle --test _8_compile_oracle --test _22_book 2>&1 | grep -E "^test |test result"
grep -rn 'eprintln!' src/ | wc -l    # 0
git diff --stat origin/main...HEAD
```

Known red on the base: `.github/CI-KNOWN-RED.md` dl8 battery section (6
nested-worktree legs). `_22_book::chapter_executors` sits at the 10s line;
measure alone three times before calling it red.

## Laws

`plans/v8/2026-09-17-import-arc.brief.md` section 8. No `eprintln!`,
`tracing` only. Comments state only what the code cannot show. dl variables
descriptive. No long-form markdown; PR body = the steps table with receipts,
under 20 lines of prose. Never run `cargo fmt` over files outside ownership;
`git diff origin/main...HEAD -- <forbidden paths>` must be empty before the PR.

## Report

```bash
boop beep --no-wait --as feat-list-literal sprefa-coordinator "list literal: PR #<n>, macrotime oracle <pass>/<total>, compile oracle <pass>/<total> (+/-<rows>), battery <pass>/<total>, red: <list or none>"
```
