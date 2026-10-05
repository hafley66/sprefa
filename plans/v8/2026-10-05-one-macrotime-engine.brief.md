# Brief: one engine for all macrotime units (page 2)

User (2026-10-05): "immediately get page 2 started". Page 1 (deltas per wave) is merged
(`df04fa652`, findings `plans/v8/2026-10-04-macrotime-deltas.findings.md`).

## Today

- `src/_1_macrotime/_4_expand.rs`: one `IrSession` per `expand` call, so one install per unit
  (5 per `registry.rs` build, plus 1 comptime). Per unit, install + first settle is nearly all of
  macrotime time (sqlite 160-330 ms, dd 20-55 ms).
- Why units cannot share today: the IR depends on the data. `ir_lower::lower` collects terms
  reachable from seeds (`src/_6_eval/_9a_ir_lower.rs` ~`:2141`) and derives constructor
  relations from term shapes found there (`lowered.constructors`, shape keys like
  `9:2:reader_node`); `install_terms` fails with `Unsupported("dynamic term constructor")`
  (`_9b_ir_eval.rs` ~`:637`) for a shape the lowering never saw.
- Measured on ir:sqlite: 7919 CREATEs over 6 installs, install 0.75 s of 2.55 s; source tables
  created one by one outside the program savepoint, each with an EXISTS query and an uncached
  `PRAGMA table_info` (`sqlite_ivm/crates/ivm-sqlite/src/4_engine.rs:237-256`).

## Do

1. Make the macrotime IR independent of seed data: constructor relations come from the program
   (rules, declarations) plus a fixed set for reader terms, or one generic constructor relation
   keyed by (functor, arity); seed terms enter as dictionary rows through `apply`, never through
   lowering. The prepared IR fingerprint must be equal for every unit of one build.
2. One `IrSession` per process for macrotime: prepare once, then each unit is a frontier
   (+ its syntax rows), its waves as page 1 does, and its rows retracted (-1) when the unit is
   done if units must not see each other (decide from how units are expanded today; keep output
   byte-identical).
3. ivm-sqlite: create source tables inside the program install savepoint; no per-table EXISTS /
   PRAGMA round trips (one sqlite_master read).
4. Measure before (findings file first) and after: installs per build, CREATEs, macrotime wall,
   registry build time/RSS on both engines (local `RCARGO_OFF=1` builds, report load), suites.
   Add a "How it works" section.

## Laws

- Byte-identical: macrotime oracle (`every_macrotime_case_matches_v7`), registry `cmp`, both
  suites. No golden edits.
- Own worktree; sqlite_ivm clone inside it from main `e5afc8f`, branch `perf/one-macrotime-engine`;
  rebuild the extension dylib from the clone; fresh `TMPDIR` for `tests/extension.rs`.
- One cargo process at a time; `uptime`; `-j 4`; `--test-threads=1`; all edits first, then test.
- No recursion, no new kernel relation (stop and report), no silent fallbacks (the page-1
  `macrotime reprepare` path must stay logged; add a test that exercises it), no `json_*(`, no book
  edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
commits: <sha subject>
before/after: installs, CREATEs, macrotime wall, registry time/RSS both engines, suites
validation: <command + result line each>
next: <one action>
```
