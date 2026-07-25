/**
 * spine.ts — the CLOSED v6 data model, ported from src/spine.rs: 9 tables + unified
 * node/edge, the secondary indexes, the pure transforms, and the rel-table mint.
 *
 * ORM seam: Rust derives DDL from SeaORM entities via `Schema::create_table_from_entity`.
 * The TS port hand-writes the equivalent `CREATE TABLE` strings (the columns, PKs, UNIQUE,
 * WITHOUT ROWID flags, and secondary indexes mirror sea-query's SqliteQueryBuilder output
 * for the entities in spine.rs). IF NOT EXISTS is added for idempotent stamping (a safe
 * addition; it does not change the schema object set).
 */

import type {
  AssertTrue,
  CreateAllTables,
  IRelTableApi,
  RelCol as RelColShape,
  SqliteDb,
} from "./types.ts";
import { createHash } from "node:crypto";

/** The unified graph family discriminator, stored as a small i32 ordinal. */
export const Family = {
  Df: 0,
  Call: 1,
  Type: 2,
  Module: 3,
} as const;
export type Family = (typeof Family)[keyof typeof Family];

export function family_as_i32(f: Family): number {
  return f;
}
export function family_from_i32(v: number): Family | undefined {
  switch (v) {
    case Family.Df:
      return Family.Df;
    case Family.Call:
      return Family.Call;
    case Family.Type:
      return Family.Type;
    case Family.Module:
      return Family.Module;
    default:
      return undefined;
  }
}

/** A rev is either committed (a git sha) or the WORK state of one root. */
export const RevKind = {
  Committed: 0,
  Work: 1,
} as const;
export type RevKind = (typeof RevKind)[keyof typeof RevKind];
export function revkind_as_i32(k: RevKind): number {
  return k;
}

// ---- entity row types (mirror spine.rs entity Models) ----------------------
export interface StringsRow {
  string_id: number;
  content: string;
}
export interface ReposRow {
  repo_id: number;
  slug: string;
  root: string;
  url: string;
}
export interface RootsRow {
  root_id: number;
  repo_id: number;
  path_string_id: number;
}
export interface RepoRevsRow {
  rev_id: number;
  repo_id: number;
  kind: number;
  git_sha: Uint8Array | null;
  root_id: number | null;
  base_rev_id: number | null;
}
export interface FilesRow {
  file_id: number;
  content_hash: Uint8Array;
  size: number;
  lines: number;
}
export interface RevsFilesRow {
  rev_id: number;
  path_string_id: number;
  file_id: number;
}
export interface FileBytesRow {
  file_id: number;
  start: number;
  end: number;
  string_id: number | null;
}
/** The nine table names (sourced from the entities in spine.rs). */
export function table_names(): string[] {
  return ["strings", "repos", "roots", "repo_revs", "files", "revs_files", "file_bytes", "node", "edge"];
}

/**
 * Create every spine table. DDL hand-written to mirror `Schema::create_table_from_entity`
 * + `secondary_indexes`. WITHOUT ROWID is applied to the composite-key junctions
 * (revs_files, file_bytes, edge) — they become their own PK index instead of paying for a
 * rowid heap plus a duplicate autoindex.
 */
