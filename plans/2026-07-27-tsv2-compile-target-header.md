# TSV2 COMPILE-TARGET ARC HEADER (planner contract; user reorientation 2026-07-27 night)

User directive, restated and confirmed: NEW EFFORT. Prolog owns the entire
compiler front (parse, AST, typecheck, lowering — "all the AST syntax
bullshit"). It emits LITERAL TypeScript files per program: the real SQLite
statements and the real rxjs chains visible in a generated .ts file you can
open and read. TypeScript keeps exactly two layers: a hand-written STATIC
runtime (reused from v6 wherever possible) and the GENERATED program files.
No AST, no parser, no lowering in TypeScript. Existing v6/dl stays untouched
and running; this is a sibling target, "v6 TS v2".

## The split (what is static, what is generated)

| layer | lives where | contains |
|---|---|---|
| compiler | v6/prolog/compile/ (new) | term-form program in, .ts file out |
| oracle log | v6/prolog/conformance/ticklog.pl (new, loads engine.pl, NEVER edits it) | fixture name in, tick-log JSONL out |
| static runtime | v6/tsv2/runtime/ (new package) | tick loop skeleton, driver seam, host exec, tracing hookup — generic algorithm, zero per-program text |
| generated programs | v6/tsv2/gen/*.ts | per-program DDL strings, per-rule delta SQL, stratum order, host table, rx wiring — all visible |

## Reuse law (failure-modes class 34: name the exact symbols, prose does not bind)

The static runtime imports these EXISTING symbols; rebuilding any of them is
the class-34 defect:
- `SqlRunner: ISqlRunner` (v6/sprefa-store/js/src/engine/sqlRunner.ts) — the
  single driver seam; generated code never touches @libsql directly.
- `spine.ts`: `create_all_tables`, `table_names`, `content_hash`,
  `byte_to_linecol`, `rel_col_int` and the rel helpers — the fact plane.
- The store's IVM machinery (engine.ts cascade/reconcile/support tracking) for
  whatever the tick algebra needs that is program-independent.
- HostRunner's effect execution + content addressing + effect_cache
  (v6/dl/src/1_hosts.ts) — lift/adapt, do not rewrite; if lifting requires
  changes in v6/dl, STOP and report (that file is shared).
- The P0 tracing spine (0_trace.ts, landing from a concurrent agent): the
  generated programs emit through the SAME channels; tsv2 gets perf lines for
  free.
- `measure.ts` memcap for RSS.
MECHANICAL GATE: a check script asserts every gen/*.ts file imports ONLY from
`../runtime/` and `rxjs`, and that runtime/ imports the named store symbols
rather than declaring parallel tables (grep-countable, runs in CI with the
suite).

What this path does NOT use (left alive for v6/dl, dead weight here): langium
grammar + 0_generated, 0_ast_bridge.ts, lower/{ast,lower,lowerSql}.ts as
executable code. lowerSql.ts's SQL shapes are the REFERENCE for what prolog
must emit — read it, copy its statement patterns, never call it.

## The grading loop (this is stopping-point item 9 landing, the marble record)

One JSONL envelope, emitted by BOTH sides, diffed byte-for-byte:
`{"tick": N, "deltas": {"relName": {"add": [rows...], "del": [rows...]}}}`
with rel names sorted, rows sorted by their canonical column order, numbers
as JSON numbers, atoms as strings. The perf line from P0 nests later as a
sibling `"perf"` key (per the perf header's SLOT-ENVELOPE promise; that slot
is DECIDED here). The oracle side prints it from the fixture's actual
run_program deltas; the tsv2 side prints it from the store's boundary diffs.
A fixture PASSES when the two logs are identical. The 109-fixture corpus is
the compiler's ready-made test suite.

## Phases

- **A — hand-carve the target** (dispatchable now): pick fixtures
  `demand_laziness_effect_rows` and `switch_as_keyed_replace`
  (conformance/fixtures/scopes.pl:344 and :31 — small, and their expected
  per-tick deltas are already written). By HAND write gen/<name>.ts exactly as
  we want prolog to emit it, plus the minimal runtime/ package that runs it
  (tick loop over the fixture's arrival schedule, SqlRunner seam, log
  emitter), plus ticklog.pl on the oracle side. DONE = both logs identical
  for both fixtures, import gate green. The hand-written file IS the emitter
  spec.
- **B — the emitter**: v6/prolog/compile/*.pl emits phase A's two files
  byte-identically (byte-diff is the grade; formatting is part of the spec).
- **C — widen**: sweep the fixture corpus, count identical-log fixtures,
  findings (not workarounds) for constructs that do not compile yet.
- **D — surface + world**: .dl text parsing as a prolog DCG; host effects
  (ghcacher rides here, with F7's crash bypassed since the generated path
  writes through its own committed SQL); real programs from the stopping
  point.

## Named slots

- SLOT-TERMFORM: phase A-C consume the fixture term form (prog(Decls, Rules))
  as the compiler input; the .dl surface parser is phase D, not before.
- SLOT-GENCHECKIN: are gen/*.ts files committed (readable in review, drift
  = regenerate + diff) or build artifacts? Start committed; user taste later.
- SLOT-RXSHAPE: the generated rx chain shape is fixed by the phase-A exemplar;
  any deviation the emitter wants is a finding first. (User's own directive
  puts real rxjs in generated files, which satisfies standing-plan item 4 for
  the generated shape; NEW hand-written runtime rx still gets the ask-first
  check.)

## Sequencing with running work

P0 tracing agent (v6/dl + sqlRunner seams) merges independently; tsv2 consumes
its channels afterward. The F7 crash hunt stays queued on v6/dl. Phase A
touches only new directories + one new prolog file, so it runs concurrently
with anything.
