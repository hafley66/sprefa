/**
 * tasks.ts — the parity surface as a living task ledger, ported from src/tasks.rs.
 *
 * The Rust Tasks struct's trait bodies are `todo!()` placeholders for impls that ALREADY
 * EXIST in engine.rs/algo.rs/lib.rs. The TS port wires those real impls: the `Tasks` class
 * implements the four trait interfaces (Reach/Cascade/Reconcile/GraphStore) by delegating
 * to the real engine functions — no stubs. GraphStorePlan + the proof tokens are typed
 * (need not be exercised by the golden test).
 *
 * Theory (one semi-naive fixpoint over the derivation graph G, two carried algebras):
 *   Instance A · retraction/dd  (counting semiring ℤ): v_k ∈ ℤ, ⊕=⊗=+, carry = "w_k crossed 0".
 *   Instance B · salsa          (value + Boolean dirty bit): dirty_k ← ∨ dirty_i, carry = "val_k moved".
 * Same machine, different carried value. The WHEN = salsa/control; the WHAT-after = dd/fact.
 */

import { type Observable, concat, map, toArray } from "rxjs";

import { SqliteReach } from "./algo.ts";
import { cascade, reach, reconcile } from "./engine.ts";
import { SqlRunner } from "./sqlRunner.ts";
import type { IGraphNs, IRelStore, QueryResult } from "./types.ts";

// =============================================================================
// Trait A · Reach — read-only graph queries over cx_dep (prune = reached)
// =============================================================================
export interface Reach {
  reaches_from(start: number): Observable<number[]>;
  reached_by(target: number): Observable<number[]>;
  multi_source_walk(
    starts: ReadonlyArray<readonly [number, number, number]>,
    halt: ReadonlyArray<number> | null,
    depth_cap: number | null,
  ): Observable<[number, number, number][]>;
  multi_source_halt_bfs(
    starts: ReadonlyArray<readonly [number, number]>,
    halt: ReadonlyArray<number>,
  ): Observable<[number, number][]>;
  scc_labels(): Observable<[number, number][]>;
  build_condensed(): Observable<reach.Condensed>;
  count_pairs(): Observable<bigint>;
}

// =============================================================================
// Trait B · Cascade — mutating Z-set over cx_row (prune = weight ≠ 0)
// =============================================================================
export interface Cascade {
  assert(seeds: ReadonlyArray<readonly [number, number]>): Observable<number>;
  retract(seeds: ReadonlyArray<readonly [number, number]>): Observable<number>; // acyclic only
  retract_scc(seeds: ReadonlyArray<readonly [number, number]>): Observable<number>; // cycle-safe
  retract_dred(seeds: ReadonlyArray<readonly [number, number]>): Observable<number>;
  retract_dred_cte(seeds: ReadonlyArray<readonly [number, number]>): Observable<number>;
  alive_keys(): Observable<number[]>; // the answer bytes diff'd against the oracle
}

// =============================================================================
// Trait C · Reconcile — salsa-in-SQL digest plane (prune = digest moved)
// =============================================================================
export interface Reconcile {
  seed(id: number, digest: bigint, deps: ReadonlyArray<readonly [number, number]>, rev: number): Observable<QueryResult[]>;
  mark_changed(ids: ReadonlyArray<number>, rev: number): Observable<QueryResult[]>;
  dirty(): Observable<number[]>; // the stale FRONTIER (one-hop, early cutoff)
  verify(id: number, new_digest: bigint, rev: number): Observable<boolean>; // moved? ⇒ cutoff
}

// =============================================================================
// Trait D · GraphStore: node+edge storage both planes sit on (NOT graph parity)
// =============================================================================
export interface GraphStore {
  create(node_value_cols: ReadonlyArray<string>, per_tuple: boolean): void;
  upsert_node(key: number, values: ReadonlyArray<number>): Observable<QueryResult>;
  upsert_edges(edges: ReadonlyArray<readonly [number, number]>): Observable<QueryResult[]>;
  children(key: number): Observable<number[]>; // forward traversal (cascade hits)
  parents(key: number): Observable<number[]>; // reverse traversal (rederive / dirty)
}

// ---- proof tokens RELEASED by an unlanded task (typed, not exercised) ------
export class Namespaced {
  private constructor() {}
  static mint(): Namespaced {
    return new Namespaced();
  }
}
export class Independent {
  private constructor() {}
  static mint(): Independent {
    return new Independent();
  }
}
export class Evidence {
  private constructor() {}
  static mint(): Evidence {
    return new Evidence();
  }
}

/** The remaining plan, as a trait. A method's ARGS are body predicates; RETURN = head. */
export interface GraphStorePlan {
  /** thread IGraphNs through cascade + reconcile + reach (Epic 2). */
  thread_namespace(ns: IGraphNs): Namespaced;
  /** two stores in one db retract without cross-talk (Epic 3). */
  two_stores_independent(proof: Namespaced): Independent;
  /** does per-tuple reconcile beat per-rel on the split shape? the real lever. */
  per_tuple_unlock_evidence(): Evidence;
}

