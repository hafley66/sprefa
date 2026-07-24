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

## Epic golden plan (parse vs lower, bound by the AST contract)

> Recursive task tree. Sequencing: **Epic 5 (parsing) runs PARALLEL to Epics 1→4
> (lowering + engine)** against the Epic 1.1 AST contract. Within the lowering,
> 1 → 2 → 3 → 4 (each depends on the prior). Epic 0 is DONE (reference).

### Recon facts (observed, not guessed)
- Port LANDED: `js/src/{engine,lib,algo,spine,measure,oracle,tasks,index}.ts` (2976 lines), golden 11/11, peak RSS 141 MiB; RelStore knobs + SQLite callable from TS (commit `1abbe7b1`).
- v5 stratification model: `src/typecheck.rs:1155 stratify_diags(prog, dl_path)`; `src/engine/derive.rs:329 rebuild_derived` evaluates stratum-by-stratum (acyclic = one pass; recursive component = semi-naive fixpoint to convergence); `src/graph/scc.rs` = Tarjan. The lowering mirrors this.
- dl constructs in scope (`examples/gh-cache.dl`): rel decl, EDB facts, `head <- body`, body = rel predicates sharing vars, selection/comparison, head aggregate `max(b)`.
- Trinity + RelKind declared in `js/tasks.d.ts` (`ResolveRel` maps kind → primitive).
- IN FLIGHT (background agent): `src/ast.ts`, `src/rulegraph.ts`, `src/lower.ts`, `tests/lower.test.ts`. DEFERRED in that arc: recursive/fixpoint (`expand`), `@next`, `@async`.

### Plan + lowering boundary
- **Authoring surface:** dl text (`examples/*.dl`). Owned by Epic 5 (parser), deferred.
- **Canonical rep:** the typed AST (Epic 1.1, `src/ast.ts`) — THE shared contract.
- **Static analysis:** the stratified rule graph (Epic 1.2, `src/rulegraph.ts`).
- **Runtime IR:** rxjs operator chains (Epic 1.3 / 2 / 3, `src/lower.ts`).
- **Target runtime:** Node + rxjs Observables; facts from SQLite (Epic 4). Diagnostics owned by the lowering (type errors via `RelKind`; stratification errors via `rulegraph`).

### Epic 0 · the cascade foundation — DONE (reference)
- **Goal:** the SQLite cascade in TS (Layer 1).
- **Contract:** `Reach`/`Cascade`/`Reconcile`/`GraphStore` (`js/src/tasks.ts`) over `RelStore` (`js/src/lib.ts`).
- **Done:** golden 11/11, peak RSS 141 MiB (`1abbe7b1`). No further work.

### Epic 1 · the lowering core (IN FLIGHT) — parse ∥ this, bound by 1.1
- **Goal:** lower a hand-built AST (acyclic dl: joins, selection, aggregation) to rxjs, golden-gated.
- **Contract:**
  - `ast.ts`: `RelDecl{name,columns,kind,origin}`; `Rule{head, headTerms, body}`; body predicate = `RelRef{name, args:(Var|Lit)[]}` | `Compare{...}`; headTerm = `Var` | `Max(Var)` | …
  - `rulegraph.ts`: `buildRuleGraph(prog)→Graph`; `scc(Graph)→SCC[]`; `stratify(Graph,SCC)→Stratum[]` where `Stratum = {rels:string[], recursive:boolean, order:number}`.
  - `lower.ts`: `lowerProgram(prog, sources:Map<string,Observable<Row[]>>)→Map<string,Observable<Row[]>>`; throws `RecursiveStratumDeferred` for a recursive stratum.
- **Pseudocode** (lower, acyclic stratum):
  ```ts
  const body$ = combineLatest(rule.body.map(p => sources.get(p.rel)!));   // join sources
  return body$.pipe(
    map(rows => equiJoin(rows, sharedVars(rule))),                         // join + project head
    filter(row => rule.selections.every(s => evalSel(row, s))),            // selection
    aggregateHead(rule.headTerms),                                         // reduce/scan: max etc.
  );
  ```
