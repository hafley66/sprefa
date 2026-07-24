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
// DATAFLOW · the free functions that move data across the interfaces above.
//
// Classes hold state; these do the work. Each namespace below is declared as one
// interface so the module's whole surface reads in a block, and the implementing
// file proves `typeof <namespace> extends I<Namespace>Api`. The connection and
// the namespace are ARGUMENTS, not fields: state lives in SQLite, and these
// functions borrow the handle. That is the shape the whole store is built on,
// so it is the shape a reader should meet here first.
//
// Read the flow bottom-up: a caller opens with `OpenDb` -> `Stamp` (or
// `IStoreStatics.open` / `IRelStoreStatics.attach`), loads through `IIngestApi`
// or the batched `IStore` writers, mutates through `ICascadeApi`, asks questions
// through `IReachApi`, and tracks staleness through `IReconcileApi`.
// ─────────────────────────────────────────────────────────────────────────────

/** Data shapes the surfaces below carry. Declared here, re-exported by the file that owns
 *  the functions, so this header stays free of package-local imports. */

/** Condensation, all component ids expressed as MIN-member representative keys. */
export interface Condensed {
  comp_of: [number, number][]; // (node_key, comp_repr)
  size: [number, number][]; // (comp_repr, member_count)
  cyclic: [number, boolean][]; // (comp_repr, is_cyclic)
  cadj: [number, number][]; // (parent_comp_repr, child_comp_repr), deduped, no self
}

/** A column in a dynamically-created rel table. */
export type RelCol =
  | { kind: "int"; name: string }
  | { kind: "int_null"; name: string }
  | { kind: "text"; name: string };

/** One parsed line of the ingest stream. */
export type FactLine =
  | { t: "str"; id: number; s: string }
  | { t: "file"; hash: string; size: number; lines: number }
  | { t: "node"; family: number; file: number; start: number; len: number; kind: number; name: number | null }
  | { t: "edge"; family: number; src: number; dst: number; kind: number }
  | { t: "rel"; name: string; row: readonly unknown[] };

export interface IngestReport {
  lines: number;
  stmts: number;
  changed: [number, number][];
}

// ---- open / stamp (src/engine/lib.ts, src/engine/spine.ts) ------------------

/** The one sqlite constructor. `url` is `file:/abs/path.sqlite` or `:memory:`. */
export type OpenDb = (url: string) => SqliteDb;

/** Stamp the split two-plane schema (cascade `cx_*` + reconcile `rx_*` + TEMP working set +
 *  indexes) under namespace `ns` onto `db`, pragmas first. */
export type Stamp = (db: SqliteDb, ns: IGraphNs) => Promise<void>;

/** Create the 9 spine tables (strings/files/nodes/edges/spans/repos/revs/junction/meta). */
export type CreateAllTables = (db: SqliteDb) => Promise<void>;

// ---- cascade (src/engine/engine.ts) ----------------------------------------

/**
 * The FACT plane as free functions: Z-set weights over `cx_row`, support edges over
 * `cx_dep`. `IRelStore` is the object-shaped wrapper over exactly these.
 */
