/**
 * serveDoor.test.ts — RECEIPT (a) of the runtime-bridge arc
 * (plans/2026-07-29-runtime-bridge-header.md scope 4a): the hand-written
 * door program, SERVED, graded byte-for-byte against the oracle.
 *
 * The whole point is that serving changes nothing about the answer. The .dl6
 * text goes over http, the arrival batches go over http one POST at a time,
 * and the tick log the served process prints must equal, byte for byte, the
 * log the reference engine prints for the same program text fed the same
 * batches as a schedule. This program never carries, so its served boundaries
 * and the schedule-fed ones coincide exactly and the comparison is against the
 * LITERAL posted schedule. A carrying program's boundaries are its own; that
 * case, and why, is serveDrain.test.ts.
 *
 * SABOTAGE RECEIPT (run 2026-07-29, reverted): reversing the line order in
 * `servedLog` fails the first test with a tick-order diff (2 pass, 1 fail), so
 * the comparison is not vacuous.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import type { IArrivalBatch } from "../runtime/types.ts";
import { oracleLog, postArrivals, postProgram, servedLog, startServed, type TickReply } from "./serveHelpers.ts";

const DOOR_DL6 = fileURLToPath(new URL("../../dl/fixtures/door-handwritten.dl6", import.meta.url));

const DOOR_SCHEDULE: readonly IArrivalBatch[] = [
  [{ rel: "event", sign: "add", row: [1, "boot"] }],
  [{ rel: "event", sign: "add", row: [1, "ready"] }],
];

test("receipt (a): door-handwritten served over http matches the oracle tick log byte for byte", async () => {
  const source = readFileSync(DOOR_DL6, "utf8");
  const served = await startServed(17521);
  try {
    const loaded = await postProgram(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);
    const parsed = JSON.parse(loaded.body) as { readonly arrivalTargets: readonly string[] };
    assert.ok(parsed.arrivalTargets.includes("event"));

    const replies: TickReply[] = [];
    for (const batch of DOOR_SCHEDULE) replies.push(await postArrivals(served.port, batch));

    assert.equal(servedLog(replies), oracleLog(source, DOOR_SCHEDULE));
  } finally {
    await served.stop();
  }
});

test("receipt (a) guard: a batch naming a rel that is not an arrival target is a 400, not a tick", async () => {
  const source = readFileSync(DOOR_DL6, "utf8");
  const served = await startServed(17522);
  try {
    assert.equal((await postProgram(served.port, source)).statusCode, 200);
    // `current` is a derived rel: rules head it, the world never does.
    await assert.rejects(
      () => postArrivals(served.port, [{ rel: "current", sign: "add", row: [1, "boot"] }]),
      /is not an arrival target/,
    );
    // Wrong column count on a real arrival target is refused the same way.
    await assert.rejects(
      () => postArrivals(served.port, [{ rel: "event", sign: "add", row: [1] }]),
      /takes 2 columns, got 1/,
    );
    // The engine is still alive and still counting from tick 1.
    const reply = await postArrivals(served.port, [{ rel: "event", sign: "add", row: [7, "later"] }]);
    assert.equal(reply.ticks[0]?.tick, 1);
  } finally {
    await served.stop();
  }
});

test("receipt (a) refusal: a program the compiler refuses is a 400 and leaves the running program alone", async () => {
  const source = readFileSync(DOOR_DL6, "utf8");
  const served = await startServed(17523);
  try {
    assert.equal((await postProgram(served.port, source)).statusCode, 200);
    await postArrivals(served.port, [{ rel: "event", sign: "add", row: [1, "boot"] }]);

    // `latest` in a level rule is a live named refusal (review-B2).
    const rejected = await postProgram(
      served.port,
      "rel event(id: int, kind: text) log keep(all).\nrel current(id: int, kind: text).\ncurrent(Id, Kind) <- latest(event(Id, Kind)).\n",
    );
    assert.equal(rejected.statusCode, 400);
    assert.match(rejected.body, /latest_in_level_rule/);

    // Still the old program, still ticking from where it was.
    const reply = await postArrivals(served.port, [{ rel: "event", sign: "add", row: [2, "ready"] }]);
    assert.equal(reply.ticks[0]?.tick, 2);
  } finally {
    await served.stop();
  }
});
