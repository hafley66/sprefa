/**
 * lib.ts — the v6 storage spine, ported from src/lib.rs. The one object that speaks the
 * connection; callers get ids/rows, never a SeaORM type or SQL text. Writes are batched
 * (the N+1 law) and FK-ordered.
 *
 * ORM seam (src/lib.rs uses sea-orm async): TS uses ONE ASYNC `@libsql/client` connection.
 * `async fn` -> async `function`; `Result<T,DbErr>` -> throw on error; the Rust txn scope
 * maps to `client.batch([...], "write")` for a fixed write list. The spine Store methods
 * mirror lib.rs; RelStore delegates to engine.{cascade,reconcile}. GraphNs + Interner are
 * pure data structures, ported verbatim.
 */

import { createClient } from "@libsql/client";
import { cascade, reconcile, stmt_counter } from "./engine.ts";
import { OPEN_PRAGMAS, create_all_tables } from "./spine.ts";
import type {
  AssertTrue,
  EdgeRow,
  GraphNsStatics,
  GraphNs as GraphNsContract,
  InternerStatics,
  Interner as InternerContract,
  NodeRow,
  RelStoreStatics,
  RelStore as RelStoreContract,
  SpanRow,
  SqliteDb,
  StoreStatics,
  Store as StoreContract,
} from "./types.ts";

/** Declared in ./types.ts (the header). Re-exported so every existing
 *  `from "./lib.ts"` importer keeps working unchanged. */
export type { EdgeRow, NodeRow, SpanRow, SqliteDb };

/** The one constructor for that connection: every caller (dl included) opens SQLite
 *  through this alias, so `@libsql/client` is imported at runtime by this file alone
 *  (the header names the `Client` TYPE, which erases at build). `url` is passed
 *  through verbatim (`file:/abs/path.sqlite` or `:memory:`). */
export function open_db(url: string): SqliteDb {
  return createClient({ url });
}

/** Widest table is `node` at 7 columns; 100 rows/statement keeps bound params under 999. */
const CHUNK_ROWS = 100;

// =============================================================================
// relstore — RelStore + GraphNs + stamp (the generic incremental relation store)
// =============================================================================
export namespace relstore {
//! RelStore — the generic incremental relation store over DENSE `(rel, row)` ids
//! (E1: `key = rel*KEY_STRIDE + row`, a rowid-clustered table). One store holds ANY number
//! of relations. Two planes, both generic: FACT (Z-set) + CONTROL (salsa-in-sql).
//!
//! The `cx_*` / `rx_*` tables are the default on-disk impl; callers speak in `(rel, row)`.

export import key = cascade.key;
export import KEY_STRIDE = cascade.KEY_STRIDE;

/**
 * The namespace for one graph store: every persistent table, index, and TEMP working-table
 * name, built from a prefix. `GraphNs.default()` (empty prefix) is the live `cx_`/`rx_` set;
 * `GraphNs.new("b_")` is an independent store in the same db.
 *
 * PREFIX, not schema-qualify, is the namespace mechanism: SQLite TEMP working tables live in
 * temp. and CANNOT be qualified to an ATTACH'd schema, so prefix is the only namespace that
 * covers the working set.
 */
export class GraphNs implements GraphNsContract {
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
export async function stamp(db: SqliteDb, ns: GraphNs): Promise<void> {
  await db.executeMultiple(OPEN_PRAGMAS);
  await cascade.create_schema(db, ns);
  await reconcile.create_schema(db, ns);
}

/** A handle on one namespaced graph store: a connection + its GraphNs. */
export class RelStore implements RelStoreContract {
  constructor(private readonly _db: SqliteDb, private readonly _ns: GraphNs) {}

  /** Open (or create) a store at `db` stamped for namespace `ns`. */
  static async attach_with(db: SqliteDb, ns: GraphNs): Promise<RelStore> {
    await stamp(db, ns);
    return new RelStore(db, ns);
  }

