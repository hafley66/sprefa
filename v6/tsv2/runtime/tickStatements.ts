/** tickStatements.ts — per-tick SQL cost. `SqlRunner` increments
 *  `stmt_counter` on every method, so a tick's cost is that counter's delta. */

import { defer, type Observable, tap } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import type { ITickDeltas, ITickStatementCount, ITickStatementLedger } from "./types.ts";

/** A served process ticks without bound, so the per-tick list is a ring. The
 *  running total is kept apart from it and stays exact past the cap. */
const RECENT_TICK_CAP = 1024;

const recent: ITickStatementCount[] = [];
let running: ITickStatementCount = { tick: 0, statements: 0, adds: 0, dels: 0 };

function sum(deltas: ITickDeltas, side: "add" | "del"): number {
  return deltas.rels.reduce((carried, rel) => carried + rel[side].length, 0);
}

function accumulate(entry: ITickStatementCount): void {
  recent.push(entry);
  if (recent.length > RECENT_TICK_CAP) recent.shift();
  running = {
    tick: running.tick + 1,
    statements: running.statements + entry.statements,
    adds: running.adds + entry.adds,
    dels: running.dels + entry.dels,
  };
}

export const TickStatementLedger: ITickStatementLedger = {
  measure(tick: number, deltas: Observable<ITickDeltas>): Observable<ITickDeltas> {
    return defer(() => {
      const opened_at = stmt_counter.get();
      return deltas.pipe(
        tap((tick_deltas) => {
          accumulate({
            tick,
            statements: stmt_counter.get() - opened_at,
            adds: sum(tick_deltas, "add"),
            dels: sum(tick_deltas, "del"),
          });
        }),
      );
    });
  },

  reset(): void {
    recent.length = 0;
    running = { tick: 0, statements: 0, adds: 0, dels: 0 };
  },

  entries(): readonly ITickStatementCount[] {
    return recent.slice();
  },

  total(): ITickStatementCount {
    return running;
  },
};