export async function create_all_tables(db: SqliteDb): Promise<void> {
  await db.executeMultiple(`CREATE TABLE IF NOT EXISTS strings (string_id INTEGER PRIMARY KEY, content TEXT NOT NULL UNIQUE);
         CREATE TABLE IF NOT EXISTS repos (repo_id INTEGER PRIMARY KEY, slug TEXT NOT NULL UNIQUE, root TEXT NOT NULL, url TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS roots (root_id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL, path_string_id INTEGER NOT NULL UNIQUE);
         CREATE TABLE IF NOT EXISTS repo_revs (rev_id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL, kind INTEGER NOT NULL, git_sha BLOB, root_id INTEGER, base_rev_id INTEGER);
         CREATE TABLE IF NOT EXISTS files (file_id INTEGER PRIMARY KEY, content_hash BLOB NOT NULL UNIQUE, size INTEGER NOT NULL, lines INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS revs_files (rev_id INTEGER NOT NULL, path_string_id INTEGER NOT NULL, file_id INTEGER NOT NULL, PRIMARY KEY (rev_id, path_string_id)) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS file_bytes (file_id INTEGER NOT NULL, start INTEGER NOT NULL, end INTEGER NOT NULL, string_id INTEGER, PRIMARY KEY (file_id, start, end)) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS node (node_id INTEGER PRIMARY KEY, family INTEGER NOT NULL, file_id INTEGER NOT NULL, byte_start INTEGER NOT NULL, byte_len INTEGER NOT NULL, kind INTEGER NOT NULL, name_id INTEGER);
         CREATE TABLE IF NOT EXISTS edge (family INTEGER NOT NULL, src_id INTEGER NOT NULL, dst_id INTEGER NOT NULL, kind INTEGER NOT NULL, PRIMARY KEY (family, src_id, dst_id, kind)) WITHOUT ROWID;`);
  for (const sql of secondary_indexes()) await db.executeMultiple(sql);
}

/** Identity-uniqueness and reverse-traversal indexes not expressible as single-col UNIQUE. */
function secondary_indexes(): string[] {
  return [
    "CREATE UNIQUE INDEX IF NOT EXISTS ux_repo_revs_identity ON repo_revs (repo_id, git_sha)",
    "CREATE UNIQUE INDEX IF NOT EXISTS ux_node_identity ON node (family, file_id, byte_start, kind)",
    "CREATE INDEX IF NOT EXISTS ix_revs_files_by_file ON revs_files (file_id)",
    "CREATE INDEX IF NOT EXISTS ix_edge_by_dst ON edge (family, dst_id)",
    // Exactly one WORK rev per root.
    "CREATE UNIQUE INDEX IF NOT EXISTS ux_repo_revs_work_root ON repo_revs (root_id) WHERE kind = 1",
  ];
}

