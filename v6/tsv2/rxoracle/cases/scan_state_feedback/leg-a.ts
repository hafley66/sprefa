/**
 * scan_state_feedback -- leg A, literal rxjs.
 *
 * Imports rxjs and nothing from this repository.
 *
 * Step schedule, mirroring cases/scan_state_feedback/case.json. stepMs is 500
 * and each batch is delivered at frame k*500+250.
 *
 *   step 0  increment -> 1
 *   step 1  increment -> 2
 *   step 2  increment -> 3
 *   step 3  TWO increments with nothing scheduled between them -> 4, then 5.
 *           rxjs has no batch concept, so both are ordinary events and `scan`
 *           emits both accumulator values.
 *
 * `counter` is emitted with `+` only. rxjs has no retraction, so leg B opts
 * into normalization N3 for the `-` half of its keyed replace.
 */

import { Subject, VirtualTimeScheduler, map, scan, tap } from "rxjs";

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

const incrementSubject = new Subject<readonly [string]>();

incrementSubject
  .pipe(
    tap((increment) => emit("increment", "+", increment)),
    scan((total) => total + 1, 0),
    map((total) => ["clicks", total] as const),
  )
  .subscribe((row) => emit("counter", "+", row));

// ── the schedule ─────────────────────────────────────────────────────────────

const midpoint = (step: number): number => step * STEP_MS + STEP_MS / 2;

scheduler.schedule(() => incrementSubject.next(["clicks"]), midpoint(0));
scheduler.schedule(() => incrementSubject.next(["clicks"]), midpoint(1));
scheduler.schedule(() => incrementSubject.next(["clicks"]), midpoint(2));
scheduler.schedule(() => {
  incrementSubject.next(["clicks"]);
  incrementSubject.next(["clicks"]);
}, midpoint(3));

scheduler.flush();
