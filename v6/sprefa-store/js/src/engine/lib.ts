/**
 * lib.ts — the v6 storage spine, ported from src/lib.rs. The one object that speaks the
 * connection; callers get ids/rows, never a SeaORM type or SQL text. Writes are batched
 * (the N+1 law) and FK-ordered.
 *
 * ORM seam (src/lib.rs uses sea-orm async): TS uses ONE `@libsql/client` connection, reached
 * only through `SqlRunner`, so every method here returns a cold `Observable`. `Result<T,DbErr>`
 * -> error notification; the Rust txn scope maps to `SqlRunner.batch` for a fixed write list.
 * The spine Store methods mirror lib.rs; RelStore delegates to engine.{cascade,reconcile}.
 * GraphNs + Interner are pure data structures, ported verbatim.
 */

import { type Observable, concat, concatMap, defer, map, of, reduce } from "rxjs";
import { createClient } from "@libsql/client";
import { cascade, reconcile } from "./engine.ts";
import { SqlRunner } from "./sqlRunner.ts";
import { OPEN_PRAGMAS, create_all_tables } from "./spine.ts";
import type {
  AssertTrue,
  EdgeRow,
  IGraphNs,
  IGraphNsStatics,
  IInterner,
  IInternerStatics,
  IRelStore,
  IRelStoreStatics,
  IStore,
  IStoreStatics,
  OpenDb,
  NodeRow,
  QueryResult,
  SpanRow,
  SqliteDb,
  SqlStatement,
  Stamp,
} from "./types.ts";

/** Declared in ./types.ts (the header). Re-exported so every existing
 *  `from "./lib.ts"` importer keeps working unchanged. */
export type { EdgeRow, NodeRow, SpanRow, SqliteDb };

/** The one constructor for that connection: every caller (dl included) opens SQLite
 *  through this alias, so `@libsql/client` is imported at runtime by this file alone
 *  (the header names the `Client` TYPE, which erases at build). `url` is passed
 *  through verbatim (`file:/abs/path.sqlite` or `:memory:`).
 *
 *  intMode stays the driver default ("number"): emitted modules project driver
 *  rows RAW at several sites (ordered-occurrence arms, edge projections) and a
 *  global "bigint" leaks BigInt into JSON.stringify there (3 fixtures threw
 *  "Do not know how to serialize a BigInt"). Wide ints are refused at the
 *  value boundary (compile.pl world-shape check + oracle + rowValueFromSql);
 *  flipping this to "bigint" requires normalizing at the SqlRunner seam
 *  first so no raw reader ever sees one. */
export function open_db(url: string): SqliteDb {
  return createClient({ url });
}

/** Widest table is `node` at 7 columns; 100 rows/statement keeps bound params under 999. */
const CHUNK_ROWS = 100;

function chunk_list<Item>(items: readonly Item[], size: number): Item[][] {
  const out: Item[][] = [];
  for (let start = 0; start < items.length; start += size) out.push(items.slice(start, start + size));
  return out;
}

// =============================================================================
// relstore — RelStore + GraphNs + stamp (the generic incremental relation store)
// =============================================================================
export namespace relstore {
//! RelStore — the generic incremental relation store over DENSE `(rel, row)` ids
//! (E1: `key = rel*KEY_STRIDE + row`, a rowid-clustered table). One store holds ANY number
//! of relations. Two planes, both generic: FACT (Z-set) + CONTROL (salsa-in-sql).
//!
//! The `cx_*` / `rx_*` tables are the default on-disk impl; callers speak in `(rel, row)`.

/**
 * The namespace for one graph store: every persistent table, index, and TEMP working-table
 * name, built from a prefix. `GraphNs.default()` (empty prefix) is the live `cx_`/`rx_` set;
 * `GraphNs.new("b_")` is an independent store in the same db.
 *
 * PREFIX, not schema-qualify, is the namespace mechanism: SQLite TEMP working tables live in
 * temp. and CANNOT be qualified to an ATTACH'd schema, so prefix is the only namespace that
 * covers the working set.
 */
export class GraphNs implements IGraphNs {
  constructor(
    public readonly row: string,
    public readonly dep: string,
    public readonly memo: string,
    public readonly rdep: string,
    public readonly ix_dep_child: string,
    public readonly ix_rdep_read: string,
    public readonly frontier: string,
    public readonly next: string,
    public readonly hits: string,
    public readonly cone: string,
    public readonly scc_scope: string,
    public readonly scc_frontier: string,
    public readonly scc_next: string,
    public readonly scc_live: string,
  ) {}

