/**
 * Run a compiled program's `boot` statements (the Initial-row
 * seed plus the t=0 level closure) once, after DDL and before the tick fold.
 *
 * The integer-to-bigint conversion is required because
 * `@libsql/client` binds safe integer JS numbers as REAL.
 */

import { concatMap, from, map, of, toArray, type Observable } from "rxjs";

import type { IBootStatement, IBootRunner, IRowScalar, ISqlSeam } from "./types.ts";

/** Integer params cross as bigint. `@libsql/client` binds a JS number as
 *  SQLite REAL, so a bound `1` lands in a TEXT-affinity column as "1.0" while
 *  `1n` lands as "1" (tests/bootBind.test.ts measures both). Identical rule to
 *  emit_ts.pl's emitted `bindArgs` helper and 1_incremental.ts's own; the boot
 *  path was the one seam still binding raw. Harmless against an INTEGER
 *  column, which round-trips either form. */
function boot_args(params: readonly IRowScalar[]): (string | number | bigint)[] {
  return params.map((param) => {
    if (typeof param === "boolean") return BigInt(param ? 1 : 0);
    if (typeof param === "number") return Number.isSafeInteger(param) ? BigInt(param) : param;
    if (typeof param === "string") return param;
    throw new Error("a list param reached a SQL parameter");
  });
}

export const BootRunner: IBootRunner = {
  run(seam: ISqlSeam, statements: readonly IBootStatement[]): Observable<void> {
    if (statements.length === 0) return of(undefined);
    return from(statements).pipe(
      concatMap((statement) =>
        seam.runner.execute(seam.db, { sql: statement.sql, args: boot_args(statement.params) }),
      ),
      toArray(),
      map(() => undefined),
    );
  },
};
