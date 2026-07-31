/**
 * Run a SELECT through the driver seam and shape the result back
 * into plain `IRow[]` (columns in the caller's declared order). Shared by
 * into `IRow[]` by declared column order.
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
 *  The same named refusal is used for values computed by SQL.
 */
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