// ---- transforms (pure pre-storage) -----------------------------------------
export namespace transforms {
//! Pure pre-storage transforms, each replacing a named v5 hack. No DB, no state.

/** Strip to ASCII alphanumeric, lowercase (the one correct v5 transform). */
export function normalize(value: string): string {
  let out = "";
  for (const c of value) {
    if (/[A-Za-z0-9]/.test(c)) out += c.toLowerCase();
  }
  return out;
}

/**
 * Content hash for a file, truncated to 16 bytes, the UNIQUE natural key on `files`.
 *
 * DIVERGENCE: Rust uses blake3. The TS port has no blake3 without a new dependency (out of
 * budget), so this uses BLAKE2b-512 (node:crypto, zero deps) truncated to 16 bytes. Same
 * role (collision-resistant 16-byte natural key for dedup), different primitive. Listed in
 * the final report. Not exercised by the golden test.
 */
export function content_hash(bytes: Uint8Array): Uint8Array {
  return createHash("blake2b512").update(bytes).digest().subarray(0, 16);
}

/** A 40-char git sha hex string -> 20 raw bytes for `repo_revs.git_sha`. */
export function git_sha_bytes(hex: string): Uint8Array | null {
  if (hex.length !== 40 || !/^[0-9a-fA-F]+$/.test(hex)) return null;
  const out = new Uint8Array(20);
  for (let i = 0; i < 20; i++) out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

/** Byte offset -> (line, col), 0-based, derived from content on demand. */
export function byte_to_linecol(content: Uint8Array, offset: number): [number, number] {
  const end = Math.min(offset, content.length);
  let line = 0;
  let col = 0;
  for (let i = 0; i < end; i++) {
    if (content[i] === 0x0a) {
      line += 1;
      col = 0;
    } else {
      col += 1;
    }
  }
  return [line, col];
}
}

// ---- rels (mint new rel tables at runtime) ---------------------------------
export namespace rels {
//! The OPEN core: mint new rel tables at runtime. Every rel table is WITHOUT ROWID with a
//! composite PK over its key columns (set semantics, no duplicate autoindex).

/** A column in a dynamically-created rel table. Declared in ./types.ts (the header);
 *  aliased here so `rels.RelCol` still resolves. */
export type RelCol = RelColShape;

export function rel_col_int(name: string): RelCol {
  return { kind: "int", name };
}
export function rel_col_int_null(name: string): RelCol {
  return { kind: "int_null", name };
}
export function rel_col_text(name: string): RelCol {
  return { kind: "text", name };
}

/**
 * Create rel table `name` with `cols`, identified by the named `pk` columns.
 *
 * Two key shapes, chosen by `surrogate`:
 *
 *  - omitted: WITHOUT ROWID, `PRIMARY KEY (pk)`. The key columns ARE the row locator, so
 *    the table is one b-tree and there is no duplicate autoindex. Cheapest on disk, and
 *    the right shape when nothing outside the table needs to name a row.
 *
 *  - given: a rowid table whose `<surrogate> INTEGER PRIMARY KEY` IS the rowid, plus
 *    `UNIQUE (pk)` to keep set semantics. That buys a DENSE INTEGER SURROGATE per row:
 *    ids run 1..n with no gaps in a fresh table, SQLite stores them as a min-bit varint
 *    (one byte up to 127, two to 16383, and so on), and any other table can name a row
 *    with that integer instead of copying its key columns. This is what the Z-set fact
 *    plane needs: `cascade.key(tag, id)` packs a dense rel tag and a dense row id into one
 *    i64, so `cx_row`/`cx_dep` address rows with integers and never with strings or hashes.
 *    The cost is honest and is the reason this is opt-in: `UNIQUE (pk)` is a second b-tree
 *    holding the key columns again.
 */
export async function create_rel_table(
  db: SqliteDb,
  name: string,
  cols: ReadonlyArray<RelCol>,
  pk: ReadonlyArray<string>,
  surrogate?: string,
): Promise<void> {
  const col_defs = cols.map((c) => {
    const ty = c.kind === "text" ? "TEXT" : "INTEGER";
    const nn = c.kind === "int_null" ? "" : " NOT NULL";
    return `${c.name} ${ty}${nn}`;
  });
  if (surrogate !== undefined) {
    let sql = `CREATE TABLE IF NOT EXISTS ${name} (${surrogate} INTEGER PRIMARY KEY, ${col_defs.join(", ")}`;
    if (pk.length > 0) sql += `, UNIQUE (${pk.join(", ")})`;
    sql += ")";
    await db.executeMultiple(sql);
    return;
  }
  let sql = `CREATE TABLE IF NOT EXISTS ${name} (${col_defs.join(", ")}`;
  if (pk.length > 0) sql += `, PRIMARY KEY (${pk.join(", ")})`;
  sql += ")";
  if (pk.length > 0) sql += " WITHOUT ROWID";
  await db.executeMultiple(sql);
}

/** Drop a rel table (an evicted rel leaves zero bytes). */
export async function drop_rel_table(db: SqliteDb, name: string): Promise<void> {
  await db.executeMultiple(`DROP TABLE IF EXISTS ${name}`);
}
}

// ---- unfuck_sqlite (the pragma bundle a fresh store opens with) ------------
export namespace unfuck_sqlite {
/**
 * Pragmas a fresh store opens with:
 *  - foreign_keys=ON  — FKs enforced (owner law).
 *  - journal_mode=WAL — concurrent readers while the writer ticks.
 *  - synchronous=NORMAL, temp_store=MEMORY — the ast-grep/biome-speed knobs.
 */
export const OPEN_PRAGMAS =
  "PRAGMA journal_mode=WAL;PRAGMA synchronous=NORMAL;PRAGMA foreign_keys=ON;PRAGMA temp_store=MEMORY;";
}

/** Top-level re-export so `import { OPEN_PRAGMAS } from "./spine.ts"` resolves. */
export const OPEN_PRAGMAS = unfuck_sqlite.OPEN_PRAGMAS;

// ---- dataflow proofs (./types.ts) -------------------------------------------
export type RelTableApiHolds = AssertTrue<typeof rels extends IRelTableApi ? true : false>;
export type CreateAllTablesHolds = AssertTrue<typeof create_all_tables extends CreateAllTables ? true : false>;
