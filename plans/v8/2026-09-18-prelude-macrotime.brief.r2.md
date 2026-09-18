# prelude-macrotime, round 2: finish steps 5-9

Continues `plans/v8/2026-09-18-prelude-macrotime.brief.md` (read it whole first;
its Goal, stage trace, Sites, Gate, Laws, Report all apply). The first lane
(sonnet) died after step 4 with step 5 uncommitted; the coordinator committed
that work as `a3ebebf8b`.

## First action

```bash
git merge --ff-only 661792457            # origin/main; STOP AND REPORT on failure
git cherry-pick 7c48ea560 64865a98d a3ebebf8b   # steps 3, 4, 5-wip from branch feat/prelude-macrotime
cargo build 2>&1 | tail -3
```

If a cherry-pick conflicts, STOP AND REPORT the conflicting file. Do not resolve.

## Steps left

| # | step | receipt |
|---|---|---|
| 5 | `dl8 compile fixtures/openapi/todo.dl7` with the `[..]` pairs; rc=0; row count equals the pre-step-5 count (`git stash`-free: compare against `64865a98d` by checking it out in a second worktree, never by stash). Refreeze the oracles in their OWN commit with the delta pasted in the subject | rc, counts, stat |
| 6 | pairs deleted where every caller allows; refreeze again only if rows move | stat |
| 7 | `caret_mint` (`macrotime/1_caret.dl7`) with the list literal; rename macro files so list loads before caret; macrotime oracle refreeze | stat |
| 8 | one book paragraph on the staged bootstrap; `cargo test --test _22_book` | PASS |
| 9 | full gate; `dl8 compile` wall time on `fixtures/openapi/todo.dl7` before and after, three runs each | numbers |

Commit after every step; subject names the step. Post the PR yourself with the
steps table and receipts. Every operation over 10s runs in background with a
cap; never foreground-wait.

## Ownership

Owned: `prelude/`, `macrotime/`, `src/_8_driver/`, `src/lib.rs`,
`src/_1_macrotime/_6_cli.rs`, `oracle/`, `tests/_2_macrotime_oracle.rs`,
`tests/_8_compile_oracle.rs`, `tests/_9_tracing.rs`, `book/src/`.
Forbidden: `src/_2_lower/`, `src/_3_check/`, `src/_6_eval/`, `src/_9_runtime/`,
`std/`, `AGENTS.md`, `CLAUDE.md`, `Cargo.lock`.