  /** `prefix` is prepended verbatim to every base name; pass `"b_"` for a namespace, `""` for default. */
  static new(prefix: string): GraphNs {
    return new GraphNs(
      `${prefix}cx_row`,
      `${prefix}cx_dep`,
      `${prefix}rx_memo`,
      `${prefix}rx_dep`,
      `${prefix}ix_cx_dep_child`,
      `${prefix}ix_rx_read`,
      `${prefix}cx_frontier`,
      `${prefix}cx_next`,
      `${prefix}cx_hits`,
      `${prefix}cx_cone`,
      `${prefix}cx_scc_scope`,
      `${prefix}cx_scc_frontier`,
      `${prefix}cx_scc_next`,
      `${prefix}cx_scc_live`,
    );
  }

  /** Empty prefix = the live cx_/rx_ set (the status quo). */
  static default(): GraphNs {
    return GraphNs.new("");
  }
}

/**
 * Stamp the split two-plane schema (cascade `cx_*` + reconcile `rx_*` + TEMP working set +
 * indexes) under the namespace `ns` onto `db` (pragmas first). `GraphNs.default()`
 * reproduces the live `cx_`/`rx_` set byte-for-byte; a custom prefix yields an independent
 * store in the same db.
 */
export function stamp(db: SqliteDb, ns: GraphNs): Observable<void> {
  return SqlRunner.executeMultiple(db, OPEN_PRAGMAS).pipe(
    concatMap(() => cascade.create_schema(db, ns)),
    concatMap(() => reconcile.create_schema(db, ns)),
  );
}

/** A handle on one namespaced graph store: a connection + its GraphNs. */
export class RelStore implements IRelStore {
  constructor(private readonly _db: SqliteDb, private readonly _ns: GraphNs) {}

  /** Open (or create) a store at `db` stamped for namespace `ns`. */
  static attach_with(db: SqliteDb, ns: GraphNs): Observable<RelStore> {
    return stamp(db, ns).pipe(map(() => new RelStore(db, ns)));
  }

  /** Open (or create) a store at `db` with the default namespace (cx_ + rx_). */
  static attach(db: SqliteDb): Observable<RelStore> {
    return RelStore.attach_with(db, GraphNs.default());
  }

  conn(): SqliteDb {
    return this._db;
  }

  /** The namespace this store's `cx_*`/`rx_*` tables live under. */
  ns(): GraphNs {
    return this._ns;
  }

  // ---- FACT plane (generic Z-set over (rel,row)) ----------------------------

  /** Insert `(rel, row, weight)` tuples. */
  add_rows(rows: ReadonlyArray<readonly [number, number, number]>): Observable<QueryResult[]> {
    return cascade.insert_rows(this._db, this._ns, rows);
  }
  /** Insert dependency edges `(parent_rel, parent_row, child_rel, child_row)`. */
  add_deps(edges: ReadonlyArray<readonly [number, number, number, number]>): Observable<QueryResult[]> {
    return cascade.insert_deps(this._db, this._ns, edges);
  }
  /** Forward add: propagate aliveness from `seeds`. Returns rounds. */
  assert(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return cascade.assert(this._db, this._ns, seeds);
  }
  /** Counting retraction (fast, correct on ACYCLIC ref-count graphs). Returns rounds. */
  retract(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return cascade.retract(this._db, this._ns, seeds);
  }
  /** Counting retraction with an on-disk SCC-scoped nested fixpoint. Returns rounds. */
  retract_scc(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return cascade.retract_scc(this._db, this._ns, seeds);
  }
  /** Cycle-safe retraction (Delete-and-Rederive), round loop. Returns rounds. */
  retract_dred(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return cascade.retract_dred(this._db, this._ns, seeds);
  }
  /** Cycle-safe retraction as two recursive CTEs. Same result as retract_dred. */
  retract_dred_cte(seeds: ReadonlyArray<readonly [number, number]>): Observable<number> {
    return cascade.retract_dred_cte(this._db, this._ns, seeds);
  }

