# Opus dispatch pack — v6 js engine (cut 2026-07-23)

> Sequencing superseded by 2026-07-23-v6-rest-epic-golden-plan.md; BLOCK 0
> (boot + hazard laws) remains canonical and is referenced by the epic plan's
> agents.

Paste BLOCK 0 at the top of the Opus session, then one arc block at a time.
Every receipt below was verified against the code this session (file:line).
Decisions marked USER are Chris's; Opus must not make them.

---

## BLOCK 0 — session boot (paste first, always)

You are working in `~/projects/sprefa/v6/sprefa-store/js/` (TS, Node 24 native TS,
tsgo typecheck, rxjs 7, `@libsql/client` async SQLite). Read before any edit:

- `tasks.d.ts` — RelKind algebra -> ResolveRel -> trinity (Observable/Subject/BehaviorSubject).
- `src/lower/ast.ts`, `src/lower/rulegraph.ts`, `src/lower/lower.ts` — AST contract,
  Tarjan+stratify, the lowering (acyclic combineLatest pipes + recursive in-stratum
  fixpoint; materialized/aggregate recursive strata defer via RecursiveStratumDeferred).
- `src/engine/engine.ts` — the two planes: cascade cx_* (Z-set weights,
  assert/retract/retract_dred/retract_dred_cte) and reconcile rx_* (rx_memo digests,
  mark_changed/dirty/verify/propagate topo sweep with early cutoff). `with_txn` is the
  atomic bracket for interactive round loops.
- `src/engine/lib.ts` — GraphNs (prefix namespacing), RelStore, Store, Interner.
- `src/engine/spine.ts` — spine DDL + `rels.create_rel_table` (runtime rel mint).

Laws (non-negotiable, every edit):
1. Validation = `pnpm typecheck && pnpm test` in `v6/sprefa-store/js/`. Goldens stay green.
2. `intMode:"bigint"` is client-global: every integer column arrives as bigint. Wrap
   `Number()` at number-use sites; digest columns stay bigint end to end (byte-parity).
