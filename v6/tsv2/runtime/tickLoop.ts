/**
 * tickLoop.ts — the ONE generic algorithm every gen/*.ts program's `tick()`
 * runs under: fold the arrival schedule through the program, one call per
 * tick, then keep calling with an empty batch while the program reports
 * `carryPending` (engine.pl q4/R2's next-tick trigger carry), capped at
 * `drainCap` extra ticks (engine.pl `drain_cap(100)`). Zero per-program
 * text — the only knowledge here is the fold shape itself; every DDL/SQL/
 * rule string lives in gen/*.ts.
 *
 * `expand` is the one new rx operator this arc adds (rxjs rule of
 * engagement): it is rxjs's built-in shape for "keep re-subscribing to a
 * projection while it keeps emitting," which is exactly the schedule+drain
 * fold — no Subject, no second manual `.subscribe()`.
 */

import { EMPTY, expand, filter, map, type Observable, of } from "rxjs";

import { TickLogEmitter } from "./ticklog.ts";
import type { IArrivalBatch, IGenProgram, ISqlSeam, ITickDeltas, ITickFold, ITickLogLine } from "./types.ts";

/** Private fold state, not part of the runtime's public contract — a `type`
 *  alias (not an `interface`), so the I-prefix law does not apply. */
type FoldStep = {
  readonly tick_number: number; // 0 = boot, nothing emitted yet
  readonly deltas: ITickDeltas | null;
  readonly drains_used: number;
};

function next_arrivals(step: FoldStep, schedule: readonly IArrivalBatch[]): IArrivalBatch | undefined {
  if (step.tick_number < schedule.length) return schedule[step.tick_number];
  return step.deltas?.carry_pending ? [] : undefined;
}

function has_deltas(step: FoldStep): step is FoldStep & { deltas: ITickDeltas } {
  return step.deltas !== null;
}

export const TickFold: ITickFold = {
  run(
    program: IGenProgram,
    seam: ISqlSeam,
    schedule: readonly IArrivalBatch[],
    drain_cap = 100,
  ): Observable<ITickLogLine> {
    const boot: FoldStep = { tick_number: 0, deltas: null, drains_used: 0 };
    return of(boot).pipe(
      expand((step) => {
        const arrivals = next_arrivals(step, schedule);
        if (arrivals === undefined) return EMPTY;
        const is_drain_tick = step.tick_number >= schedule.length;
        if (is_drain_tick && step.drains_used >= drain_cap) {
          throw new Error(`tsv2 drain overflow: ${program.name} exceeded ${drain_cap} drain ticks`);
        }
        return program.tick(seam, arrivals).pipe(
          map(
            (deltas): FoldStep => ({
              tick_number: step.tick_number + 1,
              deltas,
              drains_used: is_drain_tick ? step.drains_used + 1 : step.drains_used,
            }),
          ),
        );
      }),
      filter(has_deltas),
      map((step) => TickLogEmitter.line(step.tick_number, step.deltas, program.rel_column_types)),
    );
  },
};
