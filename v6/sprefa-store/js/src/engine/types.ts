/**
 * types.ts — the sprefa-store engine surface in one file, C-header style. Every
 * class this package exports has its contract declared here, and the file that
 * implements it imports the contract back and says `implements`. Nothing here
 * has a body: types, interfaces, and type aliases only.
 *
 * Why a header with no Rust twin: every other file in src/engine mirrors a
 * src/*.rs file 1:1, and there is no types.rs. Rust says this with traits
 * (tasks.rs declares Reach/Cascade/Reconcile/GraphStore, and the impls live
 * next to their data). TypeScript classes carry their surface inline instead,
 * so the surface was only readable by reading the implementation. This file is
 * that surface lifted out. src/engine/tasks.ts keeps the four PARITY traits
 * (Reach, Cascade, Reconcile, GraphStore, ported from tasks.rs); they describe
 * roles that several classes can satisfy, and are not duplicated here. The
 * interfaces below describe the concrete classes, one to one.
 *
 * Import direction (the header rule): implementation files import their types
 * from HERE, never from each other. Before this file, lib.ts imported cascade
 * from engine.ts while engine.ts imported GraphNs and SqliteDb back from
 * lib.ts. Type-only, so it never cycled at runtime, but it left no base layer
 * to read first. This file is that base: it imports nothing package-local.
 *
 * The one outside import is `type Client` from @libsql/client, so this header
 * can name the connection type. It is type-only and erases at build. The single
 * RUNTIME import of that package in the whole engine stays in lib.ts, which
 * owns `open_db` (the one sqlite constructor).
 */

import type { Client } from "@libsql/client";

// ─────────────────────────────────────────────────────────────────────────────
// Connection.
// ─────────────────────────────────────────────────────────────────────────────

/** The SQLite connection type (an @libsql/client `Client`, `intMode:"bigint"`). */
export type SqliteDb = Client;

// ─────────────────────────────────────────────────────────────────────────────
// Row shapes (spine entities). Plain data, passed to the batched Store writers.
// ─────────────────────────────────────────────────────────────────────────────

export interface NodeRow {
  node_id: number;
  family: number;
  file_id: number;
  byte_start: number;
  byte_len: number;
  kind: number;
  name_id: number | null;
}

export interface EdgeRow {
  family: number;
  src_id: number;
  dst_id: number;
  kind: number;
}

export interface SpanRow {
  file_id: number;
  start: number;
  end: number;
  string_id: number | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// IGraphNs — the table-name namespace for one graph store (src/engine/lib.ts).
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The namespace for one graph store: every persistent table, index, and TEMP working-table
 * name, built from a prefix. `GraphNs.default()` (empty prefix) is the live `cx_`/`rx_` set;
 * `GraphNs.new("b_")` is an independent store in the same db.
 *
 * PREFIX, not schema-qualify, is the namespace mechanism: SQLite TEMP working tables live in
 * temp. and CANNOT be qualified to an ATTACH'd schema, so prefix is the only namespace that
 * covers the working set.
 */
export interface IGraphNs {
  readonly row: string;
  readonly dep: string;
  readonly memo: string;
  readonly rdep: string;
  readonly ix_dep_child: string;
  readonly ix_rdep_read: string;
  readonly frontier: string;
  readonly next: string;
  readonly hits: string;
  readonly cone: string;
  readonly scc_scope: string;
  readonly scc_frontier: string;
  readonly scc_next: string;
  readonly scc_live: string;
}

/** The static side of the GraphNs class: its constructor and two factories. */
export interface IGraphNsStatics {
  new (
    row: string,
    dep: string,
    memo: string,
    rdep: string,
    ix_dep_child: string,
    ix_rdep_read: string,
    frontier: string,
    next: string,
    hits: string,
    cone: string,
    scc_scope: string,
    scc_frontier: string,
    scc_next: string,
    scc_live: string,
  ): IGraphNs;
  /** `prefix` is prepended verbatim to every base name; pass `"b_"` for a namespace, `""` for
   *  default. Quoted because an unquoted `new(...)` in an interface is a construct signature,
   *  and this one is a static method that happens to be spelled `new`. */
  "new"(prefix: string): IGraphNs;
  /** Empty prefix = the live cx_/rx_ set (the status quo). */
  default(): IGraphNs;
}

// ─────────────────────────────────────────────────────────────────────────────
// IRelStore — a handle on one namespaced graph store (src/engine/lib.ts).
// Two planes, both generic over dense `(rel, row)` keys: FACT (Z-set) and
// CONTROL (salsa-in-sql). Satisfies the Cascade and Reconcile parity traits in
// tasks.ts structurally; this interface is the concrete class surface.
// ─────────────────────────────────────────────────────────────────────────────

export interface IRelStore {
  conn(): SqliteDb;
  /** The namespace this store's `cx_*`/`rx_*` tables live under. */
  ns(): IGraphNs;

