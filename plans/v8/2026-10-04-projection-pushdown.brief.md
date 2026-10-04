# Brief: projection pushdown in IR lowering (and narrow DD join outputs)

User (2026-10-04): approved after the walkthrough ("term graph analysis ... we have _ too").

## Today

- `src/_6_eval/_9a_ir_lower.rs` (~`:1366`): each positive goal = `Get(rel)` + a two-input `Join`
  onto the left-deep chain; one head `Mfp` projects at the end. Every intermediate row carries all
  columns of every relation joined so far.
- `sqlite_ivm/crates/ivm-dd/src/2_dd.rs`: join arrangements are `ValSpine<Row, Row>` = key columns
  + the whole row (`:29`, `keyed` `:213`); `join` concatenates both full rows (`:411-416`).
- `?_` and singleton variables ride along until the head (each `?_` is its own variable,
  `syntax_variable(n23, variable(n23, _), _)` in `oracle/macrotime/0_minimal_1.json`).

## Do

1. Needed-variable pass per rule, right to left: needed = head vars; before goal i = needed after
   goal i plus the variables goal i shares with earlier goals (join keys) plus vars used by
   filters/negations/aggregates later. Emit `Mfp { project }` below each join input so only
   needed columns are arranged; intermediate joins keep only vars still needed later. Singletons
   and `?_` are dropped at the source. Negation/antijoin keys and aggregate group/sum columns count
   as needed.
2. ivm-dd join: emit only the projected output columns instead of `a ++ b` when the IR says which
   columns survive (no extra Mfp node needed after the join). ivm-sqlite gets the same narrower
   column lists through the IR; check its SQL renders fewer columns.
3. Measure before (write `plans/v8/2026-10-04-projection-pushdown.findings.md` first): c15 install
   arrangements (count) and arranged cells, registry build time + peak RSS on both engines, suite
   times. Then after. Add a "How it works" section (problem with numbers, step trace on one real
   rule from the prelude or std, file:line, before/after).

## Laws

- Rows byte-identical: full suite both engines, registry `cmp`, macrotime oracle; no golden edits
  (refreeze only through `oracle/refreeze.py`, expect none).
- One cargo process at a time; `uptime` first; wait if load > 12; `-j 4`; `--test-threads=1`.
  All edits first, `cargo check` while editing, then test once, re-run failures by name, final
  full run per engine; sqlite_ivm crate tests too.
- sqlite_ivm: clone inside the sprefa worktree (`git clone /Users/chrishafley/projects/sqlite_ivm
  .boop-worktrees/sqlite_ivm`, main `b791f71`), branch `perf/projection-pushdown`, symlink
  `sqlite_ivm` at it, rebuild the extension dylib from the clone (`RCARGO_OFF=1
  scripts/0_build.sh release`), fresh `TMPDIR` for `tests/extension.rs`.
- No recursion (`_9j_no_recursion`), no new kernel relation (stop and report), no silent
  fallbacks, no `json_*(`, no book edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
commits: <sha subject>
before/after: c15 arrangements + arranged cells; registry build time/RSS both engines; suite times
validation: <command + result line each>
next: <one action>
```