  /** Open (or create) a store at `db` with the default namespace (cx_ + rx_). */
  static async attach(db: SqliteDb): Promise<RelStore> {
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
  async add_rows(rows: ReadonlyArray<readonly [number, number, number]>): Promise<void> {
    await cascade.insert_rows(this._db, this._ns, rows);
  }
  /** Insert dependency edges `(parent_rel, parent_row, child_rel, child_row)`. */
  async add_deps(edges: ReadonlyArray<readonly [number, number, number, number]>): Promise<void> {
    await cascade.insert_deps(this._db, this._ns, edges);
  }
  /** Forward add: propagate aliveness from `seeds`. Returns rounds. */
  async assert(seeds: ReadonlyArray<readonly [number, number]>): Promise<number> {
    return cascade.assert(this._db, this._ns, seeds);
  }
  /** Counting retraction (fast, correct on ACYCLIC support graphs). Returns rounds. */
  async retract(seeds: ReadonlyArray<readonly [number, number]>): Promise<number> {
    return cascade.retract(this._db, this._ns, seeds);
  }
  /** Counting retraction with an on-disk SCC-scoped nested fixpoint. Returns rounds. */
  async retract_scc(seeds: ReadonlyArray<readonly [number, number]>): Promise<number> {
    return cascade.retract_scc(this._db, this._ns, seeds);
  }
  /** Cycle-safe retraction (Delete-and-Rederive), round loop. Returns rounds. */
  async retract_dred(seeds: ReadonlyArray<readonly [number, number]>): Promise<number> {
    return cascade.retract_dred(this._db, this._ns, seeds);
  }
  /** Cycle-safe retraction as two recursive CTEs. Same result as retract_dred. */
  async retract_dred_cte(seeds: ReadonlyArray<readonly [number, number]>): Promise<number> {
    return cascade.retract_dred_cte(this._db, this._ns, seeds);
  }

  /** Count live rows (weight > 0) across all relations. */
  async alive(): Promise<number> {
    stmt_counter.incr();
    const res = await this._db.execute(`SELECT count(*) FROM ${this._ns.row} WHERE weight>0`);
    return Number(res.rows[0]?.[0] ?? 0);
  }

  /** The live-row survivor SET as sorted encoded keys (`key = rel*KEY_STRIDE + row`). */
  async alive_keys(): Promise<number[]> {
    stmt_counter.incr();
    const res = await this._db.execute(`SELECT key FROM ${this._ns.row} WHERE weight>0 ORDER BY key`);
    return res.rows.map((r) => Number(r[0]));
  }

  // ---- CONTROL plane (salsa-in-sql over (rel,row) memos) --------------------

  /** Seed a rel's memo (its output digest + the deps it read), at revision `rev`. */
  async seed_memo(
    rel: number,
    row: number,
    digest: bigint,
    deps: ReadonlyArray<readonly [number, number]>,
    rev: number,
  ): Promise<void> {
    const dep_keys = deps.map(([r, w]) => cascade.key(r, w));
    await reconcile.seed(this._db, this._ns, cascade.key(rel, row), digest, dep_keys, rev);
  }
  /** Bump changed_at for `cells` at `rev` (an input's digest moved). */
  async mark_changed(cells: ReadonlyArray<readonly [number, number]>, rev: number): Promise<void> {
    const ks = cells.map(([r, w]) => cascade.key(r, w));
    await reconcile.mark_changed(this._db, this._ns, ks, rev);
  }
  /** The stale frontier as `(rel, row)` pairs. */
  async dirty(): Promise<[number, number][]> {
    const ids = await reconcile.dirty(this._db, this._ns);
    return ids.map((k) => [Math.trunc(k / cascade.KEY_STRIDE), k % cascade.KEY_STRIDE]);
  }
  /** Record a recomputed rel's digest; returns whether it moved (early cutoff). */
  async verify(rel: number, row: number, digest: bigint, rev: number): Promise<boolean> {
    return reconcile.verify(this._db, this._ns, cascade.key(rel, row), digest, rev);
  }
}
}

// ---- static-side proofs (./types.ts) -----------------------------------------
// `implements` above covers each class's INSTANCE side only. These four aliases
// check the static side (constructors, factories) against the header, so a
// signature that drifts fails the typecheck instead of going quietly stale.
export type GraphNsStaticsHold = AssertTrue<typeof relstore.GraphNs extends GraphNsStatics ? true : false>;
export type RelStoreStaticsHold = AssertTrue<typeof relstore.RelStore extends RelStoreStatics ? true : false>;
export type InternerStaticsHold = AssertTrue<typeof strings.Interner extends InternerStatics ? true : false>;
export type StoreStaticsHold = AssertTrue<typeof Store extends StoreStatics ? true : false>;

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
export class Interner implements InternerContract {
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
    const got = this.intern(content);
    if (got !== id) {
      throw new Error(
        `interner reload out of order: got id ${got} for ${JSON.stringify(content)}, expected ${id}`,
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
export class Store implements StoreContract {
  private readonly interner: strings.Interner;

  private constructor(private readonly _db: SqliteDb) {
    this.interner = new strings.Interner();
  }

  /** Open a store, apply pragmas, create the spine, hydrate the interner mirror. */
  static async open(db: SqliteDb): Promise<Store> {
    await db.executeMultiple(OPEN_PRAGMAS);
    await create_all_tables(db);
    const store = new Store(db);
    const res = await db.execute("SELECT string_id, content FROM strings ORDER BY string_id ASC");
    for (const row of res.rows) {
      store.interner.load_row(Number(row.string_id), row.content as string);
    }
    return store;
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

  /** Persist every string interned since the last flush in ONE batched insert. */
  async flush_strings(): Promise<number> {
    const dirty = this.interner.take_dirty();
    if (dirty.length === 0) return 0;
    const n = dirty.length;
    for (let i = 0; i < dirty.length; i += CHUNK_ROWS) {
      const chunk = dirty.slice(i, i + CHUNK_ROWS);
      const vals = chunk.map(([id, content]) => `(${id},${sql_str(content)})`).join(",");
      stmt_counter.incr();
      await this._db.executeMultiple(
        `INSERT INTO strings(string_id,content) VALUES ${vals} ON CONFLICT(string_id) DO NOTHING`,
      );
    }
    return n;
  }

  // ---- dimensions ---------------------------------------------------------

  async repo_upsert(slug: string, root: string, url: string): Promise<number> {
    stmt_counter.incr();
    const foundRes = await this._db.execute({ sql: "SELECT repo_id FROM repos WHERE slug = ?", args: [slug] });
    const found = foundRes.rows[0] as { repo_id: number } | undefined;
    if (found) return Number(found.repo_id);
    stmt_counter.incr();
    const res = await this._db.execute({
      sql: "INSERT INTO repos (slug, root, url) VALUES (?, ?, ?)",
      args: [slug, root, url],
    });
    return Number(res.lastInsertRowid);
  }

  async root_insert(repo_id: number, path_string_id: number): Promise<number> {
    stmt_counter.incr();
    const res = await this._db.execute({
      sql: "INSERT INTO roots (repo_id, path_string_id) VALUES (?, ?)",
      args: [repo_id, path_string_id],
    });
    return Number(res.lastInsertRowid);
  }

  /** A committed rev (shared across roots that have this sha). Find-or-insert by (repo_id, git_sha). */
  async rev_committed(repo_id: number, git_sha: Uint8Array): Promise<number> {
    stmt_counter.incr();
    const foundRes = await this._db.execute({
      sql: "SELECT rev_id FROM repo_revs WHERE repo_id = ? AND git_sha = ?",
      args: [repo_id, Buffer.from(git_sha)],
    });
    const found = foundRes.rows[0] as { rev_id: number } | undefined;
    if (found) return Number(found.rev_id);
    stmt_counter.incr();
    const res = await this._db.execute({
      sql: "INSERT INTO repo_revs (repo_id, kind, git_sha, root_id, base_rev_id) VALUES (?, 0, ?, NULL, NULL)",
      args: [repo_id, Buffer.from(git_sha)],
    });
    return Number(res.lastInsertRowid);
  }

  /** The WORK rev of a root (its uncommitted working tree). One per root. */
  async rev_work(repo_id: number, root_id: number, base_rev_id: number): Promise<number> {
    stmt_counter.incr();
    const foundRes = await this._db.execute({
      sql: "SELECT rev_id FROM repo_revs WHERE root_id = ? AND kind = 1",
      args: [root_id],
    });
    const found = foundRes.rows[0] as { rev_id: number } | undefined;
    if (found) return Number(found.rev_id);
    stmt_counter.incr();
    const res = await this._db.execute({
      sql: "INSERT INTO repo_revs (repo_id, kind, git_sha, root_id, base_rev_id) VALUES (?, 1, NULL, ?, ?)",
      args: [repo_id, root_id, base_rev_id],
    });
    return Number(res.lastInsertRowid);
  }

  // ---- files (content, dedup by hash) ------------------------------------

  async files_insert_batch(rows: ReadonlyArray<readonly [Uint8Array, number, number]>): Promise<void> {
    for (let i = 0; i < rows.length; i += CHUNK_ROWS) {
      const chunk = rows.slice(i, i + CHUNK_ROWS);
      const placeholders = chunk.map(() => "(?,?,?)").join(",");
      const params = chunk.flatMap(([hash, size, lines]) => [Buffer.from(hash), size, lines]);
      stmt_counter.incr();
      await this._db.execute({
        sql: `INSERT INTO files(content_hash,size,lines) VALUES ${placeholders} ON CONFLICT(content_hash) DO NOTHING`,
        args: params,
      });
    }
  }

  async file_id_of(content_hash: Uint8Array): Promise<number | null> {
    stmt_counter.incr();
    const res = await this._db.execute({
      sql: "SELECT file_id FROM files WHERE content_hash = ?",
      args: [Buffer.from(content_hash)],
    });
    const found = res.rows[0] as { file_id: number } | undefined;
    return found ? Number(found.file_id) : null;
  }

  /** Batch `content_hash -> file_id` resolution: ONE query per CHUNK_ROWS hashes. */
  async file_ids_by_hashes(hashes: ReadonlyArray<Uint8Array>): Promise<Map<string, number>> {
    const map = new Map<string, number>();
    for (let i = 0; i < hashes.length; i += CHUNK_ROWS) {
      const chunk = hashes.slice(i, i + CHUNK_ROWS);
      const placeholders = chunk.map(() => "?").join(",");
      stmt_counter.incr();
      const res = await this._db.execute({
        sql: `SELECT content_hash, file_id FROM files WHERE content_hash IN (${placeholders})`,
        args: chunk.map((h) => Buffer.from(h)),
      });
      for (const row of res.rows) {
        const content_hash = new Uint8Array(row.content_hash as ArrayBuffer);
        map.set(key_of(content_hash), Number(row.file_id));
      }
    }
    return map;
  }

  /** Paths in a WORK rev whose content differs from its base HEAD. */
  async unstaged_path_ids(work_rev: number): Promise<number[]> {
    stmt_counter.incr();
    const res = await this._db.execute(
      `SELECT w.path_string_id AS p \
             FROM revs_files w \
             JOIN repo_revs r ON r.rev_id = w.rev_id \
             LEFT JOIN revs_files h \
               ON h.rev_id = r.base_rev_id AND h.path_string_id = w.path_string_id \
             WHERE w.rev_id = ${work_rev} \
               AND (h.file_id IS NULL OR h.file_id <> w.file_id)`,
    );
    return res.rows.map((r) => Number(r[0]));
  }

  // ---- junction: place content at (rev, path) ----------------------------

  async place_files_batch(rows: ReadonlyArray<readonly [number, number, number]>): Promise<void> {
    for (let i = 0; i < rows.length; i += CHUNK_ROWS) {
      const chunk = rows.slice(i, i + CHUNK_ROWS);
      const vals = chunk.map(([a, b, c]) => `(${a},${b},${c})`).join(",");
      stmt_counter.incr();
      await this._db.executeMultiple(
        `INSERT INTO revs_files(rev_id,path_string_id,file_id) VALUES ${vals} ON CONFLICT(rev_id,path_string_id) DO NOTHING`,
      );
    }
  }

  // ---- unified graph -----------------------------------------------------

  async nodes_insert_batch(rows: ReadonlyArray<NodeRow>): Promise<void> {
    for (let i = 0; i < rows.length; i += CHUNK_ROWS) {
      const chunk = rows.slice(i, i + CHUNK_ROWS);
      const vals = chunk
        .map(
          (r) =>
            `(${r.node_id},${r.family},${r.file_id},${r.byte_start},${r.byte_len},${r.kind},${r.name_id === null ? "NULL" : r.name_id})`,
        )
        .join(",");
      stmt_counter.incr();
      await this._db.executeMultiple(
        `INSERT INTO node(node_id,family,file_id,byte_start,byte_len,kind,name_id) VALUES ${vals} ON CONFLICT(node_id) DO NOTHING`,
      );
    }
  }

  async edges_insert_batch(rows: ReadonlyArray<EdgeRow>): Promise<void> {
    for (let i = 0; i < rows.length; i += CHUNK_ROWS) {
      const chunk = rows.slice(i, i + CHUNK_ROWS);
      const vals = chunk.map((r) => `(${r.family},${r.src_id},${r.dst_id},${r.kind})`).join(",");
      stmt_counter.incr();
      await this._db.executeMultiple(
        `INSERT INTO edge(family,src_id,dst_id,kind) VALUES ${vals} ON CONFLICT(family,src_id,dst_id,kind) DO NOTHING`,
      );
    }
  }

  async spans_insert_batch(rows: ReadonlyArray<SpanRow>): Promise<void> {
    for (let i = 0; i < rows.length; i += CHUNK_ROWS) {
      const chunk = rows.slice(i, i + CHUNK_ROWS);
      const vals = chunk
        .map((r) => `(${r.file_id},${r.start},${r.end},${r.string_id === null ? "NULL" : r.string_id})`)
        .join(",");
      stmt_counter.incr();
      await this._db.executeMultiple(
        `INSERT INTO file_bytes(file_id,start,end,string_id) VALUES ${vals} ON CONFLICT(file_id,start,end) DO NOTHING`,
      );
    }
  }
}

/** SQL-string-escape a text literal (the store inlines text into batched inserts). */
function sql_str(s: string): string {
  return `'${s.replace(/'/g, "''")}'`;
}

/** Stable Map key for a 16-byte content hash. */
function key_of(hash: Uint8Array): string {
  let s = "";
  for (const b of hash) s += b.toString(16).padStart(2, "0");
  return s;
}