  /** Count live rows (weight > 0) across all relations. */
  alive(): Observable<number> {
    return SqlRunner.scalar(this._db, `SELECT count(*) FROM ${this._ns.row} WHERE weight>0`);
  }

  /** The live-row survivor SET as sorted encoded keys (`key = rel*KEY_STRIDE + row`). */
  alive_keys(): Observable<number[]> {
    return SqlRunner.execute(this._db, `SELECT key FROM ${this._ns.row} WHERE weight>0 ORDER BY key`).pipe(
      map((queryResult) => queryResult.rows.map((row) => Number(row[0]))),
    );
  }

  // ---- CONTROL plane (salsa-in-sql over (rel,row) memos) --------------------

  /** Seed a rel's memo (its output digest + the deps it read), at revision `rev`. */
  seed_memo(
    rel: number,
    row: number,
    digest: bigint,
    deps: ReadonlyArray<readonly [number, number]>,
    rev: number,
  ): Observable<QueryResult[]> {
    const dep_keys = deps.map(([relId, rowIndex]) => cascade.key(relId, rowIndex));
    return reconcile.seed(this._db, this._ns, cascade.key(rel, row), digest, dep_keys, rev);
  }
  /** Bump changed_at for `cells` at `rev` (an input's digest moved). */
  mark_changed(cells: ReadonlyArray<readonly [number, number]>, rev: number): Observable<QueryResult[]> {
    const keys = cells.map(([relId, rowIndex]) => cascade.key(relId, rowIndex));
    return reconcile.mark_changed(this._db, this._ns, keys, rev);
  }
  /** The stale frontier as `(rel, row)` pairs. */
  dirty(): Observable<[number, number][]> {
    return reconcile.dirty(this._db, this._ns).pipe(
      map((dirtyIds) =>
        dirtyIds.map(
          (encodedKey): [number, number] => [
            Math.trunc(encodedKey / cascade.KEY_STRIDE),
            encodedKey % cascade.KEY_STRIDE,
          ],
        ),
      ),
    );
  }
  /** Record a recomputed rel's digest; returns whether it moved (early cutoff). */
  verify(rel: number, row: number, digest: bigint, rev: number): Observable<boolean> {
    return reconcile.verify(this._db, this._ns, cascade.key(rel, row), digest, rev);
  }
}
}

// ---- static-side proofs (./types.ts) -----------------------------------------
// `implements` above covers each class's INSTANCE side only. These four aliases
// check the static side (constructors, factories) against the header, so a
// signature that drifts fails the typecheck instead of going quietly stale.
export type GraphNsStaticsHold = AssertTrue<typeof relstore.GraphNs extends IGraphNsStatics ? true : false>;
export type RelStoreStaticsHold = AssertTrue<typeof relstore.RelStore extends IRelStoreStatics ? true : false>;
export type InternerStaticsHold = AssertTrue<typeof strings.Interner extends IInternerStatics ? true : false>;
export type StoreStaticsHold = AssertTrue<typeof Store extends IStoreStatics ? true : false>;

// ---- dataflow proofs (./types.ts) -------------------------------------------
export type OpenDbHolds = AssertTrue<typeof open_db extends OpenDb ? true : false>;
export type StampHolds = AssertTrue<typeof relstore.stamp extends Stamp ? true : false>;

// ---- module-level re-exports (so `import { GraphNs, RelStore, stamp } from "./lib.ts"` works) ----
export const GraphNs = relstore.GraphNs;
export type GraphNs = relstore.GraphNs;
export const RelStore = relstore.RelStore;
export type RelStore = relstore.RelStore;
export const stamp = relstore.stamp;
export const key = cascade.key;
export const KEY_STRIDE = cascade.KEY_STRIDE;

