# TSV2 Phase C scoreboard

Sweep of `v6/prolog/compile/` over every fixture in
`v6/prolog/conformance/fixtures/*.pl`, per
`plans/2026-07-27-tsv2-compile-target-header.md`'s PHASE C CONTRACT. Driver:
`v6/tsv2/scripts/sweep.sh` (compile every fixture -> `v6/prolog/compile/out/`,
run the oracle over the same fixtures, run every compiled program on the
phase-A runtime, diff tick logs byte-for-byte).

Regenerate: `cd v6/tsv2 && bash scripts/sweep.sh`. Raw data:
`v6/prolog/compile/out/manifest.json` (compile bucket + refusal reason per
fixture) and `v6/prolog/compile/out/run-results.json` (run bucket + diff
excerpt per compiled fixture).

## Totals (this commit: harness + 3 lowering-crash fixes)

| bucket | count |
|---|---|
| fixtures swept | 109 |
| UNSUPPORTED (compiler refuses, named construct) | 89 |
| compiled (lowering + emission succeeded) | 20 |
| — of which tick log byte-identical to oracle | 12 |
| — of which WRONG (diff or crash vs oracle) | 8 |

12 + 8 + 89 = 109.

This is the FIRST state at which the sweep processes all 109 fixtures without
silently dropping any (the very first `sweep.pl` run, before any
`analyze.pl`/`lower.pl`/`emit_ts.pl` fix, showed `total=105` — 4 fixtures
vanished as bare Prolog goal failures rather than reaching either bucket; see
"History" below). A further commit narrows the gate (comparison operators,
`:=`/`is` binds, arithmetic in a rule head) to turn 3 of the 12 "identical"
results — which are false positives, not genuine passes — into clean
refusals; this commit's numbers are the honest pre-that-fix baseline, kept
here so the git history shows the scoreboard actually moving.

## History so far

1. **Baseline**: `sweep.pl` written, zero changes elsewhere.
   `SWEEP total=105 compiled=16` — 4 fixtures silently vanished
   (`sweep_one/5` failed as a bare goal rather than throwing). Root cause:
   `emit_ts.pl:recompute_levels_fn_lines/2` had no clause for
   `LevelStatements == []` (a program with zero level rules, e.g. an
   EDB-only fixture with an empty `Rules` list), so `emit_program/5` failed
   with no error term at all.
2. **This commit**: three lowering gaps the sweep surfaced immediately,
   none a new construct:
   - `emit_ts.pl`: added the `LevelStatements == []` fallback (emits
     `of(undefined)`) and the matching `DeltaStatements == []` fallback for
     `readSnapshot` (`forkJoin({})` completes without emitting, same hazard
     the edge resolver's own `forkJoin([])` guard already documents).
   - `analyze.pl`: added `declared_refs/2` — a `kind(Ref, _)` or
     `keyed(Ref, _)`/`keep(Ref, _)` declaration with **zero rule readers**
     (`engine_core.pl`'s `retention_count_prunes_oldest`: `kind(event/1,
     log), keep(event/1, count(2))`, no rules at all; `scopes.pl`'s
     `zombie_scope_negative_case_a2b`: `keyed(open_pane/2, [1])`, no
     `kind/2` either, deliberately unread — comment: "REJECTED READING
     dropped on purpose") was invisible to `program_refs/2` (which only
     walks `Rules`), so the rel got no DDL and no arrival handling at all.
   - `lower.pl`/`emit_ts.pl`: **multiple level-rule clauses sharing one
     head ref** (`shell_stream.pl`'s `terminal_is_terminal`:
     `stream_status(Args, running) <- ...` and
     `stream_status(Args, done) <- ...`, standard datalog union-of-clauses)
     were each lowered to their own `DELETE FROM ...; INSERT ...` pair, so
     the second clause's `DELETE` silently wiped the first clause's
     just-inserted rows. `level_statement_group/3` now groups adjacent
     same-head rules into one `DELETE` + N `INSERT`s (`levelstmt/3`'s third
     field changed shape from one SQL string to a list; `emit_ts.pl`,
     `test/plunit_tests.pl`, `test/run_sql_check.pl` updated to match — the
     two phase A/B exemplar fixtures, one clause per head each, still emit
     byte-identical text, re-diffed against `gen_emitted/*.ts` to confirm).
   - `emit_ts.pl`: a `bindArgs` helper wraps every raw arrival/edge-
     projection value before it becomes `SqlStatement` args. Root cause
     (verified with a throwaway `open_db` call, not assumed):
     `@libsql/client` binds a JS `number` parameter as SQLite REAL, never
     INTEGER — a bound `1` lands as the TEXT value `"1.0"` in a
     TEXT-affinity column (every column here is `TEXT NOT NULL`), `1n`
     (bigint) lands as `"1"`. `bindArgs` converts any
     `Number.isInteger(value)` argument to `BigInt(value)` first.
   - Combined result at this commit: `SWEEP total=109 compiled=20`, and
     `RUN total=20 identical=12 wrong=5 run_error=2 no_oracle_log=1`.

Full per-fixture detail, the per-construct blocked tally, and the Findings
section land in the NEXT commit's SCOREBOARD.md revision, once the gate
narrows further and the numbers stop moving for this pass.
