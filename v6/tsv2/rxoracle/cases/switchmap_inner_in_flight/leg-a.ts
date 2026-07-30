/**
 * switchmap_inner_in_flight -- leg A, literal rxjs. THE FLAGSHIP.
 *
 * Imports rxjs and nothing from this repository.
 *
 * Step schedule, mirroring cases/switchmap_inner_in_flight/case.json. stepMs is
 * 1000, each batch is delivered at its step's MIDPOINT (frame k*1000+500), and
 * the fetch takes 1950 virtual ms (leg B's `RXO_NAP`).
 *
 *   step 0 @ 500   route s1 -> r1. inner starts, would deliver at 2450.
 *   step 1 @ 1500  route s1 -> r2. switchMap UNSUBSCRIBES r1's inner: it never
 *                  delivers, and in real rxjs its teardown runs. r2's inner
 *                  starts here and delivers at 3450, in step 3.
 *   step 3 @ 3500  route s1 -> r1 again. r2's inner already completed at 3450,
 *                  so nothing is cancelled; r1's inner is subscribed AFRESH and
 *                  delivers at 5450, in step 5.
 *
 * `open_route` is emitted with `+` only. rxjs has no retraction channel at all,
 * which is exactly why leg B opts into normalization N3 for this case: the
 * `-` half of sprefa's keyed replace has nothing here it could be compared to.
 */

import { Subject, VirtualTimeScheduler, map, switchMap, tap, timer } from "rxjs";

const STEP_MS = 1000;
const GUARD_MS = 250;
const FETCH_MS = 1950;

const scheduler = new VirtualTimeScheduler();

function emit(name: string, sign: string, payload: readonly unknown[]): void {
  const frame = scheduler.now();
  const withinStep = frame % STEP_MS;
  if (withinStep < GUARD_MS || withinStep > STEP_MS - GUARD_MS) {
    process.stderr.write(
      `BOUNDARY STRADDLE: ${name} at frame ${frame}, ${withinStep}ms into a ${STEP_MS}ms step\n`,
    );
    process.exitCode = 1;
  }
  const step = String(Math.floor(frame / STEP_MS)).padStart(2, "0");
  console.log(`${step} ${name} ${sign} ${JSON.stringify(payload)}`);
}

// ── the world ────────────────────────────────────────────────────────────────

const routeChange$ = new Subject<readonly [string, string]>();

/** One fetch. `timer` on the virtual scheduler stands in for leg B's
 *  `sleep "$RXO_NAP"`; the spawn ledger leg B keeps has no counterpart here,
 *  because an unsubscribed inner in rxjs does not run at all. */
const fetchBody$ = (routeId: string) =>
  timer(FETCH_MS, scheduler).pipe(map(() => `${routeId}-body`));

routeChange$
  .pipe(
    tap((change) => {
      emit("route_change", "+", change);
      emit("open_route", "+", change);
    }),
    switchMap(([sessionId, routeId]) =>
      fetchBody$(routeId).pipe(map((payload) => [sessionId, routeId, payload] as const)),
    ),
  )
  .subscribe((row) => emit("body", "+", row));

// ── the schedule ─────────────────────────────────────────────────────────────

const midpoint = (step: number): number => step * STEP_MS + STEP_MS / 2;

scheduler.schedule(() => routeChange$.next(["s1", "r1"]), midpoint(0));
scheduler.schedule(() => routeChange$.next(["s1", "r2"]), midpoint(1));
scheduler.schedule(() => routeChange$.next(["s1", "r1"]), midpoint(3));

scheduler.flush();
