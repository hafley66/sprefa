/**
 * 2_boot.ts — runs a compiled program's `boot` statements (the Initial-row
 * seed plus the t=0 level closure) once, after DDL and before the tick fold.
 *
 * Extracted from the two harnesses that each had their own private copy of
 * this loop (scripts/sweep.ts, scripts/run-emitted.ts): both spread
 * `statement.params` straight into `args`, which is the one bind path in the
 * package that did NOT apply the integer -> bigint conversion every other
 * path applies. See IBootRunner in types.ts for why that conversion is not
 * optional, and tests/bootBind.test.ts for the measured receipt.
 */

import { concatMap, from, map, of, toArray, type Observable } from "rxjs";

import type { IBootStatement, IBootRunner, IRowValue, ISqlSeam } from "./types.ts";

/** Integer params cross as bigint. `@libsql/client` binds a JS number as
 *  SQLite REAL, so a bound `1` lands in a TEXT-affinity column as "1.0" while
 *  `1n` lands as "1" (tests/bootBind.test.ts measures both). Identical rule to
 *  emit_ts.pl's emitted `bindArgs` helper and 1_incremental.ts's own; the boot
 *  path was the one seam still binding raw. Harmless against an INTEGER
 *  column, which round-trips either form. */
function bootArgs(params: readonly IRowValue[]): (string | number | bigint)[] {
  return params.map((param) =>
    typeof param === "boolean"
      ? BigInt(param ? 1 : 0)
      : typeof param === "number" && Number.isSafeInteger(param)
        ? BigInt(param)
        : param,
  );
}

export const BootRunner: IBootRunner = {
  run(seam: ISqlSeam, statements: readonly IBootStatement[]): Observable<void> {
    if (statements.length === 0) return of(undefined);
    return from(statements).pipe(
      concatMap((statement) =>
        seam.runner.execute(seam.db, { sql: statement.sql, args: bootArgs(statement.params) }),
      ),
      toArray(),
      map(() => undefined),
    );
  },
};
