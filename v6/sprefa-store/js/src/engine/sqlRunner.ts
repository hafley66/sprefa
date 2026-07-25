/**
 * sqlRunner.ts — the single place a Promise enters the engine.
 *
 * `db.execute` is the only real asynchrony here. It is wrapped once, inside `defer`, so
 * every statement is cold until subscribed and every caller above works in observables.
 * Counting and tracing live here too: a statement cannot be run through this module and
 * escape either one.
 */

import { Observable, catchError, concatMap, defer, from, map, of, throwError } from "rxjs";

import { stmt_counter } from "./counter.ts";
import type { ISqlRunner, QueryResult, SqliteDb, SqlStatement, TraceStatement } from "./types.ts";

export const SqlRunner: ISqlRunner = {
  execute(db: SqliteDb, statement: SqlStatement, trace?: TraceStatement): Observable<QueryResult> {
    return defer(() => {
      stmt_counter.incr();
      trace?.(typeof statement === "string" ? statement : statement.sql);
      return from(db.execute(statement));
    });
  },

  run(db: SqliteDb, statement: SqlStatement, trace?: TraceStatement): Observable<void> {
    return SqlRunner.execute(db, statement, trace).pipe(map(() => undefined));
  },

  scalar(db: SqliteDb, statement: SqlStatement, trace?: TraceStatement): Observable<number> {
    return SqlRunner.execute(db, statement, trace).pipe(map((result) => Number(result.rows[0]?.[0] ?? 0)));
  },

  inTransaction<Value>(db: SqliteDb, body: () => Observable<Value>): Observable<Value> {
    return bracket(db, "BEGIN IMMEDIATE").pipe(
      concatMap(body),
      concatMap((value) => bracket(db, "COMMIT").pipe(map(() => value))),
      catchError((failure: unknown) =>
        bracket(db, "ROLLBACK").pipe(
          catchError(() => of(undefined)),
          concatMap(() => throwError(() => failure)),
        ),
      ),
    );
  },
};

/**
 * BEGIN / COMMIT / ROLLBACK, deliberately NOT counted. They are bracket overhead, not
 * data statements, and the N+1 golden tests assert an exact count of the latter.
 */
function bracket(db: SqliteDb, statement: string): Observable<void> {
  return defer(() => from(db.execute(statement))).pipe(map(() => undefined));
}
