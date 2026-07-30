/**
 * rows.ts — runs a SELECT through the driver seam and shapes the result back
 * into plain `IRow[]` (columns in the caller's declared order). Shared by
 * every gen/*.ts tick chain; the SQL TEXT itself is per-rule and lives in
 * gen/*.ts, this only unwraps the QueryResult.
 */

import { map, type Observable } from "rxjs";

import type { IRow, IRowColumnType, IRowValue, ISqlSeam } from "./types.ts";

/** A generated program's scalar columns are TEXT, INTEGER, or REAL and never
 *  NULL. Bool columns use constrained INTEGER storage and cross this boundary
 *  as booleans; float columns use finite REAL storage. `IRowValue` covers every value a
 *  SELECT can hand back here without a runtime check: libsql returns an
 *  INTEGER-affinity column as a plain JS `number` (verified empirically,
 *  not assumed -- this driver's default `intMode` is "number", not
 *  "bigint") and a TEXT-affinity column as a JS `string`. The cast stays
 *  narrow (libsql's `Value` is `null | string | number | bigint |
 *  ArrayBuffer`) and would need widening only if a future column type
 *  introduces `bigint` or `null` into this seam. */
export function rowValueFromSql(type: IRowColumnType | undefined, value: unknown): IRowValue {
  if (type === "bool") {
    if (value === 0 || value === 0n) return false;
    if (value === 1 || value === 1n) return true;
    throw new Error(`bool column crossed SQLite with ${JSON.stringify(value)}`);
  }
  if (type === "float") {
    if (typeof value !== "number" || !Number.isFinite(value)) {
      throw new Error(`float column crossed SQLite with ${JSON.stringify(value)}`);
    }
    return Object.is(value, -0) ? 0 : value;
  }
  return value as IRowValue;
}

export function selectRows(
  seam: ISqlSeam,
  sql: string,
  columns: readonly string[],
  columnTypes: readonly IRowColumnType[] = [],
): Observable<IRow[]> {
  return seam.runner.execute(seam.db, sql).pipe(
    map((result) =>
      result.rows.map(
        (row): IRow =>
          columns.map((column, index) => rowValueFromSql(columnTypes[index], row[column])),
      ),
    ),
  );
}
