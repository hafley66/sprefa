/**
 * decl -> SQLite DDL: rel_* current tables, delta log, effect_cache, store_meta.
 *
 * `ddl(decls, retention) -> string[]`.
 *   rel_<name>(cols..., PRIMARY KEY(all cols))  -- set semantics
 *   delta(rel, row_digest, tick, weight)
 *   effect_cache(full_digest PK, identity_digest, host, state, requested_tick)
 *     + an index on identity_digest: full_digest is the fire-once key (host +
 *     identity cols + witness/salt cols); identity_digest (host + identity cols
 *     only) groups every witness of one identity, the supersession group
 *     HostRunner reads by.
 *   store_meta(key PK, value)  -- the monotone 'tick' counter row
 * Row identity = full column tuple; row_digest is folded via 0_digest.ts's
 * foldRowDigest (the one shared fold; do not re-implement it).
 *
 * `retention` is accepted but does not change table SHAPE (every rel_* table is
 * the same untyped set-semantics table regardless of 0/1/all): retention is a
 * RUNTIME purge/replace policy enforced by 3_runtime.ts, not a schema difference.
 */
import type { RelDecl } from "sprefa-store-engine/src/lower/ast.ts";
import { rels as rel_tables } from "sprefa-store-engine/src/engine/spine.ts";
import { foldRowDigest } from "./0_digest.ts";
import type { AssertTrue, ColumnType, Ddl, Retention, Row } from "./0_types.ts";

/**
 * The dense-integer surrogate key column every `relbase_*` table carries. It IS the
 * table's rowid (`row_id INTEGER PRIMARY KEY`), so ids run 1..n and SQLite stores them as
 * a min-bit varint. Nothing outside a rel table ever names a row by its column tuple or
 * by a hash of it: `cascade.key(rel_tag, row_id)` packs the dense rel tag and this dense
 * row id into one i64, which is the address the Z-set fact plane (`cx_row`/`cx_dep`) uses.
 */
export const ROW_SURROGATE = "row_id";

/** The physical fact-table columns are all INTEGER; text values are dictionary ids. */
export function relBaseColumns(
  decl: RelDecl,
  columnTypes: ReadonlyMap<string, readonly ColumnType[]>,
): rel_tables.RelCol[] {
  const types = columnTypes.get(decl.name);
  if (!types || types.length !== decl.columns.length) {
    throw new Error(`ddl: missing column types for rel '${decl.name}'`);
  }
  return decl.columns.map((name) => rel_tables.rel_col_int(name));
}

/** The read view restores the text surface from dictionary ids. */
export function relViewSql(decl: RelDecl, columnTypes: ReadonlyMap<string, readonly ColumnType[]>): string {
  const types = columnTypes.get(decl.name);
  if (!types || types.length !== decl.columns.length) {
    throw new Error(`ddl: missing column types for rel '${decl.name}'`);
  }
  const selectList = decl.columns
    .map((column, index) => {
      if (types[index] === "int") return column;
      return `CASE WHEN ${column} = -1 THEN NULL ELSE (SELECT content FROM strings WHERE string_id = ${column}) END AS ${column}`;
    })
    .join(", ");
  // The dense surrogate rides along so a reader that holds a packed fact key can resolve
  // it back to a row without dropping to the physical table.
  return `CREATE VIEW IF NOT EXISTS rel_${decl.name} AS SELECT ${ROW_SURROGATE}, ${selectList} FROM relbase_${decl.name}`;
}

/** decl -> [delta, effect_cache, store_meta, tick seed, read views]. */
export function ddl(
  decls: readonly RelDecl[],
  retention: ReadonlyMap<string, Retention>,
  columnTypes: ReadonlyMap<string, readonly ColumnType[]> = new Map(
    decls.map((decl) => [decl.name, decl.columns.map(() => "text" as const)]),
  ),
): string[] {
  void retention; // shape-only param this arc; runtime reads retention for tick behavior.
  const statements: string[] = [];
  // rel_tag: the dense rel tag (0..n-1, declaration order) -> the rel name's dictionary id.
  // Readers resolve a tag to a name through here, so no other table has to carry text or a
  // sparse dictionary id in a key position.
  statements.push(
    "CREATE TABLE IF NOT EXISTS rel_tag (tag INTEGER PRIMARY KEY, name_id INTEGER NOT NULL)",
  );
  statements.push(
    "CREATE TABLE IF NOT EXISTS delta (rel_tag INTEGER NOT NULL, row_digest INTEGER NOT NULL, tick INTEGER NOT NULL, weight INTEGER NOT NULL)",
  );
  statements.push(
    "CREATE TABLE IF NOT EXISTS effect_cache (full_digest INTEGER PRIMARY KEY, identity_digest INTEGER NOT NULL, host TEXT NOT NULL, state TEXT NOT NULL, requested_tick INTEGER NOT NULL)",
  );
  statements.push("CREATE INDEX IF NOT EXISTS idx_effect_cache_identity ON effect_cache(identity_digest)");
  statements.push("CREATE TABLE IF NOT EXISTS store_meta (key TEXT PRIMARY KEY, value)");
  statements.push("INSERT INTO store_meta(key,value) VALUES ('tick',0) ON CONFLICT(key) DO NOTHING");
  for (const decl of decls) statements.push(relViewSql(decl, columnTypes));
  return statements;
}

/** A row's digest, via the shared fold in 0_digest.ts (also used by 1_hosts.ts's
 *  effectDigest). */
export function rowDigest(row: Row, columns: readonly string[]): number {
  return foldRowDigest(row, columns);
}

// ---- dataflow proof (src/0_types.ts) -----------------------------------------
export type DdlHolds = AssertTrue<typeof ddl extends Ddl ? true : false>;
