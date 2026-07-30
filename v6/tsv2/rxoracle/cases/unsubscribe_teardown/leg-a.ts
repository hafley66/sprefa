/**
 * unsubscribe_teardown -- leg A, literal rxjs.
 *
 * Imports rxjs and nothing from this repository.
 *
 * There is no leg-B run for this case: `bop check` refuses the program with
 * `unsupported_construct(lifecycle_arm(unsubscribe))`, and that refusal IS the
 * measurement. This file still runs, because what rxjs does is the thing the
 * missing construct would have to reproduce, and printing it is what makes the
 * gap concrete instead of asserted.
 *
 * Step schedule. stepMs is 500 and each push is at frame k*500+250.
 *
 *   step 0  open request q1 -- an inner subscribes
 *   step 1  open request q2 -- a second inner subscribes
 *   step 2  close q1        -- takeUntil ends q1's inner; `finalize` fires
 *                              SYNCHRONOUSLY at that moment, in step 2
 *   step 3  close q2        -- same for q2
 *
 * The teardown is observed with `finalize`, which is rxjs's own answer to
 * "tell me when this subscription ends", and it fires for the reason the
 * subscription ended rather than for a row leaving a table. That difference is
 * the whole reason `finalize/1`'s live DL6 form is not a substitute: the
 * update-arm lab measured DL6 `finalize` as a per-row retraction landing on the
 * tick AFTER the departure.
 */

import { NEVER, Subject, VirtualTimeScheduler, concat, filter, finalize, mergeMap, of, takeUntil, tap } from "rxjs";

const STEP_MS = 500;
const GUARD_MS = 100;

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

const openRequestSubject = new Subject<readonly [string]>();
const closeRequestSubject = new Subject<readonly [string]>();

closeRequestSubject.subscribe((row) => emit("close_request", "+", row));

openRequestSubject
  .pipe(
    tap((request) => emit("open_request", "+", request)),
    // `concat(of(row), NEVER)` is what keeps the inner SUBSCRIBED after it has
    // emitted. A bare `of(row)` completes on the spot, `finalize` fires
    // immediately, and the case would be measuring completion rather than
    // teardown -- which is the same confusion `finalize/1`'s DL6 form invites.
    mergeMap(([requestId]) =>
      concat(of([requestId] as const), NEVER).pipe(
        tap((row) => emit("live", "+", row)),
        takeUntil(closeRequestSubject.pipe(filter(([closing]) => closing === requestId))),
        finalize(() => emit("torn_down", "+", [requestId])),
      ),
    ),
  )
  .subscribe();

// ── the schedule ─────────────────────────────────────────────────────────────

const midpoint = (step: number): number => step * STEP_MS + STEP_MS / 2;

scheduler.schedule(() => openRequestSubject.next(["q1"]), midpoint(0));
scheduler.schedule(() => openRequestSubject.next(["q2"]), midpoint(1));
scheduler.schedule(() => closeRequestSubject.next(["q1"]), midpoint(2));
scheduler.schedule(() => closeRequestSubject.next(["q2"]), midpoint(3));

scheduler.flush();
