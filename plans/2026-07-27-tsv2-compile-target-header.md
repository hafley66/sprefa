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

## ADDENDUM (user, same night: "get the prolog compiler compilering") — A and B run CONCURRENTLY

Phase B starts now, not after A. To keep the two agents' shapes compatible the
coordinator pins the seam both code against:

```ts
// runtime/types.ts — the generated-program seam (coordinator-pinned; extend by
// adding fields, never by renaming these)
interface IGenProgram {
  name: string;
  ddl: string[];                          // CREATE TABLE ..., run once at boot
  relColumns: Record<string, string[]>;   // declared column order, drives log rows
  arrivalTargets: string[];               // EDB rels a schedule may write
  tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas>;
  // tick's BODY lives in the generated file: visible SQL strings + visible rx.
}
```

- Phase A's runtime folds a schedule over IGenProgram and emits the envelope;
  its hand-written gen/*.ts conform to this seam.
- Phase B (v6/prolog/compile/) emits modules conforming to the same seam into
  its OWN draft dir (compile/out/), never phase A's gen/.
- Grade relaxed for B v0: oracle-LOG identity when its output runs on phase A's
  runtime at reconciliation (coordinator merges both, runs the cross product).
  BYTE-identity between emitter output and the hand exemplar moves to the
  phase C entry bar; whichever side reads better wins the formatting argument,
  user taste decides ties.

## ADDENDUM 2 (user, overnight): TS is backend #1, rust is coming

"the technique of direct to ts is also partly one day gonna be to rust so
dont get too hamstrung on pl and ts being 1-1." Binding consequence: the
compiler middle (analyze/strat/lower) produces a TARGET-NEUTRAL plan term
(rel schemas + DDL intent, stratum order, per-rule SQL text, arrival/keyed/
retention semantics, diff-and-log spec) with zero TS idioms; emit_ts.pl is
one backend over that plan and a future emit_rust.pl consumes the same plan
unchanged. SQL text is shared middle content (both targets speak SQLite).
The tick-log envelope is already the cross-target grade. Relayed to the
phase B agent mid-flight.

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

## PHASE C CONTRACT (seeded after reconciliation completed 3/3 byte-identical)

The sweep: run the compiler over EVERY fixture program in
conformance/fixtures/*.pl and grade each into exactly one bucket:
- IDENTICAL: emitted module runs on the A runtime, tick log byte-identical
  to ticklog.pl's oracle log for the fixture's own schedule.
- UNSUPPORTED(constructs): the compiler's supported-subset gate refuses it,
  naming the constructs (aggregates, negation, pre, departed, now, ...).
  Refusal must be a clean named error, never wrong output.
- WRONG: compiles and runs but the log differs — always a bug, in the
  compiler or in the runtime's generality (phase A FINDING 3's carryPending
  simplification is the first suspect for edge-heavy fixtures).
The scoreboard (counts + per-fixture bucket + per-construct unsupported
tally, ranked by how many fixtures each construct blocks) is the
deliverable and drives the construct-implementation backlog. Widening the
supported subset is allowed within the sweep for constructs whose lowering
is unambiguous from engine.pl (each widening = its own commit with the
before/after scoreboard); ambiguous semantics = leave UNSUPPORTED, note the
question. Entry bar for calling phase C DONE (later, not this sweep):
byte-identity emitter-output vs the hand exemplars (formatting argument;
user taste decides ties).

## PHASE C2 CONTRACT (user rulings 2026-07-28 AM)

**Ruling 1 — typed columns, flat compounds** ("please typed yes... but we stay
flat for now to punt"): the compiler reads the fixture Decls' column types and
emits INTEGER columns for int, TEXT for text. The 5 WRONGs caused by "1" vs 1
must flip to IDENTICAL. Compound-term columns STAY inline-flat (canonical text
+ json1 matching as today) — the nested/reference storage model (struct type
as its own rel, parent holds a surrogate id, the v5 intern-dictionary pattern
one level up) is BANKED as its own future design header, never improvised.
Watch the @libsql number->REAL trap (bigint binds, already fixed once).

**Ruling 2 — unmarked edge triggers** ("yes go get this done", after the
model was confirmed NOT whole-world): default edge-rule semantics = an
arrival of ANY rel in the rule's own body is a trigger occurrence, joined
against current state of the other body atoms (the rendezvous/forkJoin case:
the LAST-arriving input completes the join). only(Atom) = the opt-in
restriction (already lowered). Ground every semantics claim in engine.pl /
level_eval.pl line citations and rulings.pl q4/q6/q7 before lowering;
multiplicity (one firing per occurrence vs per matched row) must match the
oracle exactly — the corpus byte-diff is the referee. Target: a large
fraction of the 48 blocked fixtures go IDENTICAL; any fixture this lowering
turns WRONG is a stop-and-report, not a hack site.

Grading note (A3 final-state leg): NOT yet ruled; retention stays invisible
to tick-log grading for now. Do not add a final-state line without user word.

## Sequencing with running work

P0 tracing agent (v6/dl + sqlRunner seams) merges independently; tsv2 consumes
its channels afterward. The F7 crash hunt stays queued on v6/dl. Phase A
touches only new directories + one new prolog file, so it runs concurrently
with anything.