// =============================================================================
// strings — resident interning (the v5 string subsystem, replaced by a Map arena)
// =============================================================================
export namespace strings {
//! Resident string interning. THE v5 pain point (hashed ids, bespoke allocators,
//! collision guards, rev salting), replaced by a dense sequential arena. `string_id` is
//! the dense index (0-based, contiguous). The `strings` table is the durable MIRROR of
//! the arena, never the source. New interns queue in `dirty` for ONE batched insert.

/** The resident interner. Owns the string arena and assigns dense ids. */
export class Interner implements IInterner {
  private readonly rodeo = new Map<string, number>();
  private readonly by_id: string[] = [];
  private dirty: [number, string][] = [];

  /** Intern `text`, returning its dense `string_id`. Queues `(id, text)` first time seen. */
  intern(text: string): number {
    const existing = this.rodeo.get(text);
    if (existing !== undefined) return existing;
    const id = this.by_id.length;
    this.rodeo.set(text, id);
    this.by_id.push(text);
    this.dirty.push([id, text]);
    return id;
  }

  /** `string_id -> text`, straight from the resident arena, no DB round-trip. */
  resolve(id: number): string | undefined {
    return this.by_id[id];
  }

  /**
   * Rebuild the arena from the durable mirror on open. Rows MUST arrive in ascending
   * `string_id` order so the reconstructed id equals the stored id — asserted.
   */
  load_row(id: number, content: string): void {
    const internedId = this.intern(content);
    if (internedId !== id) {
      throw new Error(
        `interner reload out of order: got id ${internedId} for ${JSON.stringify(content)}, expected ${id}`,
      );
    }
  }

  /** Drain the queued new interns for a batched `strings` insert. */
  take_dirty(): [number, string][] {
    const out = this.dirty;
    this.dirty = [];
    return out;
  }

  len(): number {
    return this.by_id.length;
  }

  is_empty(): boolean {
    return this.by_id.length === 0;
  }
}
}

// =============================================================================
// Store — the spine store (9 tables), speaks the connection, batched writes
// =============================================================================
/**
 * The one object that speaks the connection. Open applies pragmas + creates the spine,
 * hydrating the resident interner from the durable mirror. Writes are batched; ids/rows
 * are returned to callers, never a SeaORM type.
 */
export class Store implements IStore {
  private readonly interner: strings.Interner;

  private constructor(private readonly _db: SqliteDb) {
    this.interner = new strings.Interner();
  }

  /** Open a store, apply pragmas, create the spine, hydrate the interner mirror. */
  static open(db: SqliteDb): Observable<Store> {
    return SqlRunner.executeMultiple(db, OPEN_PRAGMAS).pipe(
      concatMap(() => create_all_tables(db)),
      concatMap(() => SqlRunner.execute(db, "SELECT string_id, content FROM strings ORDER BY string_id ASC")),
      map((mirror) => {
        const store = new Store(db);
        for (const row of mirror.rows) {
          store.interner.load_row(Number(row.string_id), row.content as string);
        }
        return store;
      }),
    );
  }

  db(): SqliteDb {
    return this._db;
  }

  // ---- strings: resident intern + batched durable flush --------------------

  intern(text: string): number {
    return this.interner.intern(text);
  }

  resolve(id: number): string | undefined {
    return this.interner.resolve(id);
  }

  /** Persist every string interned since the last flush in ONE batched insert.
   *  `defer` so the interner is drained at subscribe time, not at call time. */
  flush_strings(): Observable<number> {
    return defer(() => {
      const dirty = this.interner.take_dirty();
      const statements = chunk_list(dirty, CHUNK_ROWS).map((chunk) => {
        const vals = chunk.map(([id, content]) => `(${id},${sql_str(content)})`).join(",");
        return `INSERT INTO strings(string_id,content) VALUES ${vals} ON CONFLICT(string_id) DO NOTHING`;
      });
      return SqlRunner.batch(this._db, statements).pipe(map(() => dirty.length));
    });
  }

  // ---- dimensions ---------------------------------------------------------

