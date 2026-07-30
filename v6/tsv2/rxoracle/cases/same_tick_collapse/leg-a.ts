/**
 * same_tick_collapse -- leg A, literal rxjs.
 *
 * Imports rxjs and nothing from this repository.
 *
 * Step schedule, mirroring cases/same_tick_collapse/case.json. stepMs is 500
 * and the one batch is delivered at frame 250: three `next` calls with no
 * scheduling between them, which is as close as rxjs gets to "arrived
 * together". rxjs has no batch concept, so all three are ordinary events and
 * the operator chain sees every one of them.
 *
 * `distinctUntilChanged` is the same operator keyed_replace_vs_distinct uses,
 * and here it swallows nothing: alpha, beta and gamma are three different
 * values in a row.
 *
 * No normalization is opted into for this case. Whatever the diff says is the
 * whole finding.
 */

import { Subject, VirtualTimeScheduler, distinctUntilChanged, tap } from "rxjs";

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

const transitionSubject = new Subject<readonly [string, string]>();

transitionSubject
  .pipe(
    tap((transition) => emit("transition", "+", transition)),
    distinctUntilChanged((before, after) => before[0] === after[0] && before[1] === after[1]),
  )
  .subscribe((row) => emit("phase", "+", row));

// ── the schedule ─────────────────────────────────────────────────────────────

scheduler.schedule(() => {
  transitionSubject.next(["e", "alpha"]);
  transitionSubject.next(["e", "beta"]);
  transitionSubject.next(["e", "gamma"]);
}, 250);

scheduler.flush();