export interface ICascadeApi {
  /** `key = tag * KEY_STRIDE + id`, the dense encoding both planes agree on. */
  key(tag: number, id: number): number;
  readonly KEY_STRIDE: number;
  /** Run `body` inside one transaction, rolling back on throw. */
  with_txn<Result>(db: SqliteDb, body: () => Promise<Result>): Promise<Result>;
  create_schema(db: SqliteDb, ns: IGraphNs): Promise<void>;
  insert_rows(
    db: SqliteDb,
    ns: IGraphNs,
    rows: ReadonlyArray<readonly [number, number, number]>,
  ): Promise<void>;
  insert_deps(
    db: SqliteDb,
    ns: IGraphNs,
    edges: ReadonlyArray<readonly [number, number, number, number]>,
  ): Promise<void>;
  /** Forward add: propagate aliveness from `seeds`. Returns rounds. */
  assert(db: SqliteDb, ns: IGraphNs, seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
  /** Counting retraction. Fast, correct on ACYCLIC support graphs only. */
  retract(db: SqliteDb, ns: IGraphNs, seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
  /** Counting retraction with an on-disk SCC-scoped nested fixpoint. Cycle-safe. */
  retract_scc(db: SqliteDb, ns: IGraphNs, seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
  /** Delete-and-Rederive, driver round loop. Cycle-safe. */
  retract_dred(db: SqliteDb, ns: IGraphNs, seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
  /** Delete-and-Rederive as two recursive CTEs. Same answer as retract_dred. */
  retract_dred_cte(db: SqliteDb, ns: IGraphNs, seeds: ReadonlyArray<readonly [number, number]>): Promise<number>;
}

// ---- reach (src/engine/engine.ts) ------------------------------------------

/**
 * Read-only structure queries over `cx_dep`. Every function has a resident pure-JS oracle in
 * tests/engine/golden.test.ts and must agree with it byte-for-byte.
 */
export interface IReachApi {
  /** Forward transitive closure from `start` (strict; includes start iff its SCC is cyclic). */
  reaches_from(db: SqliteDb, ns: IGraphNs, start: number): Promise<number[]>;
  /** Reverse closure: everything that reaches `target`. */
  reached_by(db: SqliteDb, ns: IGraphNs, target: number): Promise<number[]>;
  multi_source_walk(
    db: SqliteDb,
    ns: IGraphNs,
    starts: ReadonlyArray<readonly [number, number, number]>,
    halt: ReadonlyArray<number> | null,
    depth_cap: number | null,
  ): Promise<[number, number, number][]>;
  multi_source_halt_bfs(
    db: SqliteDb,
    ns: IGraphNs,
    starts: ReadonlyArray<readonly [number, number]>,
    halt: ReadonlyArray<number>,
  ): Promise<[number, number][]>;
  scc_labels(db: SqliteDb, ns: IGraphNs): Promise<[number, number][]>;
  build_condensed(db: SqliteDb, ns: IGraphNs): Promise<Condensed>;
  count_pairs(db: SqliteDb, ns: IGraphNs): Promise<bigint>;
}

// ---- reconcile (src/engine/engine.ts) --------------------------------------

/**
 * The CONTROL plane as free functions: salsa-in-SQL over `rx_memo`/`rx_dep`. Digests decide
 * staleness; `verify` returning false is the early cutoff.
 */
export interface IReconcileApi {
  create_schema(db: SqliteDb, ns: IGraphNs): Promise<void>;
  seed(
    db: SqliteDb,
    ns: IGraphNs,
    id: number,
    digest: bigint,
    deps: ReadonlyArray<number>,
    rev: number,
  ): Promise<void>;
  mark_changed(db: SqliteDb, ns: IGraphNs, ids: ReadonlyArray<number>, rev: number): Promise<void>;
  /** The stale FRONTIER (one hop), not the whole cone. */
  dirty(db: SqliteDb, ns: IGraphNs): Promise<number[]>;
  /** Record a recomputed digest; returns whether it MOVED. */
  verify(db: SqliteDb, ns: IGraphNs, id: number, new_digest: bigint, rev: number): Promise<boolean>;
  /** Ascending-id sweep over the dirty cone, calling `recompute` per cell. */
  propagate(
    db: SqliteDb,
    ns: IGraphNs,
    seeds: ReadonlyArray<number>,
    rev: number,
    recompute: (id: number, dep_digests: bigint[]) => bigint,
  ): Promise<number>;
  answer(db: SqliteDb, ns: IGraphNs): Promise<bigint>;
}

// ---- rel tables (src/engine/spine.ts, namespace rels) ----------------------

/** The OPEN core: mint rel tables at runtime. Every one is WITHOUT ROWID with a composite
 *  PK over its key columns (set semantics, no duplicate autoindex). */
export interface IRelTableApi {
  rel_col_int(name: string): RelCol;
  rel_col_int_null(name: string): RelCol;
  rel_col_text(name: string): RelCol;
  create_rel_table(
    db: SqliteDb,
    name: string,
    cols: ReadonlyArray<RelCol>,
    pk: ReadonlyArray<string>,
  ): Promise<void>;
  drop_rel_table(db: SqliteDb, name: string): Promise<void>;
}

// ---- ingest (src/engine/ingest.ts) -----------------------------------------

/** The load path: a JSONL stream into both planes, batched. Takes BOTH stores, since a load
 *  writes spine rows through `IStore` and reconcile cells through `IRelStore`. */
export type IngestJsonl = (
  store: IStore,
  rels: IRelStore,
  lines: AsyncIterable<string>,
  rev: number,
) => Promise<IngestReport>;

// ─────────────────────────────────────────────────────────────────────────────
// Static-side proof helper.
//
// `implements` covers the instance side of a class and cannot see statics, and
// nothing at all binds a free function to a declared type. Each implementation
// file exports one `AssertTrue<...>` alias per class and per surface above, so a
// signature that drifts from this header fails the typecheck rather than going
// quietly out of date. The `false` branch is what makes it bite: `never extends
// true` is true, so a `never` branch would prove nothing.
// ─────────────────────────────────────────────────────────────────────────────

export type AssertTrue<Holds extends true> = Holds;
