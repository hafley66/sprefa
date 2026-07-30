/**
 * keyed_replace_vs_distinct -- leg A, literal rxjs.
 *
 * Imports rxjs and nothing from this repository.
 *
 * Step schedule, mirroring cases/keyed_replace_vs_distinct/case.json. stepMs is
 * 500 and each batch is delivered at frame k*500+250.
 *
 *   step 0  one
 *   step 1  one   (identical repeat -- distinctUntilChanged swallows it)
 *   step 2  two
 *   step 3  two   (identical repeat -- swallowed)
 *   step 4  one   (a real change again, since the comparison is to the
 *                  PREVIOUS emission and not to anything ever seen)
 *
 * The comparison is on the whole row, matching a `key(1)` head whose non-key
 * column is the value. `distinctUntilChanged` with no arguments compares with
 * `===`, which on two distinct arrays is always false, so the comparator is
 * given explicitly rather than relying on reference identity.
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

const readingSubject = new Subject<readonly [string, string]>();

readingSubject
  .pipe(
    tap((reading) => emit("reading", "+", reading)),
    distinctUntilChanged((before, after) => before[0] === after[0] && before[1] === after[1]),
  )
  .subscribe((row) => emit("current", "+", row));

// ── the schedule ─────────────────────────────────────────────────────────────

const midpoint = (step: number): number => step * STEP_MS + STEP_MS / 2;

scheduler.schedule(() => readingSubject.next(["s", "one"]), midpoint(0));
scheduler.schedule(() => readingSubject.next(["s", "one"]), midpoint(1));
scheduler.schedule(() => readingSubject.next(["s", "two"]), midpoint(2));
scheduler.schedule(() => readingSubject.next(["s", "two"]), midpoint(3));
scheduler.schedule(() => readingSubject.next(["s", "one"]), midpoint(4));

scheduler.flush();
