# Brief: cheaper retraction between macrotime units (ir:sqlite)

User (2026-10-05): "yes". Page 2 merged (`f82720080`, sqlite_ivm `3f7eb15`), findings
`plans/v8/2026-10-05-one-macrotime-engine.findings.md`.

## Today

- One engine for all macrotime units. Unit k's seed rows are retracted (-1) inside unit k+1's
  first settle (`src/_1_macrotime/_4_expand.rs`).
- ir:sqlite unit 2: 131 ms, most of it retracting unit 1 (3928 seed rows, 5243 changes in the
  settle). Unit 4 first settle carries unit 3's 396 retractions. dd: retraction about the old 6 ms
  install, so dd wall did not move.
- ivm-sqlite retracts through DRed (over-delete, rederive) per SCC
  (`sqlite_ivm/crates/ivm-sqlite/src/3_nodes.rs`, `4_engine.rs`).

## Do

1. Findings first: where the retraction settle spends time on sqlite (statements by node kind,
   DRed over-delete vs rederive, rows touched, ms). Write the table into
   `plans/v8/2026-10-05-cheap-unit-retraction.findings.md` before changing code.
2. Pick the fix from the data. Candidate levers (pick none, one or more, justify by numbers):
   - Unit-scoped rows: tag each unit's derived rows with a unit key so dropping a unit is a bulk
     `DELETE ... WHERE unit = ?` of a disjoint partition, no DRed rederive. Only valid when units
     share no derived rows; prove it or measure it.
   - DRed fast path: when every deleted support of a fixpoint comes from one retracted frontier
     and nothing outside it reaches the same rows, skip rederive.
   - Batch the retraction statements (one set-based delete per node, no per-row work).
   - No retraction at all for units whose outputs are read only by key that includes the unit.
3. Measure before/after: unit 1-4 settle ms both engines, registry build time/RSS both engines
   (local `RCARGO_OFF=1`, 3+ interleaved runs, report `uptime` load), suites. Add "How it works".

## Laws

- Byte-identical: macrotime oracle (`every_macrotime_case_matches_v7`), registry `cmp`, both
  suites. No golden edits. dd must not regress.
- Own worktree; sqlite_ivm clone inside it from main `3f7eb15`, branch
  `perf/cheap-unit-retraction`; rebuild the extension dylib from the clone; fresh `TMPDIR` for
  `tests/extension.rs`.
- Another lane (`perf/plain-inner-one-epoch`) runs at the same time and edits
  `src/_6_eval/_9a_ir_lower.rs`; stay out of that file unless required, and say so in the receipt.
- `-j 4`; `--test-threads=1`; `uptime` before timing; all edits first, then test.
- No recursion in kernel Rust, no new kernel relation (stop and report), no silent fallbacks, no
  `json_*(`, no book edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
commits: <sha subject>
before/after: unit settle ms, registry time/RSS both engines, suites
validation: <command + result line each>
next: <one action>
```
