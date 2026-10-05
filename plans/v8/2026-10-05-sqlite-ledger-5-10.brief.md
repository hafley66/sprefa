# Brief: ir:sqlite ledger rows 5-10 (settle-side costs)

User (2026-10-05): sqlite locked down; work the closed ledger.
Ledger: `plans/v8/2026-10-05-sqlite-audit.findings.md` (measured on sqlite_ivm `1074daa`).
Since then merged: CTE expansion cap + stored multi-read unions (`a9e4b37`), non-monotone LetRec
(`d4203cb`). Install-side rows (1-4) belong to `lab/sqlite-install`, running in parallel.

## Rows

| # | item | sqlite s (census) |
|---|---|---|
| 5 | DRed loop statements, first prepare (155,682 statements) | 4.1 |
| 6 | CTE fill execution, 30,676 of 39,091 executions return 0 rows | 3.4 |
| 7 | term interning through SQL: 3 statements per term; texts re-interned on every call | 3.2 (dd 0.7) |
| 8 | DRed loop execution | 2.1 |
| 9 | per-row and per-settle work outside the plan (stage, host-table writes, discarded delta round trip, per-row String clones) | 2.0 |
| 10 | integrate statements of inline nodes recomputing their CTE chain | 1.7 |

## Do

1. Re-measure rows 5-10 on current main (census script in
   `plans/v8/2026-10-05-sqlite-comptime-settle.brief.md`, plus the ledger's counters) and write the
   updated table first into `plans/v8/2026-10-05-sqlite-ledger-5-10.findings.md`.
2. Fix each row still above 0.5 s, largest first, one commit each, measured.
   Row 6: gate fills on non-empty input deltas. Row 7: intern in Rust with the existing term map,
   one batched statement per settle. Row 9: drop the discarded round trip, borrow instead of clone.
3. Before/after per row; census per phase both engines; suite wall both engines; registry build
   time/RSS (3+ interleaved runs, `uptime`). "How it works" per fix.

## Laws

- Byte-identical: registry `cmp`, macrotime oracle, both suites. No golden edits.
- Own worktree from sprefa main `874f6c93c`; sqlite_ivm clone inside it from main `d4203cb`,
  branch `perf/sqlite-ledger-5-10`; build the extension dylib in the clone before suites.
- Parallel lane `lab/sqlite-install` owns install/catalog/DDL/SQL generation; stay out; report overlap.
- `-j 4`; `--test-threads=1`; all edits first, then test.
- No recursion in kernel Rust, no silent fallbacks, no `json_*(`, no book edits, never
  `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
rows: <row: s before -> after, commit>
census/suite/registry before -> after:
validation: <command + result line each>
next: <one action>
```
