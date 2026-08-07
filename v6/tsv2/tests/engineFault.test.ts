/**
 * engineFault.test.ts — one bad tick must not end the process.
 *
 * THE DEFECT (review finding 1, plans/2026-07-30-ts-lowering-review.md).
 * `LiveEngine.ticks$` is a single `concatMap` lane over one Subject. `runBatch`
 * reported a tick fault to its submitter and then RE-THREW it into that shared
 * lane. An error inside `concatMap` terminates the outer observable, so `ticks$`
 * was dead from that moment: `tap({ finalize })` flipped `running` false and
 * every later `submit` failed with "tsv2 engine is not running". `runProgram$`
 * merges `ticks$` into the app graph, so the error also reached `serveTsv2`'s
 * single subscriber and `serve/main.ts` exited. `serve/4_http.ts:315-316` states
 * the opposite law in its own comment ("one bad request cannot end the app").
 *
 * THE FAULT USED HERE is deliberately NOT a malformed wire shape. Arrival
 * validation (see serveArrivalValidation.test.ts) now rejects those at the
 * boundary with a 400, which is correct and would make this receipt vacuous.
 * `sign: "del"` against a LOG rel is the honest remaining class: a perfectly
 * well-formed batch, on a declared arrival target, at the declared width, that
 * the ENGINE refuses -- `runtime/1_incremental.ts:635` raises "retract from log
 * rel". Same shape as every other in-tick fault the review names (a SQLite
 * constraint violation, a disk-full, a decode fault in a generated module):
 * raised below the boundary, with no boundary check that could have caught it.
 *
 * RED FIRST, verbatim, at be97647c before the fix (`node --test
 * --experimental-transform-types tests/engineFault.test.ts`), all three red:
 *
 *   ✖ a tick fault answers its own submitter and the lane keeps turning
 *     Error: connect ECONNREFUSED 127.0.0.1:50113
 *         at TCPConnectWrap.afterConnect [as oncomplete] (node:net:1705:16) {
 *       errno: -61, code: 'ECONNREFUSED', syscall: 'connect',
 *       address: '127.0.0.1', port: 50113 }
 *   ✖ the app graph survives a tick fault
 *     AssertionError [ERR_ASSERTION]: ticks$ must still be live after a fault
 *     true !== false
 *   ✖ a program swap after a tick fault still loads and ticks
 *     Error: connect ECONNREFUSED 127.0.0.1:50118
 *   # fail 3
 *
 * ECONNREFUSED is SHARPER than the review's own probe reported. The review saw
 * "tsv2 engine is not running" because it held `LiveEngine` directly. Through
 * the real server the error does not stop at the engine: it travels up the
 * merge in `runProgram$` to `serveTsv2`'s single subscriber, unsubscribes the
 * `httpServer$` teardown, and closes the listening socket. One well-formed POST
 * took the whole process down before it could answer the request that caused it.
 *
 * SABOTAGE RECEIPTS, both run after the fix and reverted:
 *
 *   Restoring `return throwError(() => failure)` in place of `return EMPTY` in
 *   3_engine.ts's runBatch error arm reproduces the three failures above.
 *
 *   Swallowing the fault ENTIRELY (`void failure` in place of
 *   `queued.subscriber.error(failure)`, keeping EMPTY) flips a DIFFERENT
 *   assertion, verbatim:
 *     ✖ a tick fault answers its own submitter and the lane keeps turning
 *       AssertionError: a tick fault is a 500, not a silent 200: {"ticks":[]}
 *       200 !== 500
 *     ✖ the app graph survives a tick fault
 *       AssertionError: the submitter is owed the failure
 *   Absorbing the fault into the lane and reporting it to the submitter are two
 *   separate obligations, and each one has its own red.
 *
 * NAMED GAP, not closed here. `engine.submit` is called from three places. Two
 * handle its error arm: http (`routeRequest$`'s catchError -> 500) and the host
 * runner (`1_hosts.ts:611` catchError -> settleInvocationError). The two BIND
 * runners (`2_binds.ts:164`, `:354`) do not, so a tick fault raised by a bind's
 * own arrival still errors `firings$` and still ends the app. Reachable only
 * through an emitted-program defect on a bind-fed rel, which is why it is
 * recorded rather than patched blind: a fix with no fail-first receipt is worth
 * less than a named gap.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { firstValueFrom, toArray } from "rxjs";

import { program as switchProgram } from "../gen_emitted/switch_as_keyed_replace.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IServedProgram } from "../runtime/types.ts";
import { LiveEngine, bootServedProgram } from "../serve/3_engine.ts";
import { postProgram, request, startServed, tickEvents } from "./serveHelpers.ts";

const SWITCH_DL6 = fileURLToPath(new URL("../../prolog/compile/dl_view/switch_as_keyed_replace.dl6", import.meta.url));

/** A well-formed batch against a log rel's declared arrival target. */
function goodBatch(routeId: string): string {
  return JSON.stringify({ batch: [{ rel: "route_change", sign: "add", row: ["session_one", routeId] }] });
}

