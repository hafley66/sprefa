/**
 * engine.ts — the ONE semi-naive cascade: frontier -> one hop -> prune -> fixpoint.
 * Three prune modes: reconcile=digest(A) . retract/assert=weight(B) . reach/temporal=reached(C).
 * Ported character-for-character from src/engine.rs. SQLite is production; dd/salsa live
 * in oracle.ts (the from-scratch math only).
 *
 * ORM seam: Rust uses sea-orm/sqlx async (ConnectionTrait, DatabaseConnection, DbErr,
 * db.transaction(|txn| async {...})). TS uses ONE `@libsql/client` ASYNC connection
 * (`Client`, `intMode:"bigint"` so every SQLite integer round-trips as `bigint`, matching the
 * Rust i64/u64 digest semantics): `async fn` -> async `function`; `Result<T,DbErr>` -> throw
 * on error. The Rust txn scope maps to `client.batch([...], "write")` for a fixed,
 * upfront-known write-statement list — ONE atomic commit, same as `db.transaction(fn)()`.
 * `client.transaction()` is NOT used: its local-sqlite3 adapter hands the physical
 * connection to the returned `Transaction` object and lazily opens a FRESH connection for
 * the client's next plain call, which for a `:memory:` store (and for any TEMP table,
 * connection-private regardless of backing store) silently loses every table. The
 * round-loop bodies (the ones that read a count mid-loop to decide whether to keep going,
 * where the next statement depends on the previous read) instead run under a MANUAL
 * bracket: `with_txn` issues BEGIN IMMEDIATE / COMMIT / ROLLBACK through single-statement
 * `client.execute` calls on the one pinned connection — the Rust all-or-nothing crash
 * bracket, restored. Inside the bracket every cascade statement ALSO goes through
 * `client.execute` one at a time: the adapter's `executeMultiple` carries a
 * `finally { if (db.inTransaction) ROLLBACK }` guard (sqlite3.js:161-172) that would kill
 * an open bracket, so cascade's `exec` splits its multi-statement strings at top-level
 * `;` (safe: cascade SQL inlines only integers, never text literals). Statement text is
 * otherwise the Rust string unchanged.
 * `client.execute(sql)` reads scalars/rows, with every bigint column wrapped in
 * `Number(...)` at its number-use site (array indices, counts, comparisons) — the digest
 * columns alone stay `bigint` end to end. Every SQL string is the Rust string unchanged;
 * only the `format!` interpolation became template-literal `${ns.x}`.
 */

import type { InArgs } from "@libsql/client";
import type {
  AssertTrue,
  GraphNs,
  SqliteDb,
  TemporalStoreStatics,
  TemporalStore as TemporalStoreContract,
} from "./types.ts";
import process from "node:process";

/** The connection type every helper takes. */
export type Db = SqliteDb;

/**
 * Global, resettable count of SQL statements issued. The N+1 tripwire (a golden test
 * resets it, runs a batch, asserts the count is O(N/CHUNK), never O(N)). Process-global.
 */
export namespace stmt_counter {
  let count = 0;
  export function incr(): void {
    count++;
  }
  export function get(): number {
    return count;
  }
  export function reset(): void {
    count = 0;
  }
}

