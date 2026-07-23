# v6 rxjs lowering + the TS cascade port (engineering plan)

> **Status: engineering plan (not theory).** The theory lives in
> `2026-07-23-v6-reactive-datalog-isomorphism.md` (bannered SUPERSEDED — take its
> RxJS-trinity + BufferPolicy correction; ignore its "locked / 3-primitive" claims).
> The port this plan builds on is **LANDED**: `v6/sprefa-store/js/` (golden 11/11,
> peak RSS 141 MiB). The foundation decision is pinned in `v6/DECISIONS.md`.
> Session record: `chat_log/20260723.2.v6-pivot-ts-on-actual-rxjs-trinity-not-locked.md`.

## What landed (the foundation the lowering composes)

The Rust SQLite cascade ported 1:1 to TS, same file structure, over `better-sqlite3`:

| rust | ts | note |
|---|---|---|
| `src/engine.rs` | `js/src/engine.ts` (966) | cascade/reconcile/reach SQL, strings verbatim |
| `src/lib.rs` | `js/src/lib.ts` (528) | Store, RelStore, GraphNs, stamp, Interner |
| `src/algo.rs` | `js/src/algo.ts` (32) | SqliteReach |
| `src/spine.rs` | `js/src/spine.ts` (238) | schema DDL + row types |
| `src/measure.rs` | `js/src/measure.ts` (249) | benchgraph + oracle_survivors + RSS sampler + soft memcap |
| `src/oracle.rs` (from-scratch math only) | `js/src/oracle.ts` (164) | mix / node_digest / reconcile_stream — dd/salsa NOT ported |
| `src/tasks.rs` | `js/src/tasks.ts` (212) | the four traits as interfaces + Tasks (real bodies) |

Gate: `js/tests/golden.test.ts` (574) — 11/11, self-contained on the ported
from-scratch oracles (`oracle_survivors` + `reconcile_stream.oracle_answer` +
the covering.rs tarjan). **dd::DdBfs and salsa::SalsaReconciler were NOT ported**;
the golden test IS the "way to tell the algorithm is correct" without them.
Compiler: `tsgo` (`@typescript/native-preview`). Runtime: Node 24 native TS.
Load-bearing port decisions: 64-bit digests as `bigint` + `.safeIntegers(true)`
(required for reconcile byte-parity); SeaORM/sqlx async → better-sqlite3 sync;
`memcap` is a SOFT RSS guard (Node cannot `setrlimit`, hard cap stays Rust-side).

The RelStore knobs (`markChanged` / `dirty` / `propagate` / `assert` / `retract` /
`retract_scc` / `retract_dred` / `retract_dred_cte` / `alive_keys` / `seed_memo` /
`verify`) + SQLite reads are now callable from TS, and `rx_dep` is in the ported
engine. **That is what the rxjs lowering composes.**

The mind, three ways (cross-linked): Rust `src/tasks.rs` ↔ TS `js/src/tasks.ts` ↔
the rxjs rel-kind type ledger `js/tasks.d.ts` (`RelKind` → `ResolveRel`).

## The lowering, top-down

Each dl rel compiles straight to the rxjs object its kind resolves to. No wrapper
class, no re-implementation of subscribe/pipe — a rel IS the rxjs object. The
lowering is a compiler, dl AST → operator chain.