- **Instance timeline:** `lowerProgram` builds the Observable graph once (cold); each rel's Observable is lazy (subscribes on demand); disposed when the engine drops it.
- **Storage / identity:** rel name = identity; rows = readonly tuples; no SQLite in this epic (sources injected). Source-map: each output row carries the body rows it joined (future).
- **Recursive tasks:**
  - `1.1` `src/ast.ts` — the typed program input (the shared contract). [IN FLIGHT]
  - `1.2` `src/rulegraph.ts` — `buildRuleGraph` + `scc` (Tarjan) + `stratify`. [IN FLIGHT]
    - `1.2.1` SCC over the rule dep graph.
    - `1.2.2` stratify: condense + topo-sort; mark recursive strata.
  - `1.3` `src/lower.ts` — lower stratum-by-stratum. [IN FLIGHT]
    - `1.3.1` EDB rel → injected source.
    - `1.3.2` derived acyclic → combineLatest+map+filter+aggregate.
    - `1.3.3` recursive stratum → throw `RecursiveStratumDeferred`.
  - `1.4` `tests/lower.test.ts` — golden (rulegraph cases + lowering cases vs from-scratch). [IN FLIGHT]
- **Lowering path:** AST → stratify → per-stratum rxjs. Diagnostics: `RecursiveStratumDeferred` names the cycle (fixpoint lowering is Epic 2).
- **Done condition:** `pnpm typecheck` clean + `pnpm test` green (`golden.test.ts` 11/11 + `lower.test.ts`).
- **Epic golden test:** a 2-fact join + a 3-rel join + a join+selection + a latest-by-gen `max(b)` aggregation, each derived emission === from-scratch reference; rulegraph SCC/stratification for chain/diamond/cycle/self-loop === from-scratch; `RecursiveStratumDeferred` marker on a cyclic program.

### Epic 2 · recursive/fixpoint lowering (`expand`) — co-design, depends on 1.2
- **Goal:** lower a recursive stratum to a rxjs fixpoint (the dd `iterate` in rxjs).
- **Contract:** `lower.ts` gains `lowerRecursiveStratum(stratum, sources)→Observable<Row[]>` via `expand` (Δ → nextFrontier | EMPTY) converged to quiescence; OR delegate to the Rust cascade (`assert`/`retract`, read `alive_keys`). PICK (frontier).
- **Pseudocode** (expand form): `seeds.pipe(expand(Δ => nextDelta(Δ).pipe(takeWhile(notEmpty))))`.
- **Instance timeline:** the fixpoint Observable completes when Δ empties; resubscribes re-run.
- **Storage / identity:** same rel/row identity; the fixpoint writes the stratum's materialized set.
- **Recursive tasks:**
  - `2.1` decide `expand`-in-rxjs vs delegate-to-cascade (frontier; see Frontier).
  - `2.2` implement `lowerRecursiveStratum` for the chosen path.
  - `2.3` golden: transitive-closure program === from-scratch closure; SCC-internal fixpoint.
- **Done condition:** a recursive dl program (e.g. `ancestor <- parent; parent`) lowers + emits the correct fixpoint, golden vs from-scratch.
- **Epic golden test:** transitive closure over a small parent graph === from-scratch, incl. a cycle converges (weight/counting, not infinite).

### Epic 3 · `@next`/reconcile + impure arms — depends on 1, 2
- **Goal:** lower `@next` (state, latest-by-gen) and `@async`/`jsonp` (impure) as rxjs arms.
- **Contract:**
  - `@next` → `BehaviorSubject<Digest>`; tap → `RelStore.verify(rel,row,digest)` for cutoff (Layer 1).
  - `@async` → `switchMap(() => from(effect))`; long effect = `concatMap`. `clock(N,bucket)` = `interval` salt.
  - `jsonp(body,…)` → `switchMap` extraction over the bound body string (the Rust CLI boundary).
- **Instance timeline:** `BehaviorSubject` lives for the engine lifetime; `@async` Observables cold per demand; clock interval shared.
- **Storage / identity:** `@next` state keyed by head rel+row; digest via the reconcile plane.
- **Recursive tasks:**
  - `3.1` `@next` carry + `verify` cutoff (wire to Layer 1 reconcile).
  - `3.2` `@async` arm (`switchMap` into `from(effect)`) + `clock(N,_)` = interval salt.
  - `3.3` `jsonp`/`json` extraction arm (the Rust CLI rpc; BOOKMARK: result bounded, groupBy in SQL).
- **Done condition:** a `gh-cache.dl` subset (`poll`/`resp`/`etag_next`/`stars`) lowers + runs against injected/mock effects, golden vs expected etag carry + entity rows.
- **Epic golden test:** the etag-carry + 304-collapse + `stars`-extract slice from `gh-cache.dl`, marble-timed, vs expected rows (a 304 appends nothing to `change_log`).

