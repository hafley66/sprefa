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

import type { IBootStatement, IBootRunner, ISqlSeam } from "./types.ts";

export const BootRunner: IBootRunner = {
  run(seam: ISqlSeam, statements: readonly IBootStatement[]): Observable<void> {
    if (statements.length === 0) return of(undefined);
    return from(statements).pipe(
      concatMap((statement) =>
        seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] }),
      ),
      toArray(),
      map(() => undefined),
    );
  },
};