// =============================================================================
// cascade — mutating Z-set over cx_row (prune = weight ≠ 0)
// =============================================================================
export namespace cascade {
//! Manual, scalable cascade retraction over a generic `(tag, id)` reference graph,
//! with Z-set weights. State on disk in SQLite; cascade by hand so we owe nothing to a
//! resident engine. The polymorphic FK is metadata: a row is addressed by `(tag, id)`.
//!
//!   cx_row(key, weight)              key = tag*KEY_STRIDE + id (rowid cluster)
//!   cx_dep(parent_key, child_key)    child depends on parent; WITHOUT ROWID
//!
//! Retraction is Z-set subtraction: `weight` = # of derivations supporting a row. A row
//! dies only when weight reaches 0 (its LAST support is gone). The number of rounds is
//! the DAG depth, not the row count.

/** Rows per multi-row INSERT (inlined integer literals; the only limit is SQL length). */
const CHUNK = 4000;

/** E1 dense key stride: `(tag, id)` -> one i64 so cx_row clusters on INTEGER PRIMARY KEY. */
export const KEY_STRIDE = 1_000_000_000;

/** Dense E1 key: (rel, row) -> one i64. `rel` picks the relation, `row` the tuple. */
export function key(tag: number, id: number): number {
  return tag * KEY_STRIDE + id;
}

/** Per-statement wall-time trace, opt-in via DL_CASCADE_TRACE=1. Off by default. */
function traced(): boolean {
  const v = process.env.DL_CASCADE_TRACE;
  return v !== undefined && v !== "0" && v.length > 0;
}

/** Top-level statement split. Safe here: cascade SQL inlines only integers, never text
 *  literals, so a `;` is always a statement boundary. */
function split_statements(sql: string): string[] {
  return sql
    .split(";")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

async function exec(db: Db, sql: string): Promise<void> {
  stmt_counter.incr();
  if (traced()) {
    const t = process.hrtime.bigint();
    for (const stmt of split_statements(sql)) await db.execute(stmt);
    const ms = Number(process.hrtime.bigint() - t) / 1e6;
    const head = sql.slice(0, 50).replace(/\n/g, " ");
    console.error(`[cascade] ${ms.toFixed(2)} ms  ${head}`);
    return;
  }
  for (const stmt of split_statements(sql)) await db.execute(stmt);
}

/**
 * The Rust `db.transaction(...)` bracket for the interactive round loops: BEGIN IMMEDIATE
 * .. COMMIT via single-statement `execute` on the one pinned connection (see the file
 * header for why `client.transaction()` / `executeMultiple` cannot carry it). BEGIN /
 * COMMIT / ROLLBACK are bracket overhead, not data statements — not counted by
 * stmt_counter.
 */
export async function with_txn<T>(db: Db, body: () => Promise<T>): Promise<T> {
  await db.execute("BEGIN IMMEDIATE");
  try {
    const out = await body();
    await db.execute("COMMIT");
    return out;
  } catch (err) {
    try {
      await db.execute("ROLLBACK");
    } catch {
      // rollback failed (connection gone); surface the original error below
    }
    throw err;
  }
}

/** One scalar (first column of first row), 0 if no row. */
async function scalar(db: Db, sql: string): Promise<number> {
  stmt_counter.incr();
  const res = await db.execute(sql);
  const v = res.rows[0]?.[0];
  return v === undefined ? 0 : Number(v);
}

/** Run a fixed, upfront-known list of write statements as one atomic batch. */
async function execBatch(db: Db, stmts: ReadonlyArray<string>): Promise<void> {
  if (stmts.length === 0) return;
  for (const _s of stmts) stmt_counter.incr();
  await db.batch(stmts.slice(), "write");
}

/**
 * Create the generic cascade schema and apply the traversal tuning. The store pins to ONE
 * connection, so the churny working tables are TEMP (RAM-resident under temp_store=MEMORY,
 * never WAL-logged). Default ns ("") reproduces the live cx_ set byte-for-byte.
 */
export async function create_schema(db: Db, ns: GraphNs): Promise<void> {
  // 256 MB page cache (cache_size negative = KiB) + 1 GB read mmap.
  await db.executeMultiple("PRAGMA cache_size=-262144; PRAGMA mmap_size=1073741824;");
  await db.executeMultiple(`CREATE TABLE ${ns.row} (
            key    INTEGER PRIMARY KEY,
            weight INTEGER NOT NULL DEFAULT 1,
            tag    INTEGER GENERATED ALWAYS AS (key / 1000000000) VIRTUAL,
            id     INTEGER GENERATED ALWAYS AS (key % 1000000000) VIRTUAL
         );
         CREATE TABLE ${ns.dep} (
            parent_key INTEGER NOT NULL,
            child_key  INTEGER NOT NULL,
            PRIMARY KEY (parent_key, child_key)
         ) WITHOUT ROWID;
         CREATE TEMP TABLE ${ns.frontier} (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE ${ns.next}     (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE ${ns.hits} (key INTEGER PRIMARY KEY, dec INTEGER NOT NULL);
         CREATE TEMP TABLE ${ns.cone} (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE ${ns.scc_scope} (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE ${ns.scc_frontier} (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE ${ns.scc_next} (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE ${ns.scc_live} (key INTEGER PRIMARY KEY);
         CREATE INDEX ${ns.ix_dep_child} ON ${ns.dep} (child_key);`);
}

/** Batch-insert rows `(tag, id, weight)`. One batch = one atomic write for the load. */
export async function insert_rows(
  db: Db,
  ns: GraphNs,
  rows: ReadonlyArray<readonly [number, number, number]>,
): Promise<void> {
  const stmts: string[] = [];
  for (let i = 0; i < rows.length; i += CHUNK) {
    const chunk = rows.slice(i, i + CHUNK);
    const vals = chunk.map(([t, id, w]) => `(${key(t, id)},${w})`).join(",");
    stmts.push(`INSERT INTO ${ns.row}(key,weight) VALUES ${vals}`);
  }
  await execBatch(db, stmts);
}

/** Batch-insert dependency edges `(parent_tag, parent_id, child_tag, child_id)`. */
export async function insert_deps(
  db: Db,
  ns: GraphNs,
  edges: ReadonlyArray<readonly [number, number, number, number]>,
): Promise<void> {
  const stmts: string[] = [];
  for (let i = 0; i < edges.length; i += CHUNK) {
    const chunk = edges.slice(i, i + CHUNK);
    const vals = chunk.map(([pt, pi, ct, ci]) => `(${key(pt, pi)},${key(ct, ci)})`).join(",");
    stmts.push(`INSERT INTO ${ns.dep}(parent_key,child_key) VALUES ${vals}`);
  }
  await execBatch(db, stmts);
}

/**
 * Retract `seeds` (each `(tag, id)` loses one unit of weight). Cascade the consequence
 * and return the number of rounds (= the depth reached). Acyclic support graphs only.
 */
export async function retract(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  return with_txn(db, () => retract_body(db, ns, seeds));
}

async function retract_body(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  await exec(db, `DELETE FROM ${ns.frontier}`);
  await exec(db, `DELETE FROM ${ns.next}`);

  // Apply the -1 to each seed, then the frontier is the seeds that hit <= 0.
  const seed_vals = seeds.map(([t, id]) => key(t, id).toString()).join(",");
  const seed_in = `(${seed_vals})`;
  await exec(db, `UPDATE ${ns.row} SET weight = weight - 1 WHERE key IN ${seed_in}`);
  await exec(
    db,
    `INSERT INTO ${ns.frontier} SELECT key FROM ${ns.row} WHERE key IN ${seed_in} AND weight <= 0`,
  );

  let rounds = 0;
  // eslint-disable-next-line no-constant-condition
  while (true) {
    if ((await scalar(db, `SELECT count(*) FROM ${ns.frontier}`)) === 0) break;
    rounds++;

    // 1. hits = the frontier's children + how many supports each loses now.
    await exec(db, `DELETE FROM ${ns.hits}`);
    await exec(
      db,
      `INSERT INTO ${ns.hits}(key,dec) \
         SELECT d.child_key, count(*) \
         FROM ${ns.frontier} f CROSS JOIN ${ns.dep} d \
           ON d.parent_key = f.key \
         GROUP BY d.child_key`,
    );

    // 2. decrement each hit child by its lost-support count (indexed by rowid).
    await exec(
      db,
      `UPDATE ${ns.row} SET weight = weight - \
            (SELECT dec FROM ${ns.hits} h WHERE h.key = ${ns.row}.key) \
         WHERE key IN (SELECT key FROM ${ns.hits})`,
    );

    // 3. next frontier = hits that CROSSED zero THIS round: dead now (weight <= 0)
    //    but alive before this decrement (weight + dec > 0).
    await exec(db, `DELETE FROM ${ns.next}`);
    await exec(
      db,
      `INSERT INTO ${ns.next}(key) \
         SELECT h.key FROM ${ns.hits} h CROSS JOIN ${ns.row} r \
           ON r.key = h.key \
         WHERE r.weight <= 0 AND r.weight + h.dec > 0`,
    );

    // 4. frontier <- next. Dead rows STAY in cx_row (weight <= 0).
    await exec(db, `DELETE FROM ${ns.frontier}`);
    await exec(db, `INSERT INTO ${ns.frontier} SELECT key FROM ${ns.next}`);
  }
  return rounds;
}

/** Cycle-correct two-pass retraction (over-delete the cone, then rederive survivors). */
export async function retract_scc(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  return retract_scc_two_pass(db, ns, seeds);
}

async function retract_scc_two_pass(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  return with_txn(db, () => retract_scc_two_pass_body(db, ns, seeds));
}

async function retract_scc_two_pass_body(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  const seed_vals = seeds.map(([t, id]) => key(t, id).toString()).join(",");
  const seed_in = `(${seed_vals})`;
  await exec(
    db,
    `DELETE FROM ${ns.frontier};
                  DELETE FROM ${ns.next};
                  DELETE FROM ${ns.cone};
                  INSERT INTO ${ns.frontier} SELECT key FROM ${ns.row} WHERE key IN ${seed_in} AND weight>0;
                  UPDATE ${ns.row} SET weight=0 WHERE key IN (SELECT key FROM ${ns.frontier});
                  INSERT INTO ${ns.cone} SELECT key FROM ${ns.frontier}`,
  );

  let rounds = 0;
  // eslint-disable-next-line no-constant-condition
  while (true) {
    await exec(
      db,
      `DELETE FROM ${ns.next};
         INSERT OR IGNORE INTO ${ns.next}(key)
         SELECT d.child_key
         FROM ${ns.frontier} f CROSS JOIN ${ns.dep} d ON d.parent_key = f.key
         CROSS JOIN ${ns.row} r ON r.key = d.child_key
         WHERE r.weight > 0`,
    );
    if ((await scalar(db, `SELECT count(*) FROM ${ns.next}`)) === 0) break;
    rounds++;
    await exec(
      db,
      `UPDATE ${ns.row} SET weight=0 WHERE key IN (SELECT key FROM ${ns.next});
         INSERT OR IGNORE INTO ${ns.cone} SELECT key FROM ${ns.next};
         DELETE FROM ${ns.frontier};
         INSERT INTO ${ns.frontier} SELECT key FROM ${ns.next}`,
    );
  }

  await exec(
    db,
    `DELETE FROM ${ns.frontier};
         DELETE FROM ${ns.next};
         INSERT OR IGNORE INTO ${ns.frontier}(key)
         SELECT c.key
         FROM ${ns.cone} c CROSS JOIN ${ns.dep} d ON d.child_key = c.key
         CROSS JOIN ${ns.row} p ON p.key = d.parent_key
         WHERE p.weight > 0;
         UPDATE ${ns.row} SET weight=1 WHERE key IN (SELECT key FROM ${ns.frontier})`,
  );
  // eslint-disable-next-line no-constant-condition
  while (true) {
    await exec(
      db,
      `DELETE FROM ${ns.next};
         INSERT OR IGNORE INTO ${ns.next}(key)
         SELECT d.child_key
         FROM ${ns.frontier} f CROSS JOIN ${ns.dep} d ON d.parent_key = f.key
         CROSS JOIN ${ns.row} r ON r.key = d.child_key
         CROSS JOIN ${ns.cone} c ON c.key = d.child_key
         WHERE r.weight = 0`,
    );
    if ((await scalar(db, `SELECT count(*) FROM ${ns.next}`)) === 0) break;
    rounds++;
    await exec(
      db,
      `UPDATE ${ns.row} SET weight=1 WHERE key IN (SELECT key FROM ${ns.next});
         DELETE FROM ${ns.frontier};
         INSERT INTO ${ns.frontier} SELECT key FROM ${ns.next}`,
    );
  }
  return rounds;
}

/**
 * Forward add: `seeds` become alive; propagate aliveness to everything reachable from
 * them that was dead. Monotonic, so cycle-safe. Returns rounds.
 */
export async function assert(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  return with_txn(db, () => assert_body(db, ns, seeds));
}

async function assert_body(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  await exec(db, `DELETE FROM ${ns.frontier}`);
  await exec(db, `DELETE FROM ${ns.next}`);
  const seed_in = `(${seeds.map(([t, id]) => key(t, id).toString()).join(",")})`;
  await exec(db, `INSERT INTO ${ns.frontier} SELECT key FROM ${ns.row} WHERE key IN ${seed_in}`);
  await exec(db, `UPDATE ${ns.row} SET weight=1 WHERE key IN ${seed_in}`);
  let rounds = 0;
  // eslint-disable-next-line no-constant-condition
  while (true) {
    await exec(
      db,
      `DELETE FROM ${ns.next}; \
         INSERT INTO ${ns.next}(key) \
         SELECT DISTINCT d.child_key \
         FROM ${ns.frontier} f CROSS JOIN ${ns.dep} d ON d.parent_key = f.key \
           CROSS JOIN ${ns.row} r ON r.key = d.child_key \
         WHERE r.weight = 0`,
    );
    if ((await scalar(db, `SELECT count(*) FROM ${ns.next}`)) === 0) break;
    rounds++;
    await exec(db, `UPDATE ${ns.row} SET weight=1 WHERE key IN (SELECT key FROM ${ns.next})`);
    await exec(db, `DELETE FROM ${ns.frontier}`);
    await exec(db, `INSERT INTO ${ns.frontier} SELECT key FROM ${ns.next}`);
  }
  return rounds;
}

/**
 * Cycle-safe retraction via Delete-and-Rederive. Over-delete the forward cone, then bring
 * back any cone row still reachable from a SURVIVING row. A dead cycle has no surviving
 * anchor, so it correctly stays dead. Returns total rounds.
 */
export async function retract_dred(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  return with_txn(db, () => retract_dred_body(db, ns, seeds));
}

async function retract_dred_body(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  await exec(db, `DELETE FROM ${ns.frontier}`);
  await exec(db, `DELETE FROM ${ns.next}`);
  await exec(db, `DELETE FROM ${ns.cone}`);

  const seed_in = `(${seeds.map(([t, id]) => key(t, id).toString()).join(",")})`;
  await exec(db, `INSERT INTO ${ns.frontier} SELECT key FROM ${ns.row} WHERE key IN ${seed_in} AND weight>0`);
  await exec(db, `UPDATE ${ns.row} SET weight=0 WHERE key IN (SELECT key FROM ${ns.frontier})`);
  await exec(db, `INSERT INTO ${ns.cone} SELECT key FROM ${ns.frontier}`);
  let rounds = 0;
  // eslint-disable-next-line no-constant-condition
  while (true) {
    await exec(
      db,
      `DELETE FROM ${ns.next}; \
         INSERT INTO ${ns.next}(key) \
         SELECT DISTINCT d.child_key \
         FROM ${ns.frontier} f CROSS JOIN ${ns.dep} d ON d.parent_key = f.key \
           CROSS JOIN ${ns.row} r ON r.key = d.child_key \
         WHERE r.weight > 0`,
    );
    if ((await scalar(db, `SELECT count(*) FROM ${ns.next}`)) === 0) break;
    rounds++;
    await exec(db, `UPDATE ${ns.row} SET weight=0 WHERE key IN (SELECT key FROM ${ns.next})`);
    await exec(db, `INSERT OR IGNORE INTO ${ns.cone} SELECT key FROM ${ns.next}`);
    await exec(db, `DELETE FROM ${ns.frontier}`);
    await exec(db, `INSERT INTO ${ns.frontier} SELECT key FROM ${ns.next}`);
  }

  // rederive: cone rows with a SURVIVING parent come back; propagate forward in cone.
  await exec(db, `DELETE FROM ${ns.frontier}`);
  await exec(db, `DELETE FROM ${ns.next}`);
  await exec(
    db,
    `INSERT INTO ${ns.frontier}(key) \
         SELECT DISTINCT c.key \
         FROM ${ns.cone} c CROSS JOIN ${ns.dep} d ON d.child_key = c.key \
           CROSS JOIN ${ns.row} p ON p.key = d.parent_key \
         WHERE p.weight > 0`,
  );
  await exec(db, `UPDATE ${ns.row} SET weight=1 WHERE key IN (SELECT key FROM ${ns.frontier})`);
  // eslint-disable-next-line no-constant-condition
  while (true) {
    await exec(
      db,
      `DELETE FROM ${ns.next}; \
         INSERT INTO ${ns.next}(key) \
         SELECT DISTINCT d.child_key \
         FROM ${ns.frontier} f CROSS JOIN ${ns.dep} d ON d.parent_key = f.key \
           CROSS JOIN ${ns.row} r ON r.key = d.child_key \
           CROSS JOIN ${ns.cone} c ON c.key = d.child_key \
         WHERE r.weight = 0`,
    );
    if ((await scalar(db, `SELECT count(*) FROM ${ns.next}`)) === 0) break;
    rounds++;
    await exec(db, `UPDATE ${ns.row} SET weight=1 WHERE key IN (SELECT key FROM ${ns.next})`);
    await exec(db, `DELETE FROM ${ns.frontier}`);
    await exec(db, `INSERT INTO ${ns.frontier} SELECT key FROM ${ns.next}`);
  }
  return rounds;
}

/**
 * Cycle-safe retraction, Delete-and-Rederive expressed as TWO recursive CTEs so SQLite
 * runs the whole traversal AND rederive inside its C engine. Identical semantics/result
 * to retract_dred; the per-round round-trip tax is gone. Returns 0 (rounds not meaningful
 * for the set-at-once CTE form). Every statement is fixed upfront (no reads gate the next
 * one), so this runs as ONE atomic batch rather than an interactive transaction.
 */
export async function retract_dred_cte(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<readonly [number, number]>,
): Promise<number> {
  const seed_in = `(${seeds.map(([t, id]) => key(t, id).toString()).join(",")})`;
  const stmts: string[] = [
    `DELETE FROM ${ns.cone}`,
    // Phase 1 — over-delete.
    `INSERT INTO ${ns.cone}(key)
         WITH RECURSIVE cone(key) AS (
            SELECT key FROM ${ns.row} WHERE key IN ${seed_in} AND weight>0
            UNION
            SELECT d.child_key FROM cone
              JOIN ${ns.dep} d ON d.parent_key = cone.key
              JOIN ${ns.row} r ON r.key = d.child_key
             WHERE r.weight>0
         )
         SELECT key FROM cone`,
    `UPDATE ${ns.row} SET weight=0 WHERE key IN (SELECT key FROM ${ns.cone})`,
    // Phase 2 — rederive.
    `DELETE FROM ${ns.frontier}`, // reused as the alive-set sink
    `INSERT INTO ${ns.frontier}(key)
         WITH RECURSIVE alive(key) AS (
            SELECT c.key FROM ${ns.cone} c
              JOIN ${ns.dep} d ON d.child_key = c.key
              JOIN ${ns.row} p ON p.key = d.parent_key
             WHERE p.weight>0
            UNION
            SELECT d.child_key FROM alive
              JOIN ${ns.dep} d ON d.parent_key = alive.key
              JOIN ${ns.cone} c ON c.key = d.child_key
         )
         SELECT key FROM alive`,
    `UPDATE ${ns.row} SET weight=1 WHERE key IN (SELECT key FROM ${ns.frontier})`,
  ];
  await execBatch(db, stmts);
  return 0;
}
}

// =============================================================================
// reach — read-only graph queries over cx_dep (prune = reached)
// =============================================================================
export namespace reach {
//! The v5 graph covering set, on-disk over `cx_dep(parent_key, child_key)`. Each function
//! has a resident pure-JS oracle (the tarjan/walk reference in tests/golden.test.ts) and
//! MUST agree with it byte-for-byte.

/** The recursive-closure CTE over the ns's dep table. */
function reach_cte(ns: GraphNs): string {
  return `WITH RECURSIVE reach(src,dst) AS (\
        SELECT parent_key,child_key FROM ${ns.dep} \
        UNION \
        SELECT reach.src, dep.child_key FROM reach \
        JOIN ${ns.dep} dep ON dep.parent_key = reach.dst)`;
}

/** Condensation, all component ids expressed as MIN-member representative keys. */
export interface Condensed {
  comp_of: [number, number][]; // (node_key, comp_repr)
  size: [number, number][]; // (comp_repr, member_count)
  cyclic: [number, boolean][]; // (comp_repr, is_cyclic)
  cadj: [number, number][]; // (parent_comp_repr, child_comp_repr), deduped, no self
}

/** Forward transitive closure from `start` (strict; includes start iff its SCC is cyclic). */
export async function reaches_from(db: Db, ns: GraphNs, start: number): Promise<number[]> {
  const sql = `WITH RECURSIVE reach(key) AS (\
            SELECT child_key FROM ${ns.dep} WHERE parent_key = ${start} \
            UNION \
            SELECT dep.child_key FROM ${ns.dep} dep JOIN reach ON dep.parent_key = reach.key\
        ) SELECT key FROM reach ORDER BY key`;
  stmt_counter.incr();
  const res = await db.execute(sql);
  return res.rows.map((r) => Number(r[0]));
}

/** Reverse transitive closure into `target` (rides ix_*_dep_child). */
export async function reached_by(db: Db, ns: GraphNs, target: number): Promise<number[]> {
  const sql = `WITH RECURSIVE reach(key) AS (\
            SELECT parent_key FROM ${ns.dep} WHERE child_key = ${target} \
            UNION \
            SELECT dep.parent_key FROM ${ns.dep} dep JOIN reach ON dep.child_key = reach.key\
        ) SELECT key FROM reach ORDER BY key`;
  stmt_counter.incr();
  const res = await db.execute(sql);
  return res.rows.map((r) => Number(r[0]));
}

let walkTableId = 0;

/** Multi-source min-depth BFS. halt stops expansion AT a halt node; depth_cap bounds it. */
export async function multi_source_walk(
  db: Db,
  ns: GraphNs,
  starts: ReadonlyArray<readonly [number, number, number]>,
  halt: ReadonlyArray<number> | null,
  depth_cap: number | null,
): Promise<[number, number, number][]> {
  const tableId = walkTableId++;
  const reached_table = `_reached_${tableId}`;
  const halt_table = `_halt_${tableId}`;
  // Sequential, auto-committing statement stream (see engine.ts's file header: the round
  // loop reads a count between writes, so an explicit `client.transaction()` would strand
  // this connection's TEMP tables on its next plain call).
  await db.executeMultiple(`CREATE TEMP TABLE ${reached_table} (\
            tag INTEGER NOT NULL, node INTEGER NOT NULL, depth INTEGER NOT NULL, round INTEGER NOT NULL,\
            PRIMARY KEY(tag,node)\
        )`);
  await db.executeMultiple(`CREATE TEMP TABLE ${halt_table} (node INTEGER PRIMARY KEY)`);

  if (halt) {
    for (const node of halt) {
      await db.executeMultiple(`INSERT OR IGNORE INTO ${halt_table} VALUES (${node})`);
    }
  }

  const ordered = starts.slice().sort((a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2]);
  for (const [tag, node, depth] of ordered) {
    await db.executeMultiple(
      `INSERT OR IGNORE INTO ${reached_table}(tag,node,depth,round) VALUES (${tag},${node},${depth},0)`,
    );
  }

  let round = 0;
  const expand_guard = depth_cap !== null ? ` AND reached.depth < ${depth_cap}` : "";
  // eslint-disable-next-line no-constant-condition
  while (true) {
    await db.executeMultiple(`INSERT OR IGNORE INTO ${reached_table}(tag,node,depth,round) \
             SELECT reached.tag, dep.child_key, reached.depth + 1, ${round + 1} \
             FROM ${reached_table} reached JOIN ${ns.dep} dep ON dep.parent_key = reached.node \
             WHERE reached.round = ${round} \
             AND NOT EXISTS (SELECT 1 FROM ${halt_table} halt WHERE halt.node = reached.node)${expand_guard}`);

    const countRes = await db.execute(`SELECT count(*) FROM ${reached_table} WHERE round = ${round + 1}`);
    const inserted = Number(countRes.rows[0]?.[0] ?? 0);
    if (inserted === 0) break;
    round++;
  }

  const rowsRes = await db.execute(`SELECT tag,node,depth FROM ${reached_table} ORDER BY tag,node`);
  await db.executeMultiple(`DROP TABLE ${reached_table}; DROP TABLE ${halt_table}`);
  return rowsRes.rows.map((r) => [Number(r.tag), Number(r.node), Number(r.depth)] as [number, number, number]);
}

/** halt-only, depth-agnostic special case of `multi_source_walk`. */
export async function multi_source_halt_bfs(
  db: Db,
  ns: GraphNs,
  starts: ReadonlyArray<readonly [number, number]>,
  halt: ReadonlyArray<number>,
): Promise<[number, number][]> {
  const starts3: [number, number, number][] = starts.map(([t, n]) => [t, n, 0]);
  return (await multi_source_walk(db, ns, starts3, halt, null)).map(([t, n]) => [t, n]);
}

/**
 * SCC partition as (node_key, comp_repr = MIN member key). Compare on the partition.
 *
 * ORM seam: Rust reads the unaliased COALESCE column by index; libsql reads by column
 * name, so the computed column carries an explicit `AS repr` alias. The SQL semantics
 * (GROUP BY/ORDER BY on node.key, the COALESCE expression) are unchanged.
 */
export async function scc_labels(db: Db, ns: GraphNs): Promise<[number, number][]> {
  const sql = `${reach_cte(ns)} \
         SELECT node.key AS key, COALESCE(MIN(CASE WHEN backward.src IS NOT NULL THEN forward.dst END), node.key) AS repr \
         FROM ${ns.row} node \
         LEFT JOIN reach forward ON forward.src = node.key \
         LEFT JOIN reach backward ON backward.src = forward.dst AND backward.dst = node.key \
         GROUP BY node.key ORDER BY node.key`;
  stmt_counter.incr();
  const res = await db.execute(sql);
  return res.rows.map((r) => [Number(r.key), Number(r.repr)] as [number, number]);
}

/** Condensation derived from `scc_labels` + cx_dep group-bys. */
export async function build_condensed(db: Db, ns: GraphNs): Promise<Condensed> {
  const comp_of = await scc_labels(db, ns);
  const repr_by_node = new Map<number, number>();
  for (const [node, repr] of comp_of) repr_by_node.set(node, repr);
  const member_counts = new Map<number, number>();
  for (const [, repr] of comp_of) member_counts.set(repr, (member_counts.get(repr) ?? 0) + 1);

  stmt_counter.incr();
  const edgesRes = await db.execute(`SELECT parent_key,child_key FROM ${ns.dep}`);
  const self_loops = new Set<number>();
  const condensed_edges = new Set<string>();
  const cadj: [number, number][] = [];
  for (const row of edgesRes.rows) {
    const parent_key = Number(row.parent_key);
    const child_key = Number(row.child_key);
    const parent_repr = repr_by_node.get(parent_key)!;
    const child_repr = repr_by_node.get(child_key)!;
    if (parent_key === child_key) self_loops.add(parent_repr);
    if (parent_repr !== child_repr) {
      const k = `${parent_repr}:${child_repr}`;
      if (!condensed_edges.has(k)) {
        condensed_edges.add(k);
        cadj.push([parent_repr, child_repr]);
      }
    }
  }
  cadj.sort((a, b) => a[0] - b[0] || a[1] - b[1]);

  const size: [number, number][] = [];
  for (const [repr, count] of member_counts) size.push([repr, count]);
  size.sort((a, b) => a[0] - b[0]);
  const cyclic: [number, boolean][] = [];
  for (const [repr, count] of member_counts) {
    cyclic.push([repr, count > 1 || self_loops.has(repr)]);
  }
  cyclic.sort((a, b) => a[0] - b[0]);

  return { comp_of, size, cyclic, cadj };
}

/**
 * Reachable ordered-pair count; matches the v5 reference byte-for-byte. Counts over the
 * CONDENSATION (ncomp components, bitset reach) so it does NOT materialize the Θ(V²)
 * node-pair table. i128 in Rust -> bigint here. The bitset is u64 words in bigint
 * (JS number `<<` is 32-bit; bigint `<<` carries the full 64-bit word).
 */
export async function count_pairs(db: Db, ns: GraphNs): Promise<bigint> {
  const cond = await build_condensed(db, ns);

  const reprs = new Set<number>();
  for (const [repr] of cond.size) reprs.add(repr);
  const reprSorted = [...reprs].sort((a, b) => a - b);
  const idx = new Map<number, number>();
  reprSorted.forEach((r, i) => idx.set(r, i));
  const ncomp = reprSorted.length;
  if (ncomp === 0) return 0n;

  const size = new Array<bigint>(ncomp).fill(0n);
  for (const [repr, n] of cond.size) size[idx.get(repr)!] = BigInt(n);
  const cyclic = new Array<boolean>(ncomp).fill(false);
  for (const [repr, is_cyclic] of cond.cyclic) cyclic[idx.get(repr)!] = is_cyclic;
  const cadj: number[][] = Array.from({ length: ncomp }, () => []);
  for (const [parent, child] of cond.cadj) cadj[idx.get(parent)!]!.push(idx.get(child)!);

  // topo order, bitset-propagate reach; total = Σ cyclic·size² + Σ size·(reachable sizes)
  const indeg = new Array<number>(ncomp).fill(0);
  for (let cc = 0; cc < ncomp; cc++) for (const s of cadj[cc]!) indeg[s]! += 1;
  const topo: number[] = [];
  for (let x = 0; x < ncomp; x++) if (indeg[x] === 0) topo.push(x);
  let qi = 0;
  while (qi < topo.length) {
    const x = topo[qi]!;
    qi++;
    for (const s of cadj[x]!) {
      indeg[s]! -= 1;
      if (indeg[s] === 0) topo.push(s);
    }
  }
  const words = (ncomp + 63) >> 6;
  const reach = new Array<bigint>(ncomp * words).fill(0n);
  for (let ti = topo.length - 1; ti >= 0; ti--) {
    const cc = topo[ti]!;
    for (const s of cadj[cc]!) {
      reach[cc * words + (s >> 6)]! |= 1n << BigInt(s & 63);
      for (let w = 0; w < words; w++) {
        reach[cc * words + w]! |= reach[s * words + w]!;
      }
    }
  }
  let total = 0n;
  for (let cc = 0; cc < ncomp; cc++) {
    if (cyclic[cc]) total += size[cc]! * size[cc]!;
    let wsum = 0n;
    for (let w = 0; w < words; w++) {
      let bits = reach[cc * words + w]!;
      while (bits !== 0n) {
        const b = w * 64 + trailing_zeros(bits);
        wsum += size[b]!;
        bits &= bits - 1n;
      }
    }
    total += size[cc]! * wsum;
  }
  return total;
}

/** Count trailing zero bits of a non-zero bigint (mirrors u64::trailing_zeros). */
function trailing_zeros(x: bigint): number {
  let n = 0;
  let v = x;
  if ((v & 0xffffffffn) === 0n) {
    n += 32;
    v >>= 32n;
  }
  if ((v & 0xffffn) === 0n) {
    n += 16;
    v >>= 16n;
  }
  if ((v & 0xffn) === 0n) {
    n += 8;
    v >>= 8n;
  }
  if ((v & 0xfn) === 0n) {
    n += 4;
    v >>= 4n;
  }
  if ((v & 0x3n) === 0n) {
    n += 2;
    v >>= 2n;
  }
  if ((v & 0x1n) === 0n) n += 1;
  return n;
}
}

// =============================================================================
// reconcile — salsa-in-SQL digest plane (prune = digest moved)
// =============================================================================
export namespace reconcile {
//! Reconciliation in SQLite — salsa's red-green dirty-check, done over a dep table
//! instead of a resident memo graph.
//!   rx_memo(id, digest, changed_at, verified_at)  -- one row per reactive rel
//!   rx_dep(reader, read)                           -- reader READS read
//!
//! `digest` is a full i64; it flows as bigint here. The client's `intMode:"bigint"` (set
//! once, at connection open) makes every integer column round-trip as bigint, so the
//! digest reads below need no per-statement opt-in: they get 64-bit fidelity for free.

async function exec(db: Db, sql: string): Promise<void> {
  stmt_counter.incr();
  await db.executeMultiple(sql);
}

async function query_ids(db: Db, sql: string): Promise<number[]> {
  stmt_counter.incr();
  const res = await db.execute(sql);
  return res.rows.map((r) => Number(r[0]));
}

/** Query bigint ids/digests (full 64-bit fidelity, the client's global intMode). */
async function query_bigints(db: Db, sql: string): Promise<bigint[]> {
  stmt_counter.incr();
  const res = await db.execute(sql);
  return res.rows.map((r) => r[0] as bigint);
}

export async function create_schema(db: Db, ns: GraphNs): Promise<void> {
  await db.executeMultiple(`CREATE TABLE ${ns.memo} (
            id          INTEGER PRIMARY KEY,
            digest      INTEGER NOT NULL,
            changed_at  INTEGER NOT NULL DEFAULT 0,
            verified_at INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE ${ns.rdep} (
            reader INTEGER NOT NULL,
            read   INTEGER NOT NULL,
            PRIMARY KEY (reader, read)
         ) WITHOUT ROWID;
         CREATE INDEX ${ns.ix_rdep_read} ON ${ns.rdep} (read);`);
}

/**
 * Seed a rel's memo (its output digest and the deps it read), at revision `rev`. Every
 * statement is fixed upfront, so this runs as one atomic batch.
 */
export async function seed(
  db: Db,
  ns: GraphNs,
  id: number,
  digest: bigint,
  deps: ReadonlyArray<number>,
  rev: number,
): Promise<void> {
  const stmts: string[] = [
    `INSERT INTO ${ns.memo}(id,digest,changed_at,verified_at) VALUES (${id},${digest},${rev},${rev})`,
  ];
  for (const d of deps) {
    stmts.push(`INSERT OR IGNORE INTO ${ns.rdep}(reader,read) VALUES (${id},${d})`);
  }
  await execBatch(db, stmts);
}

/** Run a fixed, upfront-known list of write statements as one atomic batch. */
async function execBatch(db: Db, stmts: ReadonlyArray<string>): Promise<void> {
  if (stmts.length === 0) return;
  for (const _s of stmts) stmt_counter.incr();
  await db.batch(stmts.slice(), "write");
}

/** An input's digest moved at `rev`: bump its changed_at so the CTE sees it stale. */
export async function mark_changed(db: Db, ns: GraphNs, ids: ReadonlyArray<number>, rev: number): Promise<void> {
  const in_list = ids.map((i) => i.toString()).join(",");
  await exec(db, `UPDATE ${ns.memo} SET changed_at=${rev} WHERE id IN (${in_list})`);
}

/**
 * The invalidation query, in SQL: the current stale FRONTIER — every derived rel that
 * READS something whose digest changed after that rel was last verified. A FRONTIER, not
 * the full closure; the lazy step gives early cutoff. (Low-level query; the parity-proven
 * driver is `propagate`.)
 */
export async function dirty(db: Db, ns: GraphNs): Promise<number[]> {
  return query_ids(
    db,
    `SELECT DISTINCT dep.reader
         FROM ${ns.rdep} dep
         JOIN ${ns.memo} d ON d.id = dep.read
         JOIN ${ns.memo} s ON s.id = dep.reader
         WHERE d.changed_at > s.verified_at
         ORDER BY dep.reader`,
  );
}

/**
 * Record a recomputed rel's new digest at `rev`. Returns whether the digest MOVED. If it
 * moved, changed_at = rev (readers stay dirty). If not, changed_at is untouched (EARLY
 * CUTOFF). verified_at = rev either way.
 */
export async function verify(db: Db, ns: GraphNs, id: number, new_digest: bigint, rev: number): Promise<boolean> {
  stmt_counter.incr();
  const res = await db.execute(`SELECT digest FROM ${ns.memo} WHERE id=${id}`);
  const old_row = res.rows[0] as { digest: bigint } | undefined;
  const old = old_row !== undefined ? old_row.digest : 0n;
  const moved = old !== new_digest;
  const set_changed = moved ? `, changed_at=${rev}` : "";
  await exec(
    db,
    `UPDATE ${ns.memo} SET digest=${new_digest}, verified_at=${rev}${set_changed} WHERE id=${id}`,
  );
  return moved;
}

/**
 * The correct reconcile sweep (the labkit `SqlReconciler` shape, proven byte-identical to
 * salsa on DAGs with diamonds). Process `seeds` and every transitive reader whose digest
 * actually moves, in ASCENDING id order (a valid topo order — every rx_dep edge is
 * parent->child with child > parent). Returns the recompute count (the early-cutoff meter).
 */
/** Binary min-heap + membership set = the Rust `BTreeSet<i64>` (engine.rs:1194)
 *  insert / pop-first pair: O(log n) per op instead of an O(n) scan per pop. */
class AscendingIdQueue {
  private readonly heap: number[] = [];
  private readonly members = new Set<number>();

