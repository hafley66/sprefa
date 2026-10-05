# Brief: lab-20261005-sqlite-install — every way out of sqlite install cost

User (2026-10-05): "try all the ways out, i want sqlite locked down".

## Today

- ir:sqlite install = SQL text generation + one CREATE per `_d`/`_i`/`_x`/scratch/source object +
  statement prepare. dd installs the same IR in memory.
- Per process (atlas `plans/v8/2026-10-05-phase-atlas.md`): install #1 macro library 52.6 ms
  (dd 6.3), #2 macrotime 52.7 ms (dd 5.9), #3 comptime 80.2 ms (dd 8.6). 104-program census:
  21.0 s of sqlite install vs 2.3 s dd.
- Census of objects: `plans/v8/2026-10-03-sqlite-creates.findings.md` (2232 CREATEs for the c15
  corpus; avoidable kinds listed). Per-CREATE cost grew with schema size in
  `plans/v8/2026-10-02-ir-speed.findings.md` #2 (64 us at 1630 objects, 190 us at 4959): verify.

## Do

Harness first: one install benchmark over (a) `crates/ivm-sqlite/tests/corpus/8_c15_program.json`,
(b) the three IRs of the registry build (dump them), reporting install ms, CREATE count,
statements prepared, ms per CREATE vs schema size, and dd install for the same IR. Then one arm
per option, each its own commit on the lab branch, each measured on the same harness:

| arm | change |
|---|---|
| A0 | baseline; profile install (samply or Instruments): split SQL generation, DDL execution, prepare, catalog |
| A1 | connection settings during install: in-memory vs file, `journal_mode`, `synchronous`, `temp_store`, one transaction, `PRAGMA schema_version` effects, `defer_foreign_keys`; check whether DDL cost per object is O(schema) and why (SQLite source / `sqlite3_stmt_status`) |
| A2 | fewer objects: every avoidable kind from the creates census (Mint `_d` read twice, SCC copies of inputs that already have an image, seed tables for keys with no seeds, threshold-input images, ...) |
| A3 | generic node-keyed tables: shared `_d`/`_i` tables with a `node` column (cells as term ids or fixed max arity); install = INSERTs, zero per-program DDL; measure settle cost change too |
| A4 | installed-image cache: build once, `sqlite3_serialize` the database (rusqlite `serialize` feature), key by IR structural fingerprint + sqlite_ivm version; next process `sqlite3_deserialize`; measure load ms; check macro library (#1) and macrotime (#2) IRs are identical across processes |
| A5 | lazy objects: create a node's tables on first non-empty delta instead of at install |
| A6 | prepared-statement reuse: prepare cost at install and first settle; statement cache size; `prepare_cached` misses |

Arms may combine; report each alone and the best combination. Settle cost must not regress
(run the 104-program census, script in `plans/v8/2026-10-05-sqlite-comptime-settle.brief.md`).

Write `plans/v8/2026-10-05-sqlite-install-lab.findings.md`: table arm x (install ms per corpus,
CREATEs, settle delta, suite wall), a recommendation of which arms to merge, and "How it works"
for each kept arm.

## Laws

- Lab dir/branch naming: `lab-20261005-sqlite-install` (branch `lab/sqlite-install`).
- Own worktree from sprefa main; sqlite_ivm clone inside it from main `1074daa`, branch
  `lab/sqlite-install`; build the extension dylib in the clone before suites.
- Kept arms: byte-identical registry `cmp`, macrotime oracle, both suites, ivm-sqlite tests.
- Parallel lanes: `perf/sqlite-comptime-settle` (ivm-sqlite settle path, closure passes) and
  `perf/macrotime-rewrite-rules`. Stay out of settle/closure-pass code; install/catalog/DDL only.
  Report any overlap.
- Library before bespoke: A3 and A4 start with a candidate table if a crate could do it.
- `-j 4`; `--test-threads=1`; `uptime` before timing; 3+ interleaved runs per number.
- No recursion in kernel Rust, no silent fallbacks (A4 cache miss logs), no `json_*(`, no book
  edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
arms: <arm: install ms before -> after per corpus, CREATEs, settle delta, kept?>
recommended merge set:
validation: <command + result line each>
next: <one action>
```
