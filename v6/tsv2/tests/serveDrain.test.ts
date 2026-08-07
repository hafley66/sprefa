/**
 * serveDrain.test.ts — the drain half of the served grading, and the one honest
 * difference between a schedule-fed run and a served one.
 *
 * `TickFold` drains only AFTER the whole schedule: a carrying tick that still
 * has a scheduled batch behind it takes the batch, not a drain tick. A SERVER
 * cannot know a future POST is coming, so a carrying tick with an empty queue
 * drains immediately. Measured, not assumed: `switch_as_keyed_replace` fed its
 * own two batches prints 3 ticks under `TickFold` (settle, settle, one trailing
 * drain) and 4 when served one POST at a time (settle, drain, settle, drain).
 * The DELTAS are identical; the tick BOUNDARIES are the server's own.
 *
 * So the grading is stated in the form that survives that: the served run's own
 * consumed schedule -- one batch per tick, read back off the ticks themselves --
 * replayed through the oracle. That is a total byte comparison, not a prefix
 * one, and it is the same question the schedule-fed sweep asks: given these
 * exact per-tick world pushes, does the reference engine produce these exact
 * deltas.
 *
 * SABOTAGE RECEIPT (run 2026-07-29, reverted): removing the
 * `this.queuedBatches > 0` guard from 3_engine.ts's drain rule does NOT flip
 * the first test (the replay adapts to whatever boundaries the server chose) --
 * it flips the second, which posts two batches without awaiting the first and
 * asserts no vacuous tick lands between them.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { firstValueFrom, forkJoin, toArray } from "rxjs";

import { program as switch_program } from "../gen_emitted/switch_as_keyed_replace.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IServedProgram } from "../runtime/types.ts";
import { LiveEngine, boot_served_program } from "../serve/3_engine.ts";
import { log_of_ticks, oracle_log, post_arrivals, post_program, schedule_from_ticks, start_served, tick_events } from "./serveHelpers.ts";

const SWITCH_DL6 = fileURLToPath(new URL("../../prolog/compile/dl_view/switch_as_keyed_replace.dl6", import.meta.url));

const ROUTE_BATCHES = [
  [{ rel: "route_change", sign: "add" as const, row: ["session_one", "settings"] }],
  [{ rel: "route_change", sign: "add" as const, row: ["session_one", "profile"] }],
];

test("a carrying program served one POST at a time matches the oracle replayed on its own consumed schedule", async () => {
  const source = readFileSync(SWITCH_DL6, "utf8");
  const served = await start_served();
  try {
    const loaded = await post_program(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);
    const { arrival_targets } = JSON.parse(loaded.body) as { readonly arrival_targets: readonly string[] };

    for (const batch of ROUTE_BATCHES) await post_arrivals(served.port, batch);

    const outcomes = tick_events(served.events);
    // settle, drain, settle, drain -- the server's own boundaries.
    assert.deepEqual(outcomes.map((outcome) => outcome.tick), [1, 2, 3, 4]);

    const replayed = schedule_from_ticks(outcomes, arrival_targets);
    assert.deepEqual(replayed.map((batch) => batch.length), [1, 0, 1, 0]);
    assert.equal(log_of_ticks(outcomes), oracle_log(source, replayed));
  } finally {
    await served.stop();
  }
});

test("a batch already queued behind a carrying tick takes the tick, not a drain", async () => {
  // Driven at the ENGINE, not over http, on purpose. Two concurrent POSTs do
  // NOT reproduce this: each request's body read is asynchronous, so the second
  // batch reaches `submit` only after the first has already settled and drained
  // (measured -- the http version of this test saw 4 ticks, not 3). The queue
  // guard is about batches that are ALREADY enqueued, which over http means
  // arrivals produced by the engine's own sources (a host response landing
  // while a bind firing waits), and here means two synchronous submits.
  const seam = ScratchStore.open(":memory:");
  const engine = new LiveEngine(switch_program as unknown as IServedProgram, seam);
  const ticks: number[] = [];
  const lines: string[] = [];
  await firstValueFrom(boot_served_program(seam, switch_program as unknown as IServedProgram));
  const running = engine.ticks$.subscribe((outcome) => {
    ticks.push(outcome.tick);
    lines.push(outcome.line);
  });
  try {
    const settled = firstValueFrom(
      forkJoin([
        engine.submit(ROUTE_BATCHES[0]!).pipe(toArray()),
        engine.submit(ROUTE_BATCHES[1]!).pipe(toArray()),
      ]),
    );
    await settled;
    assert.deepEqual(ticks, [1, 2, 3]);
    const vacuous = lines.filter((line) => line.endsWith('"deltas":{}}'));
    assert.equal(vacuous.length, 1, `exactly one trailing drain tick, got ${vacuous.length}: ${JSON.stringify(lines)}`);
    assert.match(vacuous[0] ?? "", /"tick":3/);
  } finally {
    running.unsubscribe();
    seam.db.close();
  }
});