  repo_upsert(slug: string, root: string, url: string): Observable<number> {
    return find_or_insert(
      this._db,
      { sql: "SELECT repo_id FROM repos WHERE slug = ?", args: [slug] },
      { sql: "INSERT INTO repos (slug, root, url) VALUES (?, ?, ?)", args: [slug, root, url] },
    );
  }

  root_insert(repo_id: number, path_string_id: number): Observable<number> {
    return SqlRunner.execute(this._db, {
      sql: "INSERT INTO roots (repo_id, path_string_id) VALUES (?, ?)",
      args: [repo_id, path_string_id],
    }).pipe(map((queryResult) => Number(queryResult.lastInsertRowid)));
  }

  /** A committed rev (shared across roots that have this sha). Find-or-insert by (repo_id, git_sha). */
  rev_committed(repo_id: number, git_sha: Uint8Array): Observable<number> {
    return find_or_insert(
      this._db,
      {
        sql: "SELECT rev_id FROM repo_revs WHERE repo_id = ? AND git_sha = ?",
        args: [repo_id, Buffer.from(git_sha)],
      },
      {
        sql: "INSERT INTO repo_revs (repo_id, kind, git_sha, root_id, base_rev_id) VALUES (?, 0, ?, NULL, NULL)",
        args: [repo_id, Buffer.from(git_sha)],
      },
    );
  }

  /** The WORK rev of a root (its uncommitted working tree). One per root. */
  rev_work(repo_id: number, root_id: number, base_rev_id: number): Observable<number> {
    return find_or_insert(
      this._db,
      { sql: "SELECT rev_id FROM repo_revs WHERE root_id = ? AND kind = 1", args: [root_id] },
      {
        sql: "INSERT INTO repo_revs (repo_id, kind, git_sha, root_id, base_rev_id) VALUES (?, 1, NULL, ?, ?)",
        args: [repo_id, root_id, base_rev_id],
      },
    );
  }

  // ---- files (content, dedup by hash) ------------------------------------

  files_insert_batch(rows: ReadonlyArray<readonly [Uint8Array, number, number]>): Observable<QueryResult[]> {
    const statements = chunk_list(rows, CHUNK_ROWS).map((chunk) => ({
      sql: `INSERT INTO files(content_hash,size,lines) VALUES ${chunk.map(() => "(?,?,?)").join(",")} ON CONFLICT(content_hash) DO NOTHING`,
      args: chunk.flatMap(([hash, size, lines]) => [Buffer.from(hash), size, lines]),
    }));
    return SqlRunner.batch(this._db, statements);
  }

  file_id_of(content_hash: Uint8Array): Observable<number | null> {
    return SqlRunner.execute(this._db, {
      sql: "SELECT file_id FROM files WHERE content_hash = ?",
      args: [Buffer.from(content_hash)],
    }).pipe(
      map((queryResult) => {
        const found = queryResult.rows[0];
        return found === undefined ? null : Number(found[0]);
      }),
    );
  }

  /** Batch `content_hash -> file_id` resolution: ONE query per CHUNK_ROWS hashes. */
  file_ids_by_hashes(hashes: ReadonlyArray<Uint8Array>): Observable<Map<string, number>> {
    return defer(() => {
      const queries = chunk_list(hashes, CHUNK_ROWS).map((chunk) =>
        SqlRunner.execute(this._db, {
          sql: `SELECT content_hash, file_id FROM files WHERE content_hash IN (${chunk.map(() => "?").join(",")})`,
          args: chunk.map((hash) => Buffer.from(hash)),
        }),
      );
      return concat(...queries).pipe(
        reduce((hashToFileId, queryResult) => {
          for (const row of queryResult.rows) {
            hashToFileId.set(hashHex(new Uint8Array(row.content_hash as ArrayBuffer)), Number(row.file_id));
          }
          return hashToFileId;
        }, new Map<string, number>()),
      );
    });
  }