  constructor(seeds: ReadonlyArray<number>) {
    for (const s of seeds) this.add(s);
  }

  get size(): number {
    return this.heap.length;
  }

  add(id: number): void {
    if (this.members.has(id)) return;
    this.members.add(id);
    this.heap.push(id);
    let i = this.heap.length - 1;
    while (i > 0) {
      const parent = (i - 1) >> 1;
      if (this.heap[parent]! <= this.heap[i]!) break;
      [this.heap[parent], this.heap[i]] = [this.heap[i]!, this.heap[parent]!];
      i = parent;
    }
  }

  pop_min(): number {
    const min = this.heap[0]!;
    const last = this.heap.pop()!;
    if (this.heap.length > 0) {
      this.heap[0] = last;
      let i = 0;
      for (;;) {
        const left = 2 * i + 1;
        const right = left + 1;
        let smallest = i;
        if (left < this.heap.length && this.heap[left]! < this.heap[smallest]!) smallest = left;
        if (right < this.heap.length && this.heap[right]! < this.heap[smallest]!) smallest = right;
        if (smallest === i) break;
        [this.heap[i], this.heap[smallest]] = [this.heap[smallest]!, this.heap[i]!];
        i = smallest;
      }
    }
    this.members.delete(min);
    return min;
  }
}

export async function propagate(
  db: Db,
  ns: GraphNs,
  seeds: ReadonlyArray<number>,
  rev: number,
  recompute: (id: number, dep_digests: bigint[]) => bigint,
): Promise<number> {
  const dirty_set = new AscendingIdQueue(seeds);
  let count = 0;
  while (dirty_set.size > 0) {
    // smallest id (BTreeSet pop-first); ascending = topo here.
    const id = dirty_set.pop_min();

    const dep_digests = await query_bigints(
      db,
      `SELECT m.digest FROM ${ns.rdep} dep JOIN ${ns.memo} m ON m.id = dep.read WHERE dep.reader = ${id}`,
    );
    const new_digest = recompute(id, dep_digests);
    const moved = await verify(db, ns, id, new_digest, rev);
    count++;
    if (moved) {
      const readers = await query_ids(db, `SELECT reader FROM ${ns.rdep} WHERE read = ${id}`);
      for (const reader of readers) dirty_set.add(reader);
    }
  }
  return count;
}

/** XOR of every durable rx_memo digest — proves the on-disk memo is the truth. */
export async function answer(db: Db, ns: GraphNs): Promise<bigint> {
  const digests = await query_bigints(db, `SELECT digest FROM ${ns.memo}`);
  let acc = 0n;
  for (const d of digests) acc = BigInt.asUintN(64, acc ^ BigInt.asUintN(64, d));
  return BigInt.asIntN(64, acc);
}
}

// =============================================================================
// temporal — append-only bitemporal fact storage (base layer, not a parity trait)
// =============================================================================
export namespace temporal {
//! fact(key, tt_from, tt_to, weight) WITHOUT ROWID; partial index ix_live WHERE
//! tt_to IS NULL. commit(deltas) = one batched, atomic write: insert new live facts, weight
//! += dw, close (tt_to = rev) at weight <= 0. live()/digest() read the tt_to IS NULL set.
//! Role: the versioned base layer UNDER the graph; no parity trait.

const SOFT_HEAP_LIMIT = "PRAGMA soft_heap_limit=4294967296;";

/** splitmix64 — same hash every plane agrees on (local copy, as in engine.rs). */
function mix_key(key: number): number {
  let mixed = BigInt.asUintN(64, BigInt(key) + 0x9e3779b97f4a7c15n);
  mixed = BigInt.asUintN(64, (mixed ^ (mixed >> 30n)) * 0xbf58476d1ce4e5b9n);
  mixed = BigInt.asUintN(64, (mixed ^ (mixed >> 27n)) * 0x94d049bb133111ebn);
  return Number(BigInt.asIntN(64, mixed ^ (mixed >> 31n)));
}

async function create_schema(db: Db): Promise<void> {
  await db.executeMultiple(`CREATE TABLE fact(key INTEGER NOT NULL, tt_from INTEGER NOT NULL,
            tt_to INTEGER, weight INTEGER NOT NULL, PRIMARY KEY(key,tt_from)) WITHOUT ROWID;
         CREATE INDEX ix_live ON fact(key) WHERE tt_to IS NULL;
         CREATE TEMP TABLE d(key INTEGER PRIMARY KEY, dw INTEGER);`);
}

function delta_json(deltas: ReadonlyArray<readonly [number, number]>): string {
  const parts = deltas.map(([key, weight]) => `[${key},${weight}]`);
  return `[${parts.join(",")}]`;
}

/** A bitemporal fact store over one connection. */
export class TemporalStore implements TemporalStoreContract {
  private revision = 0;

