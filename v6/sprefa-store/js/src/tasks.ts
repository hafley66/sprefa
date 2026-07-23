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

import { SqliteReach } from "./algo.ts";
import { cascade, reach, reconcile } from "./engine.ts";
import type { GraphNs, RelStore } from "./lib.ts";

// =============================================================================
// Trait A · Reach — read-only graph queries over cx_dep (prune = reached)
// =============================================================================
export interface Reach {
  reaches_from(start: number): number[];
  reached_by(target: number): number[];
  multi_source_walk(
    starts: ReadonlyArray<readonly [number, number, number]>,
    halt: ReadonlyArray<number> | null,
    depth_cap: number | null,
  ): [number, number, number][];
  multi_source_halt_bfs(
    starts: ReadonlyArray<readonly [number, number]>,
    halt: ReadonlyArray<number>,
  ): [number, number][];
  scc_labels(): [number, number][];
  build_condensed(): reach.Condensed;
  count_pairs(): bigint;
}

// =============================================================================
// Trait B · Cascade — mutating Z-set over cx_row (prune = weight ≠ 0)
// =============================================================================
export interface Cascade {
  assert(seeds: ReadonlyArray<readonly [number, number]>): number;
  retract(seeds: ReadonlyArray<readonly [number, number]>): number; // acyclic only
  retract_scc(seeds: ReadonlyArray<readonly [number, number]>): number; // cycle-safe
  retract_dred(seeds: ReadonlyArray<readonly [number, number]>): number;
  retract_dred_cte(seeds: ReadonlyArray<readonly [number, number]>): number;
  alive_keys(): number[]; // the answer bytes diff'd against the oracle
}

// =============================================================================
// Trait C · Reconcile — salsa-in-SQL digest plane (prune = digest moved)
// =============================================================================
export interface Reconcile {
  seed(id: number, digest: bigint, deps: ReadonlyArray<readonly [number, number]>, rev: number): void;
  mark_changed(ids: ReadonlyArray<number>, rev: number): void;
  dirty(): number[]; // the stale FRONTIER (one-hop, early cutoff)
  verify(id: number, new_digest: bigint, rev: number): boolean; // moved? ⇒ cutoff
}

// =============================================================================
// Trait D · GraphStore: node+edge storage both planes sit on (NOT graph parity)
// =============================================================================
export interface GraphStore {
  create(node_value_cols: ReadonlyArray<string>, per_tuple: boolean): void;
  upsert_node(key: number, values: ReadonlyArray<number>): void;
  upsert_edges(edges: ReadonlyArray<readonly [number, number]>): void;
  children(key: number): number[]; // forward traversal (cascade hits)
  parents(key: number): number[]; // reverse traversal (rederive / dirty)
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
  /** thread GraphNs through cascade + reconcile + reach (Epic 2). */
  thread_namespace(ns: GraphNs): Namespaced;
  /** two stores in one db retract without cross-talk (Epic 3). */
  two_stores_independent(proof: Namespaced): Independent;
  /** does per-tuple reconcile beat per-rel on the split shape? the real lever. */
  per_tuple_unlock_evidence(): Evidence;
}

/**
 * The wired impl: every method delegates to the REAL engine functions in engine/algo/lib,
 * exactly as the shipped Rust code defines them (the Rust `todo!()` bodies are placeholders
 * for these). Holds one RelStore; Reach rides a SqliteReach over its connection + namespace.
 */
export class Tasks implements Reach, Cascade, Reconcile, GraphStore {
  private readonly reacher: SqliteReach;
  constructor(private readonly store: RelStore) {
    this.reacher = new SqliteReach(store.conn(), store.ns());
  }