**Top-level reactor** (replaces v5's global `DL_POLL_SECS` tick):

```ts
const tick$ = interval(DL_POLL_SECS * 1000).pipe(share());         // the cadence drum
const engine$ = merge(fileEvents$, demand$).pipe(
  observeOn(asyncScheduler),                                       // onto the event loop
  buffer(tick$),                                                   // coalesce frontier → one batch/tick
  tap(batch => relStore.markChanged(changedCells(batch), nextRev())), // control: what moved
  switchMap(() => propagate(relStore.dirty(), rev)),               // fact: re-derive dirty frontier
  share(),                                                         // multicast — one derivation, many readers
);
```

`markChanged` / `dirty` / `propagate` are the ported knobs; `propagate` is the
parity-proven ascending topo sweep (`src/tasks.rs:33`), not the one-hop `dirty()`.

**Raw fact join** (two EDB facts → one derived rel, no impure):

```dl
rel watch(ep: text).          watch("repos/cli/cli").
rel label(ep: text, name: text).  label("repos/cli/cli", "tui").
rel watch_label(ep: text, name: text).
watch_label(ep, name) <- watch(ep), label(ep, name).
```
```ts
const watch$: Observable<[ep: string]>               = facts("watch");
const label$: Observable<[ep: string, name: string]> = facts("label");
const watch_label$ = combineLatest([watch$, label$]).pipe(
  map(([w, l]) => {
    const watched = new Set(w.map(([ep]) => ep));     // build side
    return l.filter(([ep]) => watched.has(ep))        // probe side
            .map(([ep, name]) => [ep, name] as [string, string]);
  }),
  shareReplay(1),
);
watch_label$.subscribe(rows => sink(rows));           // ? watch_label(...) = the demand
```

Trinity mapping: EDB facts + acyclic derived = `ColdDerivedRel` (cold `Observable`,
`ResolveRel` shape `"pipe"` temperature `"cold"`). `Subject` / `BehaviorSubject`
arrive with the impure half (`@in`/`@out` = Subject; `@next` = BehaviorSubject whose
emit taps `verify` for cutoff; `@async` = `switchMap` arm into `from(effect)`).

Whole-relation re-derive via `combineLatest` is the simple form (correct for raw
facts). The incremental Δ-join is the `expand`/fixpoint path, selected later by
`RelKind.materialization` for recursive rels.

## Concurrency / parse pools (async vs parallel — the line that matters)

rxjs owns **async** (overlap, await, interleave) natively. It does NOT own
**parallel** (two cores busy at once). For parallel, hand work to `worker_threads`
or a child process; rxjs dispatches and awaits.

- `mergeMap(project, n)` does two jobs: caps in-flight awaits (backpressure) AND,
  when each await is on a worker/process, those n in-flight items run on n cores.
- **Parse pool (buy, do not build):** `piscina` (worker_threads pool) — the
  default for an in-process pool. OR the **Rust extraction CLI + `execa`** — one
  process per independent shard, facts land in SQLite the engine reads next tick
  (matches the foundation decision; truest isolation, Rust already written). Move
  to WASM-under-`piscina` only if process startup bites at cadence.
- **Frontier partitioning:** `rx_dep` already encodes the edges. `groupBy(cell =>
  componentOf(cell))` → `mergeMap(partition$ => rederivePartition(partition$),
  MAX_PARTITIONS)`. Independent components race; a component with internal deps
  stays internally sequential (the topo sweep).

The one knob is a concurrency integer = the thread/process budget (the standing
"nothing seizes the machine" law). No bespoke pool, no rxjs wrapper.

## BOOKMARK — push groupBy + LIMIT into SQLite (owner, 2026-07-23)

`dirty()` is ALREADY a SQLite boundary. When the lowering needs a `groupBy`
(frontier partitions, aggregations, `@next` latest-by-gen), **lower it into SQL
with `LIMIT`, not into a TS array.** Pulling a partition into TS heap to group it
re-creates the resident-set death the unification killed (`v6/DECISIONS.md`).
`GROUP BY … LIMIT` + `dirty`-keyed reads keep the partition on disk; TS sees only
the bounded result. This is the rule for any grouping the lowering emits, not an
opt. Open question to resolve when the first `groupBy` lands: does the rxjs
`groupBy` operator stay as the dispatch key while SQL does the heavy grouping, or
does the whole group live in SQL and rxjs only fans out the bounded results?

## Relates to (cross-links)

- `chat_log/20260723.2.*.md` — session record; foundation decisions + port landed.
- `v6/DECISIONS.md` — the unification pin; this plan inherits "SQLite owns the fixpoint."
- `2026-07-23-v6-reactive-datalog-isomorphism.md` — theory (bannered; trinity correction only).
- `v6/plans/2026-07-19-v6-demand.md` — cold-by-default / Observable + subscribe (the demand ruling this lowering instantiates).
- `v6/plans/2026-07-19-reactive-style-port.md` — the rxjs-concept → target mapping table (Appendix J/K).
- `v6/sprefa-store/src/tasks.rs` / `js/src/tasks.ts` / `js/tasks.d.ts` — the mind, three languages.
- `v6/AGENTS.md` — the one pattern (the semi-naive cascade) this lowering drives.
