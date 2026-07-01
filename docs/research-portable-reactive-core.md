# Portable reactive/retraction core — does v5 have one, what would it cost

Question: is the reactivity math (incremental fixpoint, retraction, what-recomputes-on-delta,
closure/SCC condensation) captured as a backend-agnostic algebra in v5, or welded to SQLite? What
would extraction cost, and how does it compare to v4's reusable libs.

Verdict up front: **No portable core today.** The math is real and correct, but it lives as method
bodies on one `Engine` struct that holds a `rusqlite::Connection`, and the rule evaluator emits SQL
strings. Two pieces are already pure and portable (`scc.rs`, `spine.rs`). The rest is welded. v4 had
the opposite shape: a separate ~9k-LOC crate (`effect_runtime`) built entirely on traits, with a
mem backend and a sqlite backend behind the same interface — but it carried the async parked-wake
runtime the project has since rejected.

---

## 1. v5 today — where the math lives, what is pure vs welded

### File map (src, `wc -l`)

| file | LOC | role | SQLite-coupled? |
|---|---|---|---|
| engine.rs | 2521 | tick, reconcile, fixpoint driver, retraction, cond cache | **yes** — holds `Db`, 34 `.conn()` sites + 92 rusqlite touches |
| modgraph.rs | 820 | language import resolvers (`ModuleResolver` trait) | no (pure extraction) |
| rspath.rs | 336 | use-path rewrite math | no |
| spine.rs | 314 | id primitives (`StringId`/`FileId`/`WhereBytes`/`Coord`) | **no — pure, already portable** |
| typegraph.rs | 312 | syn-based `type_edge` extractor | no |
| parse.rs | 263 | scan/regex/ast/sg extraction | no |
| scc.rs | 180 | Tarjan + condensation + seeded reach | **no — pure graph, already portable** |
| lower.rs | 168 | rule/query → SQL string | **yes — emits SQL text, IS the welding** |
| db.rs | 116 | the only `Connection` owner + N+1 counter | **yes — by design** |

### The reactive tick (engine.rs:702 `tick`, :798 `tick_paths`)

One synchronous pass. No scheduler, no queue, no wake. The shape is:

1. `declare_all` (:1343) — create `rel_<name>` tables / closure VIEWs.
2. `reconcile_sources` (:1139) — diff the file set by content hash, retract changed/deleted paths,
   parse+extract changed files (rayon-parallel, :1196), batch-insert facts + `_prov`.