/** Same rel, same width, same declared target -- and a sign the engine refuses
 *  because `route_change` is a Log rel (its `arrivalDelSql` is null). */
const FAULTING_BATCH = JSON.stringify({
  batch: [{ rel: "route_change", sign: "del", row: ["session_one", "settings"] }],
});

test("a tick fault answers its own submitter and the lane keeps turning", async () => {
  const source = readFileSync(SWITCH_DL6, "utf8");
  const served = await startServed();
  try {
    assert.equal((await postProgram(served.port, source)).statusCode, 200);

    const first = await request(served.port, "/edb/events", "POST", goodBatch("settings"));
    assert.equal(first.statusCode, 200, first.body);

    const faulted = await request(served.port, "/edb/events", "POST", FAULTING_BATCH);
    assert.equal(faulted.statusCode, 500, `a tick fault is a 500, not a silent 200: ${faulted.body}`);
    assert.match(
      faulted.body,
      /retract from log rel 'route_change'/,
      `the fault must reach ITS OWN submitter naming the rel: ${faulted.body}`,
    );

    const third = await request(served.port, "/edb/events", "POST", goodBatch("profile"));
    assert.equal(third.statusCode, 200, `POST /edb/events after a faulting tick -> ${third.statusCode} ${third.body}`);
    const ticks = (JSON.parse(third.body) as { readonly ticks: readonly unknown[] }).ticks;
    assert.ok(ticks.length > 0, `the third post must produce real ticks: ${third.body}`);

    // The process is still serving every other route too, not just /edb/events.
    const read = await request(served.port, "/idb/route_view", "GET");
    assert.equal(read.statusCode, 200, read.body);
  } finally {
    await served.stop();
  }
});

test("the app graph survives a tick fault", async () => {
  // Engine level, no http: the claim is about `ticks$` itself, which is what
  // `runProgram$` merges into the app graph. A lane that errored would take
  // `serveTsv2`'s single subscriber down with it.
  const seam = ScratchStore.open(":memory:");
  const engine = new LiveEngine(switchProgram as unknown as IServedProgram, seam);
  let laneErrored = false;
  let laneCompleted = false;
  const seen: number[] = [];
  const watching = engine.ticks$.subscribe({
    next: (outcome) => seen.push(outcome.tick),
    error: () => {
      laneErrored = true;
    },
    complete: () => {
      laneCompleted = true;
    },
  });
  try {
    await firstValueFrom(bootServedProgram(seam, switchProgram as unknown as IServedProgram));

    await firstValueFrom(
      engine.submit([{ rel: "route_change", sign: "add", row: ["session_one", "settings"] }]).pipe(toArray()),
    );
    const beforeFault = seen.length;

    const submitterSaw = await firstValueFrom(
      engine.submit([{ rel: "route_change", sign: "del", row: ["session_one", "settings"] }]).pipe(toArray()),
    ).then(
      () => null,
      (failure: unknown) => (failure instanceof Error ? failure.message : String(failure)),
    );
    assert.match(String(submitterSaw), /retract from log rel/, "the submitter is owed the failure");

    assert.equal(laneErrored, false, "ticks$ must still be live after a fault");
    assert.equal(laneCompleted, false, "ticks$ must not complete on a fault");

    await firstValueFrom(
      engine.submit([{ rel: "route_change", sign: "add", row: ["session_one", "profile"] }]).pipe(toArray()),
    );
    assert.ok(seen.length > beforeFault, `the lane must keep emitting: ticks seen ${seen.join(",")}`);
    // A faulted tick advances no tick number: the numbering the oracle grades
    // against must not have a hole in it.
    assert.deepEqual(seen, [...Array(seen.length).keys()].map((index) => index + 1));
  } finally {
    watching.unsubscribe();
    seam.db.close();
  }
});

test("a program swap after a tick fault still loads and ticks", async () => {
  // The fault must not leave the SERVER's program-lifetime branch poisoned
  // either: `switchMap` over accepted loads is what makes a swap work, and an
  // errored inner would have already ended the merge it sits in.
  const source = readFileSync(SWITCH_DL6, "utf8");
  const served = await startServed();
  try {
    assert.equal((await postProgram(served.port, source)).statusCode, 200);
    assert.equal((await request(served.port, "/edb/events", "POST", FAULTING_BATCH)).statusCode, 500);

    assert.equal((await postProgram(served.port, source)).statusCode, 200, "a swap after a fault must be accepted");
    const afterSwap = await request(served.port, "/edb/events", "POST", goodBatch("settings"));
    assert.equal(afterSwap.statusCode, 200, afterSwap.body);
    assert.ok(tickEvents(served.events).length > 0, "the fresh program's ticks reach the app graph");
  } finally {
    await served.stop();
  }
});