  // ---- Reach ----
  reaches_from(start: number): number[] {
    return this.reacher.reaches_from(start);
  }
  reached_by(target: number): number[] {
    return this.reacher.reached_by(target);
  }
  multi_source_walk(
    starts: ReadonlyArray<readonly [number, number, number]>,
    halt: ReadonlyArray<number> | null,
    depth_cap: number | null,
  ): [number, number, number][] {
    return reach.multi_source_walk(this.store.conn(), this.store.ns(), starts, halt, depth_cap);
  }
  multi_source_halt_bfs(
    starts: ReadonlyArray<readonly [number, number]>,
    halt: ReadonlyArray<number>,
  ): [number, number][] {
    return reach.multi_source_halt_bfs(this.store.conn(), this.store.ns(), starts, halt);
  }
  scc_labels(): [number, number][] {
    return this.reacher.scc_labels();
  }
  build_condensed(): reach.Condensed {
    return reach.build_condensed(this.store.conn(), this.store.ns());
  }
  count_pairs(): bigint {
    return this.reacher.count_pairs();
  }

  // ---- Cascade ----
  assert(seeds: ReadonlyArray<readonly [number, number]>): number {
    return this.store.assert(seeds);
  }
  retract(seeds: ReadonlyArray<readonly [number, number]>): number {
    return this.store.retract(seeds);
  }
  retract_scc(seeds: ReadonlyArray<readonly [number, number]>): number {
    return this.store.retract_scc(seeds);
  }
  retract_dred(seeds: ReadonlyArray<readonly [number, number]>): number {
    return this.store.retract_dred(seeds);
  }
  retract_dred_cte(seeds: ReadonlyArray<readonly [number, number]>): number {
    return this.store.retract_dred_cte(seeds);
  }
  alive_keys(): number[] {
    return this.store.alive_keys();
  }

  // ---- Reconcile ----
  /** Seed a rel's memo (id is a dense key; deps are (rel,row) pairs, keyed here). */
  seed(id: number, digest: bigint, deps: ReadonlyArray<readonly [number, number]>, rev: number): void {
    const dep_keys = deps.map(([r, w]) => cascade.key(r, w));
    reconcile.seed(this.store.conn(), this.store.ns(), id, digest, dep_keys, rev);
  }
  mark_changed(ids: ReadonlyArray<number>, rev: number): void {
    reconcile.mark_changed(this.store.conn(), this.store.ns(), ids, rev);
  }
  dirty(): number[] {
    return reconcile.dirty(this.store.conn(), this.store.ns());
  }
  verify(id: number, new_digest: bigint, rev: number): boolean {
    return reconcile.verify(this.store.conn(), this.store.ns(), id, new_digest, rev);
  }

  // ---- GraphStore (aspirational API; the traversal methods are real) ----
  create(_node_value_cols: ReadonlyArray<string>, _per_tuple: boolean): void {
    // The split two-plane schema is stamped by `stamp` (RelStore.attach). A generic node
    // store is the frontier in GraphStorePlan; no-op here pending that measurement.
  }
  upsert_node(key: number, values: ReadonlyArray<number>): void {
    const weight = values.length > 0 ? values[0]! : 1;
    this.store.conn().exec(
      `INSERT INTO ${this.store.ns().row}(key,weight) VALUES (${key},${weight}) ON CONFLICT(key) DO UPDATE SET weight=excluded.weight`,
    );
  }
  upsert_edges(edges: ReadonlyArray<readonly [number, number]>): void {
    if (edges.length === 0) return;
    const vals = edges.map(([p, c]) => `(${p},${c})`).join(",");
    this.store
      .conn()
      .exec(`INSERT OR IGNORE INTO ${this.store.ns().dep}(parent_key,child_key) VALUES ${vals}`);
  }
  children(key: number): number[] {
    return this.store
      .conn()
      .prepare(`SELECT child_key FROM ${this.store.ns().dep} WHERE parent_key = ${key} ORDER BY child_key`)
      .pluck(true)
      .all() as number[];
  }
  parents(key: number): number[] {
    return this.store
      .conn()
      .prepare(`SELECT parent_key FROM ${this.store.ns().dep} WHERE child_key = ${key} ORDER BY parent_key`)
      .pluck(true)
      .all() as number[];
  }
}
