# Brief: sqlite work counts — prove the broken counts are gone

User (2026-10-05): "stop timing things and time the problem then tell me the root inner count of
the obviously broken thing in counting is gone and we are literally doing less work".

## Do

1. A permanent, deterministic work counter in ivm-sqlite (behind a cargo feature or an
   always-on cheap counter struct exposed through `Engine` counters; no wall clock), reporting
   per install and per settle:
   - CREATEs executed; sqlite_schema rows parsed by DDL (sum over CREATEs of the target
     schema's object count at that moment, per attached shard);
   - statements prepared; SQL text bytes prepared; unfolded bytes (SQL text with every CTE
     reference replaced by its body, recursively — the expansion SQLite does);
   - statement executions; executions that return or write 0 rows;
   - VM steps: sum of `sqlite3_stmt_status(SQLITE_STMTSTATUS_VM_STEP)` over every statement
     (including DDL and `sqlite3_exec` paths), plus FULLSCAN_STEP, SORT, AUTOINDEX, REPREPARE;
   - rows written per table kind (`_d`, `_i`, `_x`, scratch, source, catalog).
   Same counters on dd where they have a meaning (operators built, records processed).
2. Run the counter on the registry build and the 104-program census at four sqlite_ivm commits:
   `1074daa` (before today's sqlite work), `a9e4b37` (CTE expansion cap + stored unions),
   `4710596` (sharded schemas + image cache), and current main. Backport the counter as a patch
   onto each older commit in scratch (do not commit to old branches).
3. Findings `plans/v8/2026-10-05-sqlite-work-counts.findings.md`: one table per program
   (registry, census sum) with counts per commit, and the ratio sqlite/dd where comparable.
   State for each fix which count it reduced and by how much; flag any count that went up.
   No ms columns.
4. Commit the counter (step 1) on current main's tree as its own commit, with a test that pins
   the registry counts so a regression in work shows up as a count change.

## Laws

- Own worktree from sprefa main `64f07f278`; sqlite_ivm clone inside it from main `4710596`,
  branch `feat/sqlite-work-counts`; build the extension dylib in the clone before suites.
- Parallel lane `perf/sqlite-ledger-5-10` edits settle-side ivm-sqlite code; keep counter hooks
  minimal and report overlap.
- `-j 4`; `--test-threads=1`. No timing passes.
- Byte-identical outputs; counters must not change behavior.
- No silent fallbacks, no `json_*(`, no book edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
registry counts per commit: <table: count | 1074daa | a9e4b37 | 4710596 | main | dd>
census counts per commit: <same>
counts that went up: <list or none>
validation: <command + result line each>
next: <one action>
```