3. NEVER `client.transaction()` on the shared client (it strands the pinned connection's
   TEMP/:memory: state). NEVER `executeMultiple` inside an open `with_txn` bracket (the
   adapter's finally-ROLLBACK guard, sqlite3.js:161 in the installed client, kills it).
   Atomic = `client.batch([...], "write")` for fixed statement lists, or `with_txn` +
   single-statement `execute` for interactive loops.
4. N+1: never a per-row write. Collect the set, one batched statement per CHUNK.
   `stmt_counter` is the meter.
5. Engine SQL strings stay byte-identical to the Rust originals in
   `v6/sprefa-store/src/*.rs` unless the arc explicitly says otherwise.
6. dl/TS variable names descriptive, never single-letter. Banned identifiers/prose:
   provenance, substrate, load-bearing, regime, and "port" as the rel-boundary noun
   (owner-banned 2026-07-23, a v5 regret; say "host rel". The verb "to port code"
   stays fine).
7. Rows never enter JS heap on the engine plane; heap budget is O(frontier + dirty ids).
   The lower.ts rx plane is heap-resident by design (it is the readable spec plane).
8. Doubt before asserting: verify claims against code, hedge what you did not run.

---

## ARC 1 — RelDecl -> CREATE TABLE port (unhardcode the schema)

Goal: port the Rust decl-to-DDL layer so TS mints rel tables from typed decls instead
of hand-written strings.

Receipts:
- Rust source: `~/projects/sprefa/src/engine/decls.rs` (v5 root). Read it fully first.
- TS target: `src/engine/spine.ts` already has the open mint seam:
  `rels.RelCol` (int | int_null | text), `rels.create_rel_table(db, name, cols, pk)`
  emits `CREATE TABLE IF NOT EXISTS ... WITHOUT ROWID` with composite PK.
- The lowering's decl type is `RelDecl{name, columns, kind, origin}` in src/lower/ast.ts.

Work: a `relTableFor(decl: RelDecl): {name, cols, pk}` mapping + a stamp function that
walks a Program's rels and mints tables. Column typing comes from decls.rs semantics;
if decls.rs carries types the TS AST lacks, EXTEND ast.ts RelDecl (additive only) and
say so in the report. Tests: mint from a hand-built Program, PRAGMA table_info asserts
columns/PK/WITHOUT ROWID; goldens untouched.

Owns: src/engine/spine.ts, src/lower/ast.ts (additive), new tests/engine/decls.test.ts.
Does not touch: engine.ts, lower.ts.

## ARC 2 — stratified negation (the control-flow unlock)

Goal: negated body predicates (`not rel(...)`) with the standard stratification
safety check. This is `else`; cases/guards already exist as multi-rule union + Compare.

Receipts:
- ast.ts:9-11 lists negation as an explicit deferral; body predicates are
  RelRef | Compare (ast.ts:67).
- Stratification machinery exists: rulegraph.ts buildRuleGraph/scc/stratify.
  Negation needs edge polarity: a negative edge inside an SCC is illegal
  (non-stratifiable), a program with one must fail loudly with the cycle named.
- Evaluation: in lowerDerivedRule / stratumFixpoint, a negated ref filters bindings by
  absence (anti-join against the current set of an ALREADY-EVALUATED stratum; the
  stratify order guarantees the negated rel is complete).

Work: `NegRelRef` AST node + constructor; polarity on Graph edges; the safety check
(negative edge in a cycle -> typed error naming the rels); anti-join in the join path
(bindings whose projection hits the negated rel's row set are dropped); tests: even/odd
via negation, a `missing(x) <- all(x), not seen(x)` case, and the non-stratifiable
program error. Reference oracle: from-scratch set difference.

Owns: src/lower/ast.ts, rulegraph.ts, lower.ts, tests/lower/. Does not touch: engine/.

## ARC 3 — @next + @async arms (the impure half of the trinity)

Goal: the two hot trinity rows stop being types-only.

Receipts:
- tasks.d.ts:77-84: StateRel = BehaviorSubject (@next), EventRel = Subject (@in/@out).
- Cutoff knob exists and is parity-proven: `reconcile.verify(db, ns, id, digest, rev)`
  returns moved? (engine.ts, reconcile namespace); RelStore.verify wraps it.
- Reactor shape (pinned, do not redesign): merge -> observeOn -> buffer(tick$) ->
  markChanged -> propagate -> share. propagate is the ascending-id topo sweep.
- GLITCH LAW (pinned this session): effects NEVER hang off raw combineLatest chains
  (diamond double-fire with transiently inconsistent view). @async effects subscribe to
  the tick/propagate path only.

Work: `@next` = BehaviorSubject per state rel, emission taps a digest computation and
consults verify-for-cutoff (unchanged digest -> no downstream emission). `@async` =
`switchMap(() => from(effect))` for cancel-stale demand, `concatMap` for long effects,
`clock(N, bucket)` = shared `interval` salt. AST: temporal modifier fields on RelDecl
(additive). Golden: the gh-cache etag-carry shape — a 304 (unchanged digest) appends
nothing downstream; marble-timed with injected mock effects.

Owns: src/lower/ (additive AST + new arm module), tests/lower/. Reads engine reconcile;
does not modify engine/.

## ARC 4 — cascade-delegate backend + facts() (the engine wiring)

Goal: the deferred recursive shapes (materialized members) run on the SQLite cascade,
and rels read real SQLite instead of injected Observables.

Receipts:
- The seam: lower.ts `recursiveBackend` returns "defer" for materialized members; the
  deferred marker carries the stratum. This arc consumes those markers.
- The knobs are proven: RelStore add_rows/add_deps/assert/retract*/alive_keys
  (lib.ts), rel-table mint (spine.ts rels), the SQL fixpoint pattern
  (labs/fixpoint.ts datalogSqlClosure: INSERT OR IGNORE ... SELECT join loop until
  changes = 0, rows never in JS heap).
- reactor-claims measured: libsql heavy query leaves the loop free (gap 0ms vs 633ms
  blocked). expand earns its keep over async hops — THIS is the async hop.
- BOOKMARK law: any grouping/partitioning lowers into SQL with LIMIT; TS sees only
  bounded results.

Work: (a) generic rule->SQL for a recursive stratum (generalize datalogSqlClosure's
hand-lowered shape to arbitrary stratum rules over minted rel tables); (b) drive it
with `expand` over the async hop, concurrency-capped; (c) `facts(rel)` = cold
Observable that SELECTs the rel table and re-emits on the dirty signal; (d) golden:
same closure programs as tests/lower must produce identical row sets through the SQL
backend (the two backends are cross-oracles). Peak RSS printed and bounded.

Owns: new src/engine/ wiring module + tests. Modifies lower.ts only at the
recursiveBackend dispatch point.

## ARC 5 — builtin-rel registry (the open tool seam)

Goal: user-defined builtin rels: `materialization:"host"` + Op(inputRows)->outputRows.

Receipts: Rust model at `~/projects/sprefa/src/rels/catalog.rs` (read first; it is the
registered-catalog + reserved-name-guard pattern). tasks.d.ts carries
`Materialization = ... | "host"` (renamed from the banned noun 2026-07-23).
ResolveRel needs no change.

Work: a registry keyed by rel name -> Op; lowering treats a host rel as a body source
whose Observable is `map(Op)` over its input rels; registration API with a
reserved-name guard against declared program rels; tests: a custom host rel inside an
acyclic join, and the name-collision error.

Owns: new src/lower/ registry module, lower.ts dispatch, tests/lower/.

---

## USER decisions (Opus must surface, never decide)

- Commit the uncommitted batch (reorg + libsql port + this session's fixes).
- `multi_source_walk` per-row INSERT loops (engine.ts, reach ns): N+1 by the law's
  letter, but SQL-verbatim parity with Rust unverified. Batch or leave.
- Parser lib choice (Epic 5: peggy / lezer / tree-sitter / hand DCG). Nothing imported.
- json-rx port timing: pure math (rulegraph, equiJoin/fixpoint) is already rx-free or
  extractable; when to cut it into the spec package.
