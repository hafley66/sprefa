/**
 * latest_sampling -- leg A, literal rxjs.
 *
 * Imports rxjs and nothing from this repository.
 *
 * Step schedule, mirroring cases/latest_sampling/case.json. stepMs is 500 and
 * each batch is delivered at frame k*500+250.
 *
 *   step 0  tick_event e0, before configChange$ has emitted anything.
 *           withLatestFrom drops the notification entirely.
 *   step 1  config becomes v1
 *   step 2  tick_event e1 samples v1
 *   step 3  config becomes v2
 *   step 4  tick_event e2 samples v2
 *
 * `tick_event` is emitted from a `tap` BEFORE withLatestFrom, because the
 * arrival relation on leg B gains its row whether or not the join derives
 * anything, and grading only the derived relation would hide the step-0 drop.
 *
 * `config` is emitted with `+` only. rxjs has no retraction, so leg B opts into
 * normalization N3 for the `-` half of its keyed replace.
 */

import { Subject, VirtualTimeScheduler, tap, withLatestFrom } from "rxjs";

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

const tickEventSubject = new Subject<readonly [string]>();
const configChangeSubject = new Subject<readonly [string, string]>();

const config$ = configChangeSubject.pipe(
  tap((change) => {
    emit("config_change", "+", change);
    emit("config", "+", change);
  }),
);

// withLatestFrom subscribes to config$ here, which is what makes the tap above
// run. Subscribing it a second time would double every config line.
tickEventSubject
  .pipe(
    tap((event) => emit("tick_event", "+", event)),
    withLatestFrom(config$),
  )
  .subscribe(([event, config]) => emit("sampled", "+", [event[0], config[1]]));

// ── the schedule ─────────────────────────────────────────────────────────────

const midpoint = (step: number): number => step * STEP_MS + STEP_MS / 2;

scheduler.schedule(() => tickEventSubject.next(["e0"]), midpoint(0));
scheduler.schedule(() => configChangeSubject.next(["cfg", "v1"]), midpoint(1));
scheduler.schedule(() => tickEventSubject.next(["e1"]), midpoint(2));
scheduler.schedule(() => configChangeSubject.next(["cfg", "v2"]), midpoint(3));
scheduler.schedule(() => tickEventSubject.next(["e2"]), midpoint(4));

scheduler.flush();
