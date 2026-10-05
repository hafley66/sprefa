# Brief: ir:sqlite cost audit — every place sqlite is slower than dd

User (2026-10-05): "i want sqlite locked down, i dont want to keep finding new things about it that
are obvious". Read-only audit plus measurement; no fixes.

## Do

Produce one exhaustive ledger of ir:sqlite cost vs ir:dd for the same IR, so that later lanes
work down a closed list.

1. Inventory every runtime path in `sqlite_ivm/crates/ivm-sqlite` (`4_engine.rs`, `3_nodes.rs`,
   `2_terms.rs`, `catalog.rs`, `engine.rs`) and the sprefa side that drives it (`_9b_ir_eval.rs`
   sessions, snapshot, decode, intern): for each, what it does per install, per settle, per row.
2. Measure on: the registry build, the 104-program census (script in
   `plans/v8/2026-10-05-sqlite-comptime-settle.brief.md`), the c15 corpus. Profile with samply or
   Instruments (stacks, not guesses). Count: statements executed by kind, statements prepared,
   rows written per table kind, rows read that were empty, passes/rounds, DDL, pragmas,
   transactions/savepoints, json/text encoding, allocation in decode.
3. Ledger file `plans/v8/2026-10-05-sqlite-audit.findings.md`: one row per cost item:
   item | where (path:line) | sqlite ms | dd equivalent ms | per-install/settle/row | cause |
   candidate fix | owned by (install lab / comptime-settle lane / new). Sorted by ms. Close with
   the sum of rows vs measured total (unexplained remainder stated).
4. Cross-check against prior findings (`plans/v8/2026-10-0*-*.findings.md`) and mark which items
   are already fixed, in flight, or new.

## Laws

- Read-only on source; temporary instrumentation in a scratch worktree only, removed after.
- Own worktree from sprefa main; sqlite_ivm clone inside it from main `1074daa`.
- Parallel lanes (`lab/sqlite-install`, `perf/sqlite-comptime-settle`, `perf/macrotime-rewrite-rules`)
  are changing these files; audit main as of `1074daa`.
- `-j 4`; `uptime` before timing.
- Commit only the findings file on branch `audit/sqlite`. No merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
branch + sha:
ledger: <top 10 rows: item, sqlite ms, dd ms, owner>
explained / measured total:
next: <one action>
```
