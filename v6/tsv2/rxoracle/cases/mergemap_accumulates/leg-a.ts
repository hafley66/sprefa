/**
 * mergemap_accumulates -- leg A, literal rxjs.
 *
 * Imports rxjs and nothing from this repository, including no shared `emit`
 * helper: the point of leg A is that it can be checked against the rxjs docs
 * with no knowledge of sprefa at all.
 *
 * Step schedule, mirroring cases/mergemap_accumulates/case.json. stepMs is 500
 * and each batch is delivered at its step's MIDPOINT, so frame k*500+250.
 *
 *   step 0 @ 250   four `item` rows seed the lookup table
 *   step 1 @ 750   request r1 over key a
 *   step 2 @ 1250  request r2 over key b
 *   step 3 @ 1750  request r3 over key a again
 *   step 4         idle
 *
 * Every source push is itself an event on its own named observable, because
 * sprefa's tick log carries a delta for the arrival relation too; grading only
 * the derived relation would compare half the run.
 */

import { Subject, VirtualTimeScheduler, mergeMap, of } from "rxjs";

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

const item$ = new Subject<readonly [string, string]>();
const request$ = new Subject<readonly [string, string]>();

/** The lookup table as rxjs sees it: rows already delivered on `item$`. A
 *  level-rule join has no ordering rule, so a plain accumulating array is the
 *  faithful reading, not a `scan` with replay semantics of its own. */
const itemRows: [string, string][] = [];

item$.subscribe((row) => {
  itemRows.push([row[0], row[1]]);
  emit("item", "+", row);
});

// mergeMap: the inner for one request is the set of matching lookup rows.
// Nothing unsubscribes an earlier inner, so results accumulate.
request$
  .pipe(
    mergeMap((request) => {
      emit("request", "+", request);
      const matches = itemRows.filter(([key]) => key === request[1]);
      return of(...matches.map(([, part]) => [request[0], request[1], part] as const));
    }),
  )
  .subscribe((row) => emit("enriched", "+", row));

// ── the schedule ─────────────────────────────────────────────────────────────

const midpoint = (step: number): number => step * STEP_MS + STEP_MS / 2;

scheduler.schedule(() => {
  item$.next(["a", "one"]);
  item$.next(["a", "two"]);
  item$.next(["b", "one"]);
  item$.next(["b", "two"]);
}, midpoint(0));
scheduler.schedule(() => request$.next(["r1", "a"]), midpoint(1));
scheduler.schedule(() => request$.next(["r2", "b"]), midpoint(2));
scheduler.schedule(() => request$.next(["r3", "a"]), midpoint(3));

scheduler.flush();
