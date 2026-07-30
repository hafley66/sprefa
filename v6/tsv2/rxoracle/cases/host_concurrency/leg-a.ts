/**
 * host_concurrency -- leg A, literal rxjs.
 *
 * Imports rxjs and nothing from this repository.
 *
 * Step schedule, mirroring cases/host_concurrency/case.json. stepMs is 1000,
 * the one batch is delivered at frame 500, and each job's work takes 1950
 * virtual ms (leg B's `RXO_NAP`).
 *
 *   step 0 @ 500   job j1 and job j2, back to back with nothing scheduled
 *                  between them. mergeMap subscribes BOTH inners immediately.
 *   step 2 @ 2450  both inners deliver, because they overlapped.
 *
 * `mergeMap` with no concurrency argument is unbounded, which is the operator
 * this case is asking about. `concatMap` would produce leg B's answer, and
 * writing it here would be answering the question with itself.
 */

import { Subject, VirtualTimeScheduler, map, mergeMap, tap, timer } from "rxjs";

const STEP_MS = 1000;
const GUARD_MS = 250;
const WORK_MS = 1950;

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

const jobSubject = new Subject<readonly [string]>();

const work$ = (jobId: string) => timer(WORK_MS, scheduler).pipe(map(() => `${jobId}-done`));

jobSubject
  .pipe(
    tap((job) => emit("job", "+", job)),
    mergeMap(([jobId]) => work$(jobId).pipe(map((payload) => [jobId, payload] as const))),
  )
  .subscribe((row) => emit("result", "+", row));

// ── the schedule ─────────────────────────────────────────────────────────────

scheduler.schedule(() => {
  jobSubject.next(["j1"]);
  jobSubject.next(["j2"]);
}, 500);

scheduler.flush();
