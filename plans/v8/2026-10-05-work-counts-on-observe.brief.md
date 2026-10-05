# Brief: move sqlite work counts onto hafley-observe; growth assertions

User (2026-10-05): "why is hafley-observe not being used". Go.

## Today

- `sqlite_ivm/crates/ivm-sqlite/src/0_work.rs` (519 lines) reads `sqlite3_stmt_status` itself and
  keeps its own counters; pins hand-written in `crates/ivm-sqlite/tests/13_work_counts.rs` and
  sprefa `tests/it/_9h_build.rs::tsi_registry_sqlite_work_counts`.
- `hafley-rs/crates/hafley-observe` already has: `5_sqlite.rs` `StatementCounters`,
  `instrument(connection)` (VM step, fullscan, sort, autoindex, reprepare, run, mem);
  `4_counts.rs` `CountRecorder`, `assert_instances`, `assert_children_at_most`,
  `observed_growth` / `assert_growth` (Constant / Log / Linear / Quadratic);
  `11_testkit.rs` `#[oh::test(time_ms, logs)]`, `counting_allocator!`.
  ivm-sqlite and ivm-dd already depend on it (`src/observe.rs`).

## Do

1. Read hafley-observe fully (README, `4_counts.rs`, `5_sqlite.rs`, `11_testkit.rs`). Map each
   `0_work.rs` count to an observe facility; list the counts observe lacks (CREATEs, schema rows
   parsed, unfolded bytes, rows written per table kind, zero-row executions).
2. Missing facilities go into hafley-observe (hafley-rs repo, generic, not ivm-specific), then
   ivm-sqlite reports through them (spans/events with fields, statement counters via
   `instrument`). Delete what `0_work.rs` duplicates. Keep the counts the user has already seen
   (same names where possible) and equal values.
3. Tests: keep the exact registry pins, and add `assert_growth` checks: for a family of
   generated programs of size n and 4n (rules, relations, nodes), install DDL work, schema rows
   parsed, prepared bytes, settle VM steps and rows written must be Linear or better; a
   Quadratic result fails. Use `#[oh::test]` budgets where they fit.
4. Findings `plans/v8/2026-10-05-work-counts-on-observe.findings.md`: mapping table, what moved
   into observe, growth results per count (sqlite and dd), counts equal before/after.

## Laws

- hafley-rs changes: in a hafley-rs worktree/branch `feat/observe-work-counts` (read the
  `hafley-rs-repo` skill first; other agents work in hafley-rs); sqlite_ivm reaches hafley-rs via
  its `hafley-rs` symlink: point the clone's symlink at your hafley-rs worktree.
- sprefa worktree from main (current head); sqlite_ivm clone from main `14675e8`, branch
  `feat/observe-work-counts`; extension dylib built in the clone before suites.
- Features in Rust only. No caching. No wall-clock timing.
- Parallel lanes: `feat/rule-interpreter`, `feat/macrotime-waves-engine`; report overlap.
- `-j 4`; `--test-threads=1`. Never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
hafley-rs branch + sha + worktree:
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
mapping: <count -> observe facility, new or existing>
growth: <count: class, sqlite and dd>
validation: <command + result line each>
next: <one action>
```