### Epic 4 · IO/reactor wiring + the Rust boundary — depends on 1-3
- **Goal:** the live engine — SQLite-backed `facts()` sources, the `engine$` reactor, the Rust extraction CLI boundary, the dirty signal, groupBy→SQL.
- **Contract:**
  - `facts(rel)` → cold `Observable<Row[]>` that `SELECT`s the rel table + re-emits on dirty.
  - `engine$ = merge(fileEvents$,demand$) → observeOn(loop) → buffer(tick$) → markChanged → propagate(relStore.dirty(),rev) → share()`.
  - demand RPC: `mergeMap(() => from(rpcReextract(sources)), BUDGET)` (BUDGET = thread cap).
  - BOOKMARK: groupBy/LIMIT pushed INTO SQL at the dirty boundary (RAM thesis).
- **Instance timeline:** `engine$` lives for the daemon lifetime; `facts()` cold per subscriber; the Rust CLI subprocess per demand, budget-capped.
- **Storage / identity:** SQLite tables (Layer 1 `GraphNs`); dirty = the reconcile frontier; the shared db file is the 2-hand seam (Rust writes, TS reads).
- **Recursive tasks:**
  - `4.1` `facts(rel)` — SQLite SELECT re-emit on dirty.
  - `4.2` `engine$` reactor — merge / buffer(tick$) / markChanged / propagate / share.
  - `4.3` demand RPC (`mergeMap`→`from(rpcReextract)`, BUDGET).
  - `4.4` groupBy/LIMIT into SQL (BOOKMARK) — partition reads stay on disk.
  - `4.5` golden: an end-to-end tick (seed via Rust-CLI-stand-in → `facts()` → derived) + peak RSS.
- **Done condition:** an end-to-end reactive run over a real(ish) db, peak RSS bounded.
- **Epic golden test:** changed paths → dirty → a derived rel re-emits the joined set; peak RSS printed + under budget; a groupBy partition read does not pull the full set into TS heap.

### Epic 5 · parsing (turnkey JS) — PARALLEL to 1-4, against the 1.1 AST contract
- **Goal:** dl text → the typed AST (`src/ast.ts`). Turnkey JS parser; no reinvention.
- **Contract:** `parseDl(text)→Program` (`RelDecl[]` + `Rule[]` + facts); parse errors → diagnostics owned here, surfaced to the lowering's type/stratification checks.
- **Instance timeline:** parse once per program load / per `.dl` file change (re-parse on watch).
- **Storage / identity:** source-map row → AST node (for diagnostics); the AST is immutable per parse.
- **Recursive tasks:**
  - `5.1` pick the turnkey parser (frontier — defer; "make it work later"; no lib imported yet).
  - `5.2` parse rel decl + EDB facts + `head <- body` + body predicates + head aggregates.
  - `5.3` source-map (AST node ↔ source span) for diagnostics.
  - `5.4` golden: `examples/gh-cache.dl` parses to the AST the lowering accepts (round-trip with 1.1).
- **Done condition:** `gh-cache.dl` parses to an AST that lowers (Epic 1) without hand-editing.
- **Epic golden test:** `parse(gh-cache.dl)` === the hand-built AST used in Epic 1's golden (the contract round-trips); diagnostics snapshot for a malformed program.

### Frontier (deferred decisions + evidence needed)
- **Epic 2 fork:** recursive fixpoint via rxjs `expand` (pure TS dataflow) vs delegate to the Rust cascade (rxjs triggers `assert`/`retract`, reads `alive_keys`). Evidence: the port's RSS confirms SQLite owns the data → lean delegate for heavy/recursive; `expand` for small/acyclic demand. Decide when Epic 1 lands.
- **Epic 5 parser choice:** turnkey JS lib (peggy / tree-sitter / lezer / …) vs hand-written. Owner defers ("make it work later"); no lib imported until chosen. Evidence: dl grammar size + the editor/LSP story.
- **groupBy ownership (Epic 4.4):** does rxjs `groupBy` stay as the dispatch key while SQL does the heavy grouping, or does the whole group live in SQL + rxjs fans out bounded results? Evidence: the first real partitioned workload.
- **Parse-vs-lower parallelism is SAFE** because the AST (Epic 1.1) is the contract; both sides code to it. No serialization between them until Epic 4 wires the live run.
