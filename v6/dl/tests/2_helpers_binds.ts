/**
 * tests/2_helpers_binds.ts -- shared bind-test setup, mirroring tests/2_helpers_hosts.ts
 * exactly (same numbering reasoning: imports tests/1_helpers_db.ts + src/1_binds.ts,
 * sits below tests/4_binds.test.ts which also exercises the runtime).
 *
 * Exports: bootBindRunnerFixture/disposeBindFixture (boot a DlRuntime + a BindRunner
 * over it, subscribed the same way main.ts composes the real app graph),
 * advanceVirtualSeconds (drive an injected rxjs TestScheduler forward by whole
 * seconds -- the shared helper both 4_binds.test.ts and 6_binds_http.test.ts use to
 * run a bind's cadence on virtual time instead of a real setTimeout sleep),
 * waitForScheduledAction (the bounded real poll both of those files need once,
 * right after activating a scheduler-driven bind, before the first flush).
 */
import { asyncScheduler, merge, type SchedulerLike, type Subscription, type VirtualTimeScheduler } from "rxjs";

import { BindRunner, type BindDef } from "../src/1_binds.ts";
import { DlRuntime } from "../src/3_runtime.ts";
import type { BridgeOk } from "../tasks.d.ts";
import { cleanupDbFile, freshDbPath } from "./1_helpers_db.ts";
import { waitUntil } from "./2_helpers_hosts.ts";

export interface BindFixture {
  readonly rt: DlRuntime;
  readonly runner: BindRunner;
  readonly dbPath: string;
  /** The test's stand-in for main.ts's terminal subscription: it runs the tick loop
   *  and the runner's commits for the life of the fixture. Merged in that order, same
   *  as bootHostRunnerFixture, so deltas$ is live before the runner's own source$
   *  observables start firing. */
  readonly running: Subscription;
}

/** `scheduler` defaults to rxjs's own `asyncScheduler` (real wall-clock, matching
 *  production) -- a test that wants a bind's cadence on virtual time passes an rxjs
 *  `TestScheduler`/`VirtualTimeScheduler` instead (0_types.ts's BindConfig seam). */
export async function bootBindRunnerFixture(
  bridgeOk: BridgeOk,
  binds: readonly BindDef[],
  scheduler: SchedulerLike = asyncScheduler,
): Promise<BindFixture> {
  const dbPath = freshDbPath();
  const rt = await DlRuntime.boot({ dbPath, bridge: bridgeOk });
  const runner = new BindRunner(rt, binds, bridgeOk.program, scheduler);
  const running = merge(rt.deltas$, runner.commits$).subscribe({
    error: (failure: unknown) => {
      throw failure;
    },
  });
  return { rt, runner, dbPath, running };
}

export async function disposeBindFixture(fixture: BindFixture): Promise<void> {
  fixture.running.unsubscribe();
  await fixture.rt.dispose();
  cleanupDbFile(fixture.dbPath);
}

/** Drives an injected `VirtualTimeScheduler`-shaped scheduler (rxjs's `TestScheduler`
 *  extends it) forward by `seconds` worth of virtual frames and flushes its action
 *  queue. `interval(ms, scheduler)`'s frame unit IS milliseconds (rxjs's own
 *  `AsyncScheduler`/`VirtualTimeScheduler` convention), so `seconds * 1000` lines up
 *  with `clockIntervalFor`'s `periodSecs * 1000` directly -- no unit invented here.
 *  Bounded on purpose (`maxFrames` advances by a finite amount, never `Infinity`):
 *  `VirtualTimeScheduler.flush()` executes every queued action whose delay is
 *  `<= maxFrames`, and a live `interval` reschedules itself each firing, so an
 *  unbounded `maxFrames` on an active interval would spin forever. */
export function advanceVirtualSeconds(scheduler: VirtualTimeScheduler, seconds: number): void {
  scheduler.maxFrames = scheduler.frame + seconds * 1000;
  scheduler.flush();
}

/** clockBind's `source$` does one ASYNC boot-scan read (`config.runtime.rows(
 *  "clock_period")`, 1_binds.ts) before it ever subscribes `interval(ms, scheduler)`
 *  -- that read resolves on the real event loop (a real, if fast, DB round trip),
 *  never on the injected virtual scheduler. So `interval`'s recurring action does
 *  not exist on `scheduler.actions` the instant a program finishes loading; flushing
 *  before it lands is a no-op that advances nothing. This is a short, bounded REAL
 *  poll for that one async registration to land -- not a stand-in for the cadence
 *  itself (which stays on virtual time throughout). Also covers the teardown/reload
 *  direction: waiting for `scheduler.actions.length === 0` after an unsubscribe or a
 *  program swap, since the same async gap applies to the OLD interval's removal when
 *  it is driven by switchMap rather than a direct `.unsubscribe()` call. */
export async function waitForScheduledAction(
  scheduler: VirtualTimeScheduler,
  predicate: (actionCount: number) => boolean,
): Promise<void> {
  await waitUntil(async () => (predicate(scheduler.actions.length) ? true : undefined));
}