  // ---- FACT plane (generic Z-set over (rel,row)) ----------------------------

  /** Insert `(rel, row, weight)` tuples. */
  add_rows(rows: ReadonlyArray<readonly [number, number, number]>): Promise<void>;
  /** Insert dependency edges `(parent_rel, parent_row, child_rel, child_row)`. */
  add_deps(edges: ReadonlyArray<readonly [number, number, number, number]>): Promise<void>;
  /** Forward add: propagate aliveness from `seeds`. Returns rounds. */
  assert(seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
  /** Counting retraction (fast, correct on ACYCLIC support graphs). Returns rounds. */
  retract(seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
  /** Counting retraction with an on-disk SCC-scoped nested fixpoint. Returns rounds. */
  retract_scc(seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
  /** Cycle-safe retraction (Delete-and-Rederive), round loop. Returns rounds. */
  retract_dred(seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
  /** Cycle-safe retraction as two recursive CTEs. Same result as retract_dred. */
  retract_dred_cte(seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
  /** Count live rows (weight > 0) across all relations. */
  alive(): Promise<number>;
  /** The live-row survivor SET as sorted encoded keys (`key = rel*KEY_STRIDE + row`). */
  alive_keys(): Promise<number[]>;

  // ---- CONTROL plane (salsa-in-sql over (rel,row) memos) --------------------

  /** Seed a rel's memo (its output digest + the deps it read), at revision `rev`. */
  seed_memo(
    rel: number,
    row: number,
    digest: bigint,
    deps: ReadonlyArray<readonly [number, number]>,
    rev: number,
  ): Promise<void>;
  /** Bump changed_at for `cells` at `rev` (an input's digest moved). */
  mark_changed(cells: ReadonlyArray<readonly [number, number]>, rev: number): Promise<void>;
  /** The stale frontier as `(rel, row)` pairs. */
  dirty(): Promise<[number, number][]>;
  /** Record a recomputed rel's digest; returns whether it moved (early cutoff). */
  verify(rel: number, row: number, digest: bigint, rev: number): Promise<boolean>;
}

/** The static side of the RelStore class: its constructor and two open paths. */
export interface IRelStoreStatics {
  new (db: SqliteDb, ns: IGraphNs): IRelStore;
  /** Open (or create) a store at `db` stamped for namespace `ns`. */
  attach_with(db: SqliteDb, ns: IGraphNs): Promise<IRelStore>;
  /** Open (or create) a store at `db` with the default namespace (cx_ + rx_). */
  attach(db: SqliteDb): Promise<IRelStore>;
}

// ─────────────────────────────────────────────────────────────────────────────
// IInterner — resident string interning (src/engine/lib.ts, namespace strings).
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The resident interner. Owns the string arena and assigns dense ids. `string_id` is the
 * dense index (0-based, contiguous). The `strings` table is the durable MIRROR of the
 * arena, never the source. New interns queue for ONE batched insert.
 */
export interface IInterner {
  /** Intern `text`, returning its dense `string_id`. Queues `(id, text)` first time seen. */
  intern(text: string): number;
  /** `string_id -> text`, straight from the resident arena, no DB round-trip. */
  resolve(id: number): string | undefined;
  /**
   * Rebuild the arena from the durable mirror on open. Rows MUST arrive in ascending
   * `string_id` order so the reconstructed id equals the stored id, asserted.
   */
  load_row(id: number, content: string): void;
  /** Drain the queued new interns for a batched `strings` insert. */
  take_dirty(): [number, string][];
  len(): number;
  is_empty(): boolean;
}

/** The static side of the Interner class. */
export interface IInternerStatics {
  new (): IInterner;
}

// ─────────────────────────────────────────────────────────────────────────────
// IStore — the spine store, 9 tables (src/engine/lib.ts).
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The one object that speaks the connection. Open applies pragmas + creates the spine,
 * hydrating the resident interner from the durable mirror. Writes are batched; ids/rows
 * are returned to callers, never a SeaORM type.
 */
export interface IStore {
  db(): SqliteDb;

  // ---- strings: resident intern + batched durable flush --------------------

  intern(text: string): number;
  resolve(id: number): string | undefined;
  /** Persist every string interned since the last flush in ONE batched insert. */
  flush_strings(): Promise<number>;

  // ---- dimensions ---------------------------------------------------------

  repo_upsert(slug: string, root: string, url: string): Promise<number>;
  root_insert(repo_id: number, path_string_id: number): Promise<number>;
  /** A committed rev (shared across roots that have this sha). Find-or-insert by (repo_id, git_sha). */
  rev_committed(repo_id: number, git_sha: Uint8Array): Promise<number>;
  /** The WORK rev of a root (its uncommitted working tree). One per root. */
  rev_work(repo_id: number, root_id: number, base_rev_id: number): Promise<number>;

  // ---- files (content, dedup by hash) ------------------------------------

  files_insert_batch(rows: ReadonlyArray<readonly [Uint8Array, number, number]>): Promise<void>;
  file_id_of(content_hash: Uint8Array): Promise<number | null>;
  /** Batch `content_hash -> file_id` resolution: ONE query per CHUNK_ROWS hashes. */
  file_ids_by_hashes(hashes: ReadonlyArray<Uint8Array>): Promise<Map<string, number>>;
  /** Paths in a WORK rev whose content differs from its base HEAD. */
  unstaged_path_ids(work_rev: number): Promise<number[]>;

  // ---- junction: place content at (rev, path) ----------------------------

  place_files_batch(rows: ReadonlyArray<readonly [number, number, number]>): Promise<void>;

  // ---- unified graph -----------------------------------------------------

  nodes_insert_batch(rows: ReadonlyArray<NodeRow>): Promise<void>;
  edges_insert_batch(rows: ReadonlyArray<EdgeRow>): Promise<void>;
  spans_insert_batch(rows: ReadonlyArray<SpanRow>): Promise<void>;
}

/** The static side of the Store class. Its constructor is private: `open` is the only way in. */
export interface IStoreStatics {
  /** Open a store, apply pragmas, create the spine, hydrate the interner mirror. */
  open(db: SqliteDb): Promise<IStore>;
}

// ─────────────────────────────────────────────────────────────────────────────
// ISqliteReach — reachability over cx_dep (src/engine/algo.ts).
// ─────────────────────────────────────────────────────────────────────────────

/**
 * SQLite reach engine over `cx_dep` (the on-disk covering set). Borrows the connection and
 * the store's GraphNs; state is on disk, not in the object. The role trait it satisfies
 * (`Reach`) lives in tasks.ts and is wider than this class: the four methods below are the
 * subset SqliteReach implements directly.
 */
export interface ISqliteReach {
  reaches_from(start: number): Promise<number[]>;
  reached_by(target: number): Promise<number[]>;
  scc_labels(): Promise<[number, number][]>;
  count_pairs(): Promise<bigint>;
}

/** The static side of the SqliteReach class. */
export interface ISqliteReachStatics {
  new (db: SqliteDb, ns: IGraphNs): ISqliteReach;
}

// ─────────────────────────────────────────────────────────────────────────────
// ITemporalStore — bitemporal fact store (src/engine/engine.ts, namespace temporal).
// ─────────────────────────────────────────────────────────────────────────────

/**
 * A bitemporal fact store over one connection. `fact(key, tt_from, tt_to, weight)` WITHOUT
 * ROWID with a partial index over the live set; `commit(deltas)` is one batched atomic
 * write. Role: the versioned base layer UNDER the graph, with no parity trait.
 */
export interface ITemporalStore {
  commit(deltas: ReadonlyArray<readonly [number, number]>): Promise<void>;
  live(): Promise<number>;
  total_rows(): Promise<number>;
  digest(): Promise<number>;
  conn(): SqliteDb;
}

/** The static side of the TemporalStore class. Its constructor is private. */
export interface ITemporalStoreStatics {
  attach(db: SqliteDb): Promise<ITemporalStore>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Static-side proof helper.
//
// `implements` covers the instance side of a class and cannot see statics. Each
// implementation file exports one `AssertTrue<...>` alias per class instead, so
// a factory signature that drifts from this header fails the typecheck rather
// than going quietly out of date. The `false` branch is what makes it bite:
// `never extends true` is true, so a `never` branch would prove nothing.
// ─────────────────────────────────────────────────────────────────────────────

export type AssertTrue<Holds extends true> = Holds;
