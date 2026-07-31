/**
 * rows.ts — runs a SELECT through the driver seam and shapes the result back
 * into plain `IRow[]` (columns in the caller's declared order). Shared by
 * every gen/*.ts tick chain; the SQL TEXT itself is per-rule and lives in
 * gen/*.ts, this only unwraps the QueryResult.
 */

import { catchError, map, type Observable } from "rxjs";

import type {
  IRow,
  IRowColumnType,
  IRowValue,
  IRowValueFromSql,
  ISelectRows,
  ISqlSeam,
} from "./types.ts";

/** A generated program's scalar columns are TEXT, INTEGER, or REAL and never
 *  NULL. Bool columns use constrained INTEGER storage and cross this boundary
 *  as booleans; float columns use finite REAL storage. `IRowValue` covers every value a
 *  SELECT can hand back here after the bigint driver seam is normalized:
 *  safe INTEGER values become JS numbers and unsafe values receive the
 *  named int_out_of_range refusal. TEXT-affinity columns cross as strings.
 *  The cast stays narrow (libsql's `Value` is `null | string | number |
 *  bigint | ArrayBuffer`) and would need widening only if a future column
 *  type introduces `null` into this seam. */
export const rowValueFromSql: IRowValueFromSql = (type: IRowColumnType | undefined, value: unknown): IRowValue => {
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
  if (type === "int") {
    if (typeof value === "bigint") {
      if (value < -9007199254740991n || value > 9007199254740991n) {
        throw new Error("int_out_of_range");
      }
      return Number(value);
    }
    if (typeof value === "number" && !Number.isSafeInteger(value)) {
      throw new Error("int_out_of_range");
    }
  }
  return value as IRowValue;
};

/** The driver's own answer to an integer past 2^53-1. @libsql/client runs with
 *  the default `intMode: "number"`, so a SQLite INTEGER it cannot represent
 *  makes the CONVERSION throw, before any value reaches `rowValueFromSql`.
 *  A raw `RangeError` names no rel, no column and no statement, which is the
 *  one unnamed emitter failure the type matrix still measured.
 *
 *  Ruling wide_int_fate = refuse_everywhere_with_todo (user 2026-07-31): the
 *  answer is the SAME named refusal the arrival gate gives, at every reach
 *  point, including this read-back one. An arrival can no longer carry such an
 *  integer in; what still arrives here is an integer SQL COMPUTED (a sum, a
 *  json_extract out of a document, a bigint bind), so the statement text is
 *  what says where it came from.
 *
 *  TODO(bigint): the other half of the ruling is not taken. Carrying
 *  arbitrary-precision integers end to end needs `intMode: "bigint"` (reverted
 *  once already: a global bigint leaks into JSON.stringify at every raw row
 *  projection), a bigint column type, and a tick-log encoding for values a
 *  JSON number cannot hold. User: "not finance yet". The prolog-side twin of
 *  this comment is at 0_type_plane.pl:wide_integer_witness/2. */
const isWideIntegerRangeError = (error: unknown): boolean =>
  error instanceof RangeError && /safely represented|out of range/i.test(error.message);

/** Bound to `ISelectRows` rather than folded into a namespace object: emitted
 *  modules import this name directly (137 of them), and the import text comes
 *  from the prolog emitter. The annotation is what buys the compiler check. */
export const selectRows: ISelectRows = (
  seam: ISqlSeam,
  sql: string,
  columns: readonly string[],
  columnTypes: readonly IRowColumnType[] = [],
): Observable<IRow[]> => {
  return seam.runner.execute(seam.db, sql).pipe(
    catchError((error: unknown) => {
      if (!isWideIntegerRangeError(error)) throw error;
      throw new Error(`int_out_of_range reading ${sql}`);
    }),
    map((result) =>
      result.rows.map(
        (row): IRow =>
          columns.map((column, index) => rowValueFromSql(columnTypes[index], row[column])),
      ),
    ),
  );
};
