/**
 * 2_binds.ts — LIVE `bind interval(...)` execution (scope 1 of the
 * runtime-bridge arc). RX-B1 from the hosts-extraction verdict, made real:
 *
 *   interval(periodMs, scheduler) -> map(toBucketRow) -> mergeMap(submit)
 *
 * WHICH PERIODS. The bind DECLARATION authorizes a world source; the program's
 * own RULES say which cadence it consumes, as integer literals in the bind
 * atom's first column (`interval(300, Bucket)`). emit_ts.pl collects those
 * literals into the emitted plan's `periods`, so this file spins exactly the
 * timers the program asked for and no default. A program that declares the
 * bind and reads no literal period gets `periods: []` and therefore no timer
 * at all -- an honest zero, never an invented cadence.
 *
 * This closes v6/dl's own recorded gap in passing: `1_binds.ts` reads its
 * `clock_period` config from a rel ONCE at subscribe time, so a period added
 * mid-run spins nothing until reload. Here the cadence is a COMPILE-TIME fact
 * of the program text, so "read once per program load" is not a gap, it is the
 * whole truth: a new period is a new program.
 *
 * BUCKET LAW, unchanged from the shipped clock bind: floor(epoch_secs/period),
 * never a process-local counter, so a restart does not replay bucket 0,1,2...
 * The scheduler seam moves the FIRING; the bucket VALUE stays real wall clock
 * (a TestScheduler run therefore produces real-clock bucket values, which is
 * why the receipt asserts firing COUNT and column shape, not bucket numbers).
 *
 * TEARDOWN is unsubscription and nothing else: an rxjs `interval`'s unsubscribe
 * IS the underlying clearInterval. A program swap's `switchMap` therefore stops
 * every timer with no Subscription field and no dispose() here.
 */

import { EMPTY, type Observable, type SchedulerLike, asyncScheduler, interval, map, merge, mergeMap, take } from "rxjs";

import type {
  IArrivalRow,
  IBindFired,
  IBindPlan,
  IIntervalBindRunner,
  ILiveEngine,
  IRowValue,
} from "../runtime/types.ts";
import { ServeTrace } from "./0_trace.ts";

function bucketFor(periodSecs: number): number {
  return Math.floor(Date.now() / 1000 / periodSecs);
}

/** The `interval` bind's row is (period, bucket) by registry definition
 *  (registry.pl `bind_definition/2`), so the row is built positionally from the
 *  plan's own declared columns: first column carries the period, second the
 *  bucket. A future bind with another column shape needs its own row builder
 *  and would be refused by name below rather than filled by guess. */
function bucketRow(plan: IBindPlan, periodSecs: number): readonly IRowValue[] {
  return plan.columns.map((_column, index) => (index === 0 ? periodSecs : bucketFor(periodSecs)));
}

export class IntervalBindRunner implements IIntervalBindRunner {
  readonly firings$: Observable<IBindFired>;

  constructor(
    engine: ILiveEngine,
    plans: readonly IBindPlan[],
    scheduler: SchedulerLike = asyncScheduler,
  ) {
    const timers = plans.flatMap((plan) => {
      if (plan.execution !== "live_interval") {
        throw new Error(`unknown bind executor '${plan.execution}' for bind '${plan.name}'`);
      }
      if (plan.columns.length !== 2) {
        throw new Error(`bind '${plan.name}' has ${plan.columns.length} columns; the interval row shape is (period, bucket)`);
      }
      return plan.periods.map((periodSecs) =>
        interval(periodSecs * 1000, scheduler).pipe(
          map((): IArrivalRow => ({ rel: plan.name, sign: "add", row: bucketRow(plan, periodSecs) })),
          mergeMap((arrival) =>
            // One firing is one reported value: `take(1)` keeps the SETTLE tick
            // and lets any drain ticks the batch causes flow out on `ticks$`
            // like every other tick. The batch itself is already enqueued in
            // the engine, so completing early cannot cancel it.
            engine.submit([arrival]).pipe(
              take(1),
              map((outcome): IBindFired => {
                const bucket = Number(arrival.row[1] ?? 0);
                ServeTrace.bind(plan.name, periodSecs, bucket);
                return { rel: plan.name, period: periodSecs, bucket, tick: outcome.tick };
              }),
            ),
          ),
        ),
      );
    });
    this.firings$ = timers.length > 0 ? merge(...timers) : EMPTY;
  }
}
