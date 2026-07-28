/**
 * rows.ts — runs a SELECT through the driver seam and shapes the result back
 * into plain `IRow[]` (columns in the caller's declared order). Shared by
 * every gen/*.ts tick chain; the SQL TEXT itself is per-rule and lives in
 * gen/*.ts, this only unwraps the QueryResult.
 */

import { map, type Observable } from "rxjs";

import type { IRow, IRowValue, ISqlSeam } from "./types.ts";

/** Every column this fixture pair stores is TEXT (no ints, no nulls); the
 *  cast is narrow (libsql's `Value` is `null | string | number | bigint |
 *  ArrayBuffer`) and would need widening the day a gen file adds an int
 *  column. */
export function selectRows(seam: ISqlSeam, sql: string, columns: readonly string[]): Observable<IRow[]> {
  return seam.runner.execute(seam.db, sql).pipe(
    map((result) => result.rows.map((row): IRow => columns.map((column) => row[column] as IRowValue))),
  );
}
