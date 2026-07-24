/**
 * 2_schema.ts - decl -> SQLite DDL: rel_* current tables, delta log, effect_cache,
 * store_meta.
 *
 * Contract (plan M2, tasks.d.ts): `ddl(decls, retention) -> string[]`.
 *   rel_<name>(cols..., PRIMARY KEY(all cols))     -- set semantics
 *   delta(rel, row_digest, tick, weight)           -- tick shape (b), pinned in DECISIONS
 *   effect_cache(digest PK, host, state, requested_tick)
 *   store_meta(key PK, value)                      -- the monotone 'tick' counter row
 * Row identity = full column tuple; row_digest = oracle.mix XOR law (ingest.ts note 6),
 * reused verbatim via content_digest (do not re-implement the fold).
 *
 * `retention` is accepted per the contract but does not change table SHAPE (every rel_*
 * table is the same untyped set-semantics table regardless of 0/1/all): retention is a
 * RUNTIME purge/replace policy enforced by 3_runtime.ts, not a schema difference.
 */
import { content_digest } from "sprefa-store-engine/src/engine/ingest.ts";
import type { RelDecl } from "sprefa-store-engine/src/lower/ast.ts";
import type { Retention, Row, Value } from "../tasks.d.ts";

/** decl -> [CREATE TABLE rel_*..., delta, effect_cache, store_meta, tick seed]. */
export function ddl(decls: readonly RelDecl[], retention: ReadonlyMap<string, Retention>): string[] {
  void retention; // shape-only param this arc; runtime reads retention for tick behavior.
  const statements: string[] = [];
  for (const decl of decls) {
    const columnList = decl.columns.join(", ");
    statements.push(
      `CREATE TABLE IF NOT EXISTS rel_${decl.name} (${columnList}, PRIMARY KEY (${columnList}))`,
    );
  }
  statements.push(
    "CREATE TABLE IF NOT EXISTS delta (rel TEXT NOT NULL, row_digest INTEGER NOT NULL, tick INTEGER NOT NULL, weight INTEGER NOT NULL)",
  );
  statements.push(
    "CREATE TABLE IF NOT EXISTS effect_cache (digest TEXT PRIMARY KEY, host TEXT NOT NULL, state TEXT NOT NULL, requested_tick INTEGER NOT NULL)",
  );
  statements.push("CREATE TABLE IF NOT EXISTS store_meta (key TEXT PRIMARY KEY, value)");
  statements.push("INSERT INTO store_meta(key,value) VALUES ('tick',0) ON CONFLICT(key) DO NOTHING");
  return statements;
}

/** A named Value, folded via the one hash law every plane agrees on (content_digest),
 *  then narrowed to a 53-bit-safe JS number: asUintN(64) makes the signed fold unsigned,
 *  asUintN(53) narrows to what a JS number holds exactly, per the pinned contract. */
export function rowDigest(row: Row, columns: readonly string[]): number {
  const parts = columns.map((column) => valueToDigestPart(row[column] ?? null));
  const folded = content_digest(parts);
  return Number(BigInt.asUintN(53, BigInt.asUintN(64, folded)));
}

function valueToDigestPart(value: Value): number | string | null {
  if (typeof value === "boolean") return value ? 1 : 0;
  return value;
}