// =============================================================================
// LAYER 2 · the rxjs engine (the reactive layer ON TOP of the cascade above)
// =============================================================================
// The four traits above are LAYER 1 — the SQLite cascade (parity-proven, ported 1:1
// from Rust, golden 11/11, peak RSS 141 MiB). State on disk; RSS = page cache.
// LAYER 2 is the dl→rxjs engine that COMPOSES those knobs + reads the same SQLite.
// Foundation (v6/DECISIONS.md): TS on ACTUAL rxjs; do NOT reinvent or wrap rxjs — use
// its primitives directly. The fixpoint stays in SQLite (the cascade); rxjs owns the
// control plane (demand, dirty, wake, compose) the v5 global tick did badly. Re-doing
// Z-set IVM in TS is the resident-RAM trap the unification killed.
//
// The trinity (the rel-kind algebra IS the type system) — declared in tasks.d.ts:
//   Observable<T>       cold, unicast   a cold/lazy derived rel     (ColdDerivedRel)
//   Subject<T>          hot, multicast  an event feed @in/@out      (EventRel)
//   BehaviorSubject<T>  hot, state      @next / latest-by-gen        (StateRel)
//   + BufferPolicy { replay, onFull } = the one rx↔CSP/Go-channel knob.
// ResolveRel<RelKind, T> maps a kind to its primitive (or never = a type error).
//
// The reactor (engine$, the controlled loop — replaces v5's global DL_POLL_SECS tick):
//   merge(fileEvents$, demand$) → observeOn(loop) → buffer(tick$)
//     → markChanged → propagate(relStore.dirty(), rev) → share()
// markChanged/dirty/propagate are LAYER 1 knobs; propagate is the parity-proven
// ascending topo sweep (src/tasks.rs:33), not the one-hop dirty().
//
// The lowering (a compiler: dl AST → rxjs operator chain; a rel IS the rxjs object its
// RelKind resolves to — no wrapper). Stratify FIRST, the rule graph is static:
//   rulegraph.ts: buildRuleGraph(prog) → scc (Tarjan; reuse this crate's reach) →
//     stratify → ordered strata (single acyclic rel | recursive group). GENERIC json-rx-
//     spec algorithm — reusable beyond lowering, robust to higher-order maps (a rel
//     reading a derived rel is just another edge). Mirrors v5: src/typecheck.rs:1155
//     stratify_diags + src/engine/derive.rs:329 stratum-by-stratum semi-naive
//     (acyclic = one pass; recursive = fixpoint to convergence).
//   lower.ts: lower stratum-by-stratum. EDB = injected source Observable; derived =
//     combineLatest(body) → map(equi-join + project) → filter → aggregate (reduce/scan,
//     e.g. latest-by-gen = keep max). PINNED rxjs ops, no latitude. Recursive strata
//     throw RecursiveStratumDeferred (the fixpoint arc is Epic 2, below).
//
// PARSE vs LOWER — shared contract, parallelizable:
//   The typed AST (src/ast.ts) IS the contract. A future turnkey JS PARSER produces it;
//   rulegraph + lower CONSUME it. So parsing and lowering develop IN PARALLEL against
//   ast.ts. No parser imported yet ("make it work later"); the lowering core is built +
//   golden-tested with hand-built ASTs + injected in-memory sources (no SQLite here).
//
// PROGRESS / FRONTIER (read off this file; full epic tree in the plan doc):
//   [x] L1 cascade port (verbatim Rust→TS) — golden 11/11 (commit 1abbe7b1).
//   [~] L2 lowering core (ast + rulegraph + stratify + join/filter/aggregate) — IN FLIGHT
//       (background agent); recursive/fixpoint strata DEFERRED (RecursiveStratumDeferred).
//   [~] Epic 2 · recursive/fixpoint lowering — the in-memory backend LANDED in lower.ts
//       (lazy strata lower via an in-stratum naive fixpoint, plain `while` per the labs
//       verdict); a materialized member or an aggregate head still defers — that is the
//       cascade-delegate path (needs the engine wiring, Epic 4).
//   [ ] Epic 3 · @next carry (BehaviorSubject + verify cutoff) + @async/jsonp impure arms.
//   [ ] Epic 4 · IO wiring: SQLite-backed facts() sources + engine$ reactor + the Rust
//       extraction CLI boundary (watcher + rayon parse upstream of rxjs; TS sees dirty via
//       SQLite). mergeMap(() => from(rpcReextract), BUDGET) = the demand RPC. BOOKMARK:
//       groupBy/LIMIT pushes INTO SQL at the dirty boundary (RAM thesis).
//   [ ] Epic 5 · parsing (turnkey JS) — PARALLEL to 1-4 against ast.ts; lands anytime.
// Plan: v6/plans/2026-07-23-v6-rxjs-lowering-and-ts-port.md (epic golden plan).

