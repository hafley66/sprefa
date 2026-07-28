/**
 * rows.ts — runs a SELECT through the driver seam and shapes the result back
 * into plain `IRow[]` (columns in the caller's declared order). Shared by
 * every gen/*.ts tick chain; the SQL TEXT itself is per-rule and lives in
 * gen/*.ts, this only unwraps the QueryResult.
 */

import { map, type Observable } from "rxjs";

import type { IRow, IRowValue, ISqlSeam } from "./types.ts";

/** A generated program's columns are TEXT or INTEGER (PHASE C2 RULING 1;
 *  never NULL), so `IRowValue` (string | number) covers every value a
 *  SELECT can hand back here without a runtime check: libsql returns an
 *  INTEGER-affinity column as a plain JS `number` (verified empirically,
 *  not assumed -- this driver's default `intMode` is "number", not
 *  "bigint") and a TEXT-affinity column as a JS `string`. The cast stays
 *  narrow (libsql's `Value` is `null | string | number | bigint |
 *  ArrayBuffer`) and would need widening only if a future column type
 *  introduces `bigint` or `null` into this seam. */
export function selectRows(seam: ISqlSeam, sql: string, columns: readonly string[]): Observable<IRow[]> {
  return seam.runner.execute(seam.db, sql).pipe(
    map((result) => result.rows.map((row): IRow => columns.map((column) => row[column] as IRowValue))),
  );
}