3. digest prune (`prune_unchanged_by_digest` :1110) — drop source rels whose XOR-fold row digest
   did not move (comment/format edits don't propagate).
4. `affected_derived` (:342) — transitive reach in the rule dependency graph from the changed
   source rels; only those derived heads rebuild.
5. `rebuild_derived` (:1792) — the fixpoint.
6. `rebuild_closures` (:1843) + `refresh_cond_cache` (:2031) — SCC condensation.
7. run queries.

The invariant "no rule may schedule a tick" holds structurally: there is no queue to schedule onto.
Every wake is external — `run` (CLI), `tick_paths` (watcher delta), or LSP keystroke. The tick is a
plain function call.

### The fixpoint evaluator (engine.rs:1792 `rebuild_derived`)

This IS the incremental/semi-naive math, and it is welded:

```
for rel in derived_rels { DELETE FROM rel_<rel> }          // wipe affected heads
for group in stratify(derived_rules)? {                    // stratum-ordered
    loop {
        for ri in group { delta += conn.execute(&lower_rule(rule, rels)?, []) }
        if delta == 0 { break }                            // INSERT OR IGNORE rowcount = naive Δ
    }
}
```

- "Semi-naive" here is SQLite's `INSERT OR IGNORE ... SELECT` returning a changed-row count
  (engine.rs:1801). Convergence = a round inserts 0 new rows. There is no explicit delta relation;
  the set-PK dedup + rowcount stands in for the frontier. The comment at v4 fixpoint.rs:1 calls this
  out as the same trick v4 used.
- Stratification (`stratify` :379) is **pure** — it interns rel names, builds the dependency graph,
  calls `scc::tarjan`, and computes per-stratum order. No SQL. This function could move to a portable
  core unchanged.
- The body of each rule is a SQL string built by `lower_rule` (lower.rs:43). This is the weld point:
  the rule algebra (join on shared var, negation = `NOT EXISTS`, comparison, interpolation) is
  expressed only as SQL text. There is no intermediate relational-algebra IR.

### Retraction (engine.rs:1234 `retract_paths`, :1110 digest prune)

v5 retraction is **two-tier and coarse**, not row-level cascade:

- **Source facts**: `_prov(rel, repo, path, src)` maps each fact's content hash (`__src`) to the
  `(repo, path)` that produced it (insert at :1880). To retract a path: delete its `_prov` rows,
  then `DELETE FROM rel_<r> WHERE __src NOT IN (SELECT src FROM _prov WHERE rel=...)` (:1262). A fact
  with another surviving source path stays. This is an orphan-sweep, run once per relation (:1259),
  O(rels × table), not per-path.
- **Derived facts**: NOT cascaded. `rebuild_derived` wipes every *affected* derived head
  (`affected_derived` reachability) and recomputes from scratch (:1793). There is no per-row
  derivation witness, no support count, no "delete the row that lost its last derivation."

Contrast: v4's fixpoint.rs:1 documents true DRed retraction — each derived row carries one SUPPORT
triple per derivation witness; removing a base row runs `cascade_retract` over the witnesses, and a
row keeps `sum(mult) > 0` if another derivation survives. v5 deliberately dropped that (the memory
ledger flags FactStore/support/witness as DD vocabulary to exorcise) and replaced it with
**recompute-the-affected-subgraph**. Cheaper to reason about, coarser grain.

So v5's "retraction math" is: (a) `_prov` orphan-sweep for sources — portable algebra expressible
over any keyed store; (b) `affected_derived` + wipe-and-refill for derived — portable, but it is a
graph-reachability decision (pure, :342) wrapped around `DELETE`/re-eval (SQL).

### Reactivity / what-recomputes (engine.rs:342 `affected_derived`)

Pure. A fixpoint over the rule dependency graph: seed with changed source rels, propagate to any
head whose body touches an affected rel. Returns the affected heads. No SQL. This is the
"reactive" decision and it is already backend-independent.

### Closure / SCC condensation (scc.rs — entirely pure; engine.rs:1843, :2031 — the SQL glue)

`scc.rs` (180 LOC) is **fully portable**: iterative Tarjan (`tarjan` :13), `build_condensed` (:73),
`count_pairs` (:103), `reaches_from` (:146), `reached_by` (:165). No SQL, no engine types. Its own
header says so ("Pure graph: no SQL, no engine types").

The welding is in the engine: `load_edges` (:1821) reads a 2-col edge relation out of SQLite and
interns to dense u32; `rebuild_closures` (:1843) writes `scc_node_<edge>` / `scc_edge_<edge>` tables;
`declare_closure` (:1763) creates a recursive-CTE VIEW over them; the cross-tick cache
(`refresh_cond_cache` :2031) holds `scc::Cond` in a `HashMap` on the Engine and only re-runs Tarjan
when an edge's `edge_content_digest` (:2008) moves. The perf win — recondense only dirty edges, reuse
the rest — is cache bookkeeping on the Engine, not in `scc.rs`. The cond cache logic is itself
backend-neutral (it operates on `scc::Cond` + a digest); only `load_edges`/`edge_content_digest`
read SQL.

### Abstraction boundary: is there a trait seam?

`grep '^pub trait' src/*.rs`:
- `ModuleResolver` (modgraph.rs:90) — language extraction, not the relation store.
- spine `From` impls — id conversions.

**There is no trait over the relation backend.** `Db` (db.rs:19) is a concrete struct wrapping
`rusqlite::Connection`. Its method set (`exec`, `insert_rows`, `conn`) is plural-by-construction and
documents itself as "the one place SQL is issued, so the backend stays swappable" (db.rs:8) — but
swappable in aspiration only: the return types are `rusqlite` rows, callers reach through `conn()`
(34 sites in engine.rs), and `lower.rs` emits SQL text that only SQLite can run. The seam is a
chokepoint for *counting* (N+1 detector), not an interface anyone could implement against.

---

## 2. v4's "soul" — the reusable libs

The reusable runtime is the crate **`v3/crates/effect_runtime`** (the path is historical; v4 depends
on it as `effect_runtime = { path = "../v3/crates/effect_runtime" }`, v4/Cargo.toml:47). It is a
real standalone crate, trait-based, generic over the carrier value.

### LOC (effect_runtime, `find -name '*.rs' | wc -l`)

| layer | files | LOC |
|---|---|---|
| total crate | 33 | 11903 |
| minus tests (v2/tests.rs) | | 9239 |
| `v2/fact_store.rs` (relation backend trait + Mem + Sqlite impls) | 1 | 1406 |
| `v2/runtime_graph.rs` (the reactive node/edge graph) | 1 | 1141 |
| `v2/sqlite_queue.rs` (durable parked-wake queue) | 1 | 862 |
| `v2/expand.rs` (the driver) | 1 | 575 |
| `v2/mem_queue.rs` | 1 | 439 |
| `v2/memoize.rs` + `memo_seam.rs` | 2 | 487 |
| `v2/component.rs` (the op/Component trait) | 1 | 375 |
| `v2/queue.rs` (QueueBackend trait + park promotion) | 1 | 219 |

On the v4 side, the glue that consumed it (v4/src):

| file | LOC | role |
|---|---|---|
| runtime_graph.rs | 2160 | v4's binding to `effect_runtime::v2::FactRuntimeGraph` |
| sql.rs | 1558 | SQL backend glue |
| mounted_query.rs | 1178 | query mount + `cascade_retract` / support ledger |
| store.rs | 691 | content-id intern over `Arc<dyn FactStore>` |
| stratify.rs | 403 | stratification |
| fact.rs | 433 | FactWrite/FactRead components |
| fixpoint.rs | 246 | DRed semi-naive fixpoint + witness retraction |

### What the soul actually captured (the genuinely reusable part)

1. **`FactStore<R: Row>` trait** (fact_store.rs:66) — the backend-agnostic relation store. Methods:
   `declare`, `insert`/`insert_batch`, `read_where`, `rows_of`, `iter_table`, `len`,
   `delete_matching`, `table_version`, `commit(gen, bus)`. Two impls behind it: `MemFactStore`
   (in-RAM HashMap, :311) and `SqliteFactStore`. **This is precisely the seam v5 lacks.** A rule
   evaluator written against this trait runs unchanged on RAM or SQLite.

2. **`QueueBackend<N: Next>` trait** (queue.rs:92) — pluggable queue with `MemQueue`, `SqliteQueue`,
   `HybridQueue` impls. This is the async parked-wake machinery: `Wake::{Immediate, Tick, Key}`,
   `dispatch_park` promotes parked rows to runnable when a dirty key fires (queue.rs:114), the
   `EventBus` fans dirty events out (`DirtyQueuePromoter`, queue.rs:201). **This is the part the
   project rejected** — it is the redux-out-of-hand async runtime, and a portable v5 core must not
   import it.

3. **`Component` trait** (component.rs) — an op yields rows; the `expand` driver (expand.rs) batches
   per tick. The Haxl/DataLoader applicative-batching shape.

4. **DRed retraction** (fixpoint.rs + mounted_query.rs `cascade_retract`) — per-witness support
   ledger, delete-on-last-witness cascade. Genuinely reusable retraction algebra, but coupled to the
   FactStore + queue.

So v4's soul = ~9.2k LOC, of which the cleanly reusable, backend-neutral part is the FactStore trait
(~1.4k) + Component/expand (~0.95k) + stratify/fixpoint algebra (~0.65k). The rest (~6k: runtime
graph, both queue backends, memo tiers, sql glue) is the async runtime and its durable plumbing —
the part deliberately not being rebuilt.

### Tangled vs portable in v4

Portable in principle (trait-bound, carrier-generic): FactStore, QueueBackend, Component, expand.
Tangled in practice: `runtime_graph.rs` on the v4 side (2160 LOC) bound the generic core to sprefa's
`Cursor` carrier, the dirty-owner worklist, continuations, and SQL — and that binding is most of why
v4 was 58k LOC. The generic crate was clean; the application glue around it was not.

---

## 3. v5-vs-v4, and the extraction gap

- **v5 has no relation-backend trait; v4 had `FactStore<R: Row>` (fact_store.rs:66) with Mem+Sqlite
  impls.** That trait is the single thing whose absence makes v5 "welded." (v4 soul ~9.2k LOC; the
  trait itself ~1.4k.)
- **v5's fixpoint is 1 SQL string per rule (lower.rs:43) run in a rowcount loop
  (engine.rs:1792, ~15 LOC); v4's was a witness-tracked DRed fixpoint (fixpoint.rs, 246 LOC).** v5
  is simpler and coarser: wipe-affected-and-recompute, no per-row support ledger.
- **v5's retraction is `_prov` orphan-sweep for sources + affected-subgraph wipe for derived
  (engine.rs:1234/342); v4's was per-witness cascade.** v5 deliberately dropped the DD-flavored
  support/witness model the memory index flags for removal.
- **v5 already has two pure, portable pieces: `scc.rs` (180 LOC, SCC/closure) and `spine.rs`
  (314 LOC, id math).** Plus three pure functions inside engine.rs that only happen to live there:
  `stratify` (:379), `affected_derived` (:342), `comp_stratum` (:367). v4's portability came from a
  crate boundary; v5's comes from a few functions that never learned they were portable.
- **v5 reactivity is a sync function with no queue (tick/tick_paths); v4 reactivity was the async
  parked-wake queue (queue.rs).** The async runtime is the ~6k LOC the project will not rebuild.

### What extraction would actually touch

To lift a backend-agnostic core out of v5, the work is:

1. **Define a relation-store trait** (`RelStore` or similar) with the v5-shaped method set: `declare`,
   `insert_rows`, `delete_where`, `scan(table) -> rows`, `count`, `exec_select`. Mirror `Db`'s
   plural API (db.rs). ~120 LOC for the trait + a SQLite impl that is today's `Db` renamed. A
   `MemRelStore` (BTreeMap of rows, like v4's `MemFactStore`) is ~250 LOC.
2. **Replace `lower.rs` SQL-text emission with a relational-algebra IR**, then have each backend
   interpret the IR. This is the hard part: today the rule semantics (join/neg/cmp/interp,
   lower.rs:43–134) exist *only* as SQL. A `MemRelStore` cannot run a SQL string. Either (a) define a
   small RA plan enum (Scan/Join/AntiJoin/Filter/Project) the evaluator walks, with a SQL backend
   that compiles it back to today's string and a mem backend that interprets it — ~400–600 LOC new,
   plus rewriting `rebuild_derived` to drive the IR (~150 LOC), or (b) keep SQL as the only backend
   and accept the "portable" claim covers only scc/spine/stratify/affected (no real second backend).
3. **Move the pure functions out**: `scc.rs` and `spine.rs` already qualify; lift `stratify`,
   `affected_derived`, `comp_stratum`, the digest helpers, the cond-cache decision logic
   (`refresh_cond_cache` minus its two SQL reads). ~300 LOC moved, near-zero rewrite.
4. **Leave the queue out.** No `QueueBackend`, no `Wake`, no `EventBus`. The tick stays a function.

**Cost estimate:**
- Thin-seam-only (steps 1, 3; SQL stays the sole backend): **~400–500 LOC new + ~300 moved**, low
  risk, no perf change. Buys: a named trait boundary, the pure math in its own module, the *option*
  of a second backend later. Does not buy a working mem backend.
- Real portable core (add step 2, RA IR + mem backend): **~1200–1600 LOC net new/rewritten**, medium
  risk. Buys: rules runnable without SQLite. Costs: the RA interpreter will not match SQLite's query
  planner on the 16k×96k joins the auto-index comment describes (engine.rs:294–315); a naive mem
  join is the O(F×C) blowup that motivated SQLite in the first place. The perf goals (500-repo batch,
  sub-100ms LSP/keystroke) are met today *because* the join/fixpoint runs in SQLite's planner and the
  cond cache avoids re-Tarjan; a hand-written mem backend re-litigates both.

### Perf note (what the weld buys)

The two reasons v5 is fast are both SQL-or-cache:
- The fixpoint join uses SQLite's planner + the auto-indexes (`auto_indexes` :316,
  `create_auto_indexes` :1331). The comment at :294 documents the 30s→fast collapse from indexing the
  join key. A portable mem backend would need its own index selection.
- The closure avoids the Θ(V²) pair table by condensing once and reusing across ticks
  (`refresh_cond_cache` :2031, `recondensed` counter :481). This part is already pure (`scc.rs`) — it
  ports for free.

So the SCC/closure speed is portable; the fixpoint-join speed is rented from SQLite.

---

## 4. Recommendation

**Thin-seam-only.** Extract a `RelStore` trait mirroring `Db`'s plural API and move the already-pure
math (`scc`, `spine`, `stratify`, `affected_derived`, cond-cache decision) into a backend-neutral
module. Keep SQL as the only backend implementation for now. Do NOT build the RA-IR + mem backend
until a concrete second host demands it.

Rationale:
- The reactivity *model* the project wants (sync tick, external-only wake, no scheduler) is already
  realized and is independent of SQLite — it is the `tick` function, not a runtime. Nothing needs
  extracting to preserve it; the trap is re-importing v4's `QueueBackend`/`Wake` to feel "portable,"
  which would smuggle back the async parked-wake runtime that was deliberately removed.
- The genuinely portable math (closure/SCC, stratification, affected-set, id spine) is small,
  already pure, and one refactor away from living behind a module boundary — cheap, zero perf cost,
  real win.
- A full backend-agnostic rule evaluator means an RA IR + a mem join engine that re-solves the index
  selection and Θ(V²) problems SQLite already solves. That is where the 500-repo / sub-100ms budget
  lives. Pay that cost only against a real second backend, not on spec.

One line: **the math is portable; the rule evaluator is rented from SQLite — extract the math behind
a thin `RelStore` trait now, leave the SQL fixpoint welded, and never re-import v4's queue.**