/**
 * The wired impl: every method delegates to the REAL engine functions in engine/algo/lib,
 * exactly as the shipped Rust code defines them (the Rust `todo!()` bodies are placeholders
 * for these). Holds one IRelStore; Reach rides a SqliteReach over its connection + namespace.
 */
export class Tasks implements Reach, Cascade, Reconcile, GraphStore {
  private readonly reacher: SqliteReach;
  constructor(private readonly store: IRelStore) {
    this.reacher = new SqliteReach(store.conn(), store.ns());
  }

  // ---- Reach ----
  reaches_from(start: number): Observable<number[]> {
    return this.reacher.reaches_from(start);
  }
  reached_by(target: number): Observable<number[]> {
    return this.reacher.reached_by(target);
  }
  multi_source_walk(
    starts: ReadonlyArray<readonly [number, number, number]>,
    halt: ReadonlyArray<number> | null,
    depth_cap: number | null,
  ): Observable<[number, number, number][]> {
    return reach.multi_source_walk(this.store.conn(), this.store.ns(), starts, halt, depth_cap);
  }
  multi_source_halt_bfs(
    starts: ReadonlyArray<readonly [number, number]>,
    halt: ReadonlyArray<number>,
  ): Observable<[number, number][]> {
    return reach.multi_source_halt_bfs(this.store.conn(), this.store.ns(), starts, halt);
  }
  scc_labels(): Observable<[number, number][]> {
    return this.reacher.scc_labels();
  }
  build_condensed(): Observable<reach.Condensed> {
    return reach.build_condensed(this.store.conn(), this.store.ns());
  }
  count_pairs(): Observable<bigint> {
    return this.reacher.count_pairs();
  }

  // ---- Cascade ----
  assert(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return this.store.assert(seeds);
  }
  retract(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return this.store.retract(seeds);
  }
  retract_scc(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return this.store.retract_scc(seeds);
  }
  retract_dred(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return this.store.retract_dred(seeds);
  }
  retract_dred_cte(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return this.store.retract_dred_cte(seeds);
  }
  alive_keys(): Observable<number[]> {
    return this.store.alive_keys();
  }

  // ---- Reconcile ----
  /** Seed a rel's memo (id is a dense key; deps are (rel,row) pairs, keyed here). */
  seed(id: number, digest: bigint, deps: ReadonlyArray<readonly [number, number]>, rev: number): Observable<QueryResult[]> {
    const dep_keys = deps.map(([relId, rowIndex]) => cascade.key(relId, rowIndex));
    return reconcile.seed(this.store.conn(), this.store.ns(), id, digest, dep_keys, rev);
  }
  mark_changed(ids: ReadonlyArray<number>, rev: number): Observable<QueryResult[]> {
    return reconcile.mark_changed(this.store.conn(), this.store.ns(), ids, rev);
  }
  dirty(): Observable<number[]> {
    return reconcile.dirty(this.store.conn(), this.store.ns());
  }
  verify(id: number, new_digest: bigint, rev: number): Observable<boolean> {
    return reconcile.verify(this.store.conn(), this.store.ns(), id, new_digest, rev);
  }

  // ---- GraphStore (aspirational API; the traversal methods are real) ----
  create(_node_value_cols: ReadonlyArray<string>, _per_tuple: boolean): void {
    // The split two-plane schema is stamped by `stamp` (IRelStore.attach). A generic node
    // store is the frontier in GraphStorePlan; no-op here pending that measurement.
  }
  upsert_node(key: number, values: ReadonlyArray<number>): Observable<QueryResult> {
    const weight = values.length > 0 ? values[0]! : 1;
    return SqlRunner.execute(
      this.store.conn(),
      `INSERT INTO ${this.store.ns().row}(key,weight) VALUES (${key},${weight}) ON CONFLICT(key) DO UPDATE SET weight=excluded.weight`,
    );
  }
  upsert_edges(edges: ReadonlyArray<readonly [number, number]>): Observable<QueryResult[]> {
    const vals = edges.map(([parentKey, childKey]) => `(${parentKey},${childKey})`).join(",");
    const statements = edges.length === 0
      ? []
      : [`INSERT OR IGNORE INTO ${this.store.ns().dep}(parent_key,child_key) VALUES ${vals}`];
    return concat(...statements.map((statement) => SqlRunner.execute(this.store.conn(), statement))).pipe(toArray());
  }
  children(key: number): Observable<number[]> {
    return SqlRunner.execute(
      this.store.conn(),
      `SELECT child_key FROM ${this.store.ns().dep} WHERE parent_key = ${key} ORDER BY child_key`,
    ).pipe(map((queryResult) => queryResult.rows.map((row) => Number(row[0]))));
  }
  parents(key: number): Observable<number[]> {
    return SqlRunner.execute(
      this.store.conn(),
      `SELECT parent_key FROM ${this.store.ns().dep} WHERE child_key = ${key} ORDER BY parent_key`,
    ).pipe(map((queryResult) => queryResult.rows.map((row) => Number(row[0]))));
  }
}