  /** Paths in a WORK rev whose content differs from its base HEAD. */
  unstaged_path_ids(work_rev: number): Observable<number[]> {
    return SqlRunner.execute(
      this._db,
      `SELECT w.path_string_id AS p \
             FROM revs_files w \
             JOIN repo_revs r ON r.rev_id = w.rev_id \
             LEFT JOIN revs_files h \
               ON h.rev_id = r.base_rev_id AND h.path_string_id = w.path_string_id \
             WHERE w.rev_id = ${work_rev} \
               AND (h.file_id IS NULL OR h.file_id <> w.file_id)`,
    ).pipe(map((queryResult) => queryResult.rows.map((row) => Number(row[0]))));
  }

  // ---- junction: place content at (rev, path) ----------------------------

  place_files_batch(rows: ReadonlyArray<readonly [number, number, number]>): Observable<QueryResult[]> {
    const statements = chunk_list(rows, CHUNK_ROWS).map((chunk) => {
      const vals = chunk
        .map(([revisionId, pathStringId, fileId]) => `(${revisionId},${pathStringId},${fileId})`)
        .join(",");
      return `INSERT INTO revs_files(rev_id,path_string_id,file_id) VALUES ${vals} ON CONFLICT(rev_id,path_string_id) DO NOTHING`;
    });
    return SqlRunner.batch(this._db, statements);
  }

  // ---- unified graph -----------------------------------------------------

  nodes_insert_batch(rows: ReadonlyArray<NodeRow>): Observable<QueryResult[]> {
    const statements = chunk_list(rows, CHUNK_ROWS).map((chunk) => {
      const vals = chunk
        .map(
          (row) =>
            `(${row.node_id},${row.family},${row.file_id},${row.byte_start},${row.byte_len},${row.kind},${row.name_id === null ? "NULL" : row.name_id})`,
        )
        .join(",");
      return `INSERT INTO node(node_id,family,file_id,byte_start,byte_len,kind,name_id) VALUES ${vals} ON CONFLICT(node_id) DO NOTHING`;
    });
    return SqlRunner.batch(this._db, statements);
  }

  edges_insert_batch(rows: ReadonlyArray<EdgeRow>): Observable<QueryResult[]> {
    const statements = chunk_list(rows, CHUNK_ROWS).map((chunk) => {
      const vals = chunk.map((row) => `(${row.family},${row.src_id},${row.dst_id},${row.kind})`).join(",");
      return `INSERT INTO edge(family,src_id,dst_id,kind) VALUES ${vals} ON CONFLICT(family,src_id,dst_id,kind) DO NOTHING`;
    });
    return SqlRunner.batch(this._db, statements);
  }

  spans_insert_batch(rows: ReadonlyArray<SpanRow>): Observable<QueryResult[]> {
    const statements = chunk_list(rows, CHUNK_ROWS).map((chunk) => {
      const vals = chunk
        .map((row) => `(${row.file_id},${row.start},${row.end},${row.string_id === null ? "NULL" : row.string_id})`)
        .join(",");
      return `INSERT INTO file_bytes(file_id,start,end,string_id) VALUES ${vals} ON CONFLICT(file_id,start,end) DO NOTHING`;
    });
    return SqlRunner.batch(this._db, statements);
  }
}

/** The dimension-table shape: the existing surrogate key if `find` hits, else `insert`'s new
 *  rowid. `find` must select exactly the key column. */
function find_or_insert(db: SqliteDb, find: SqlStatement, insert: SqlStatement): Observable<number> {
  return SqlRunner.execute(db, find).pipe(
    concatMap((foundResult) => {
      const found = foundResult.rows[0];
      if (found !== undefined) return of(Number(found[0]));
      return SqlRunner.execute(db, insert).pipe(map((insertResult) => Number(insertResult.lastInsertRowid)));
    }),
  );
}

/** SQL-string-escape a text literal (the store inlines text into batched inserts). */
function sql_str(s: string): string {
  return `'${s.replace(/'/g, "''")}'`;
}

/** Stable Map key for a content hash: lowercase hex. */
export function hashHex(hash: Uint8Array): string {
  let hex = "";
  for (const byte of hash) hex += byte.toString(16).padStart(2, "0");
  return hex;
}