  private constructor(private readonly db: Db) {}

  static async attach(db: Db): Promise<TemporalStore> {
    await db.executeMultiple(SOFT_HEAP_LIMIT);
    await create_schema(db);
    const store = new TemporalStore(db);
    const res = await db.execute("SELECT COALESCE(MAX(tt_from), 0) FROM fact");
    store.revision = Number(res.rows[0]?.[0] ?? 0);
    return store;
  }

  async commit(deltas: ReadonlyArray<readonly [number, number]>): Promise<void> {
    if (deltas.length === 0) return;
    this.revision += 1;
    const revision = this.revision;
    const delta_json_val = delta_json(deltas);
    const stmts: { sql: string; args: InArgs }[] = [
      { sql: "DELETE FROM d", args: [] },
      {
        sql: "INSERT INTO d(key,dw) SELECT json_extract(value,'$[0]'), sum(json_extract(value,'$[1]')) FROM json_each(?) GROUP BY 1",
        args: [delta_json_val],
      },
      {
        sql: "INSERT INTO fact(key,tt_from,tt_to,weight) SELECT d.key, ?, NULL, 0 FROM d LEFT JOIN fact f ON f.key=d.key AND f.tt_to IS NULL WHERE f.key IS NULL AND d.dw>0",
        args: [revision],
      },
      {
        sql: "UPDATE fact SET weight = weight + (SELECT dw FROM d WHERE d.key=fact.key) WHERE tt_to IS NULL AND key IN (SELECT key FROM d)",
        args: [],
      },
      {
        sql: "UPDATE fact SET tt_to=? WHERE tt_to IS NULL AND weight<=0 AND key IN (SELECT key FROM d)",
        args: [revision],
      },
    ];
    await this.db.batch(stmts, "write");
  }

  async live(): Promise<number> {
    stmt_counter.incr();
    const res = await this.db.execute("SELECT count(*) FROM fact WHERE tt_to IS NULL");
    return Number(res.rows[0]?.[0] ?? 0);
  }

  async total_rows(): Promise<number> {
    stmt_counter.incr();
    const res = await this.db.execute("SELECT count(*) FROM fact");
    return Number(res.rows[0]?.[0] ?? 0);
  }

  async digest(): Promise<number> {
    stmt_counter.incr();
    const res = await this.db.execute("SELECT key FROM fact WHERE tt_to IS NULL");
    return res.rows.reduce((digest, row) => (digest ^ mix_key(Number(row[0]))) | 0, 0);
  }

  conn(): Db {
    return this.db;
  }
}

/** Static-side proof (./types.ts): `implements` covers the instance side only. */
export type TemporalStoreStaticsHold = AssertTrue<typeof TemporalStore extends TemporalStoreStatics ? true : false>;
}
