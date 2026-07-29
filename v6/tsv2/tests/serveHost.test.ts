/**
 * serveHost.test.ts — RECEIPT (b) of the runtime-bridge arc
 * (plans/2026-07-29-runtime-bridge-header.md scope 4b): a program with BOTH
 * live world seams running at once -- an interval bind on a TestScheduler and a
 * hermetic sh host that really spawns -- graded against the oracle.
 *
 * GRADING, and why it is total rather than a prefix. Two columns in this
 * program are not schedule-determined: the bind's `bucket` (real epoch time by
 * the bucket law, deliberately NOT moved by the injected scheduler) and every
 * digest derived from it. The receipt does not exclude them: it REPLAYS them.
 * `scheduleFromTicks` reads back, per tick, the rows the world actually pushed
 * (an arrival-target rel's boundary delta at a tick IS that push), and the
 * oracle is fed exactly those as its schedule. So the diff below covers every
 * column of every tick, with the live values carried across rather than
 * hand-waved away.
 *
 * NO WALL-CLOCK SLEEP drives the cadence: the interval runs on virtual time.
 * The two real waits here are for ASYNC REGISTRATION and ASYNC SETTLEMENT (the
 * db round trip before the interval subscribes; the subprocess and its response
 * commit afterwards), both bounded polls, neither a stand-in for the cadence --
 * the same split v6/dl's own bind tests record.
 *
 * SABOTAGE RECEIPT (run 2026-07-29, reverted): writing anything but the demand
 * row's own witness into the response arrival's `witness_digest` column
 * (1_hosts.ts's response row builder) breaks the join the probe lowered to, so
 * `answered` never derives and this test fails with "timeout waiting for the sh
 * host's response to land in answered" (1 pass, 1 fail).
 *
 * KNOWN LIMIT of replay grading, stated rather than hidden: because the oracle
 * is fed the rows the server actually pushed, a sabotage of the WORLD side (a
 * wrong bucket value, a mangled stdout) is replayed faithfully and stays green
 * here. What this receipt grades is the ENGINE: given those pushes, are the
 * derived deltas the reference engine's. The world side is graded by the
 * assertions below it (witness shape, host output text) and by the endurance
 * receipt (scripts/endurance.sh).
 */

import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { VirtualTimeScheduler, firstValueFrom } from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import { oracleLog, logOfTicks, postArrivals, postProgram, request, scheduleFromTicks, startServed, tickEvents } from "./serveHelpers.ts";

const HOST_CLOCK_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-host-clock.dl6", import.meta.url));

const STRUCT_HOST_DL6 = `
type span(end: int, start: int).
rel source_path(path: text).
rel host_span(path: text, at: span).
rel host_start(path: text, start: int).

sh scan_span(path: text) -> (at: span) =
  \`printf '%s\\n' '{"at":{"start":17,"end":42}}'; : "$path"\`.

host_span(Path, At) <-
  source_path(Path),
  ? scan_span(Path, At).

host_start(Path, Start) <-
  host_span(Path, At),
  decode(At, {start: Start}).
`;

/** Advance an injected virtual scheduler by whole seconds and flush. `maxFrames`
 *  is bounded on purpose: a live `interval` reschedules itself every firing, so
 *  an unbounded flush would spin forever. */
function advanceVirtualSeconds(scheduler: VirtualTimeScheduler, seconds: number): void {
  scheduler.maxFrames = scheduler.frame + seconds * 1000;
  scheduler.flush();
}

async function waitUntil(predicate: () => boolean | Promise<boolean>, what: string, timeoutMs = 10_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!(await predicate())) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
}

test("receipt (b): live interval bind + live sh host, served, matches the oracle fed the same answers", async () => {
  const source = readFileSync(HOST_CLOCK_DL6, "utf8");
  const scheduler = new VirtualTimeScheduler();
  const served = await startServed(17531, scheduler);
  try {
    const loaded = await postProgram(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);
    const plans = JSON.parse(loaded.body) as {
      readonly hosts: readonly string[];
      readonly binds: readonly { readonly name: string; readonly literals: readonly (string | number)[] }[];
      readonly arrivalTargets: readonly string[];
    };
    // The program's own rules said the cadence; nothing out here chose it.
    assert.deepEqual(plans.hosts, ["answer"]);
    assert.deepEqual(plans.binds, [{ name: "interval", literals: [1] }]);

    await postArrivals(served.port, [{ rel: "seed", sign: "add", row: ["alpha"] }]);

    // The interval subscribes only after the program's boot SQL resolves, on
    // the real event loop; flushing before it registers advances nothing.
    await waitUntil(() => scheduler.actions.length >= 1, "the interval to register on the injected scheduler");
    advanceVirtualSeconds(scheduler, 1);

    // The bind's tick is synchronous with the flush; the host's subprocess and
    // its response commit are not.
    await waitUntil(async () => {
      const answered = JSON.parse((await request(served.port, "/idb/answered", "GET")).body) as { rows: unknown[] };
      return answered.rows.length === 1;
    }, "the sh host's response to land in answered");

    const outcomes = tickEvents(served.events);
    assert.equal(outcomes.length, 3, `expected seed / bind / host ticks, got ${outcomes.length}`);

    const replayed = scheduleFromTicks(outcomes, plans.arrivalTargets);
    assert.deepEqual(
      replayed.map((batch) => batch.map((arrival) => arrival.rel)),
      [["seed"], ["interval"], ["__host_response_answer"]],
    );
    assert.equal(logOfTicks(outcomes), oracleLog(source, replayed));

    // The host really ran: one witness, one spawn, one answer.
    const demand = JSON.parse((await request(served.port, "/idb/__host_demand_answer", "GET")).body) as {
      rows: readonly (readonly (string | number)[])[];
    };
    assert.equal(demand.rows.length, 1);
    assert.match(String(demand.rows[0]?.[1]), /^witness\|answer\|name:text=alpha\|bucket=\d+$/);
    const answered = JSON.parse((await request(served.port, "/idb/answered", "GET")).body) as {
      rows: readonly (readonly (string | number)[])[];
    };
    assert.equal(answered.rows[0]?.[0], "alpha");
    assert.equal(answered.rows[0]?.[2], "alpha-answered");
  } finally {
    await served.stop();
  }
});

test("receipt (b) teardown: a program swap stops the previous program's interval", async () => {
  const source = readFileSync(HOST_CLOCK_DL6, "utf8");
  const scheduler = new VirtualTimeScheduler();
  const served = await startServed(17532, scheduler);
  try {
    assert.equal((await postProgram(served.port, source)).statusCode, 200);
    await waitUntil(() => scheduler.actions.length === 1, "the first program's interval to register");

    // A program with no bind at all: the switchMap must leave zero timers.
    const plain = "rel event(id: int, kind: text) log keep(all).\nrel current(id: int, kind: text).\ncurrent(Id, Kind) <- event(Id, Kind).\n";
    assert.equal((await postProgram(served.port, plain)).statusCode, 200);
    await waitUntil(() => scheduler.actions.length === 0, "the swapped-out program's interval to be torn down");
  } finally {
    await served.stop();
  }
});

/**
 * HOST-OUTPUT-SEAM FAIL-FIRST RECEIPT:
 *   1. Before decl-B parser acceptance, POST /program returned 400 with
 *      unsupported_surface(column_type_wrapper(scan_span,at,none)).
 *   2. With parser acceptance alone, the host effect settled as error because
 *      StructPlane received canonical JSON text and refused
 *      type_arrival_shape_mismatch: not_an_object(span, ...).
 *
 * The file-backed injected seam lets this one receipt inspect the dictionary
 * count after the live host tick without exposing dictionaries through /idb.
 */
test("declared-struct live host output interns once and tick logs render the canonical value", async () => {
  const directory = mkdtempSync(join(tmpdir(), "tsv2-struct-host-"));
  const dbUrl = `file:${join(directory, "served.sqlite")}`;
  const served = await startServed(17533, undefined, dbUrl);
  try {
    const loaded = await postProgram(served.port, STRUCT_HOST_DL6);
    assert.equal(loaded.statusCode, 200, loaded.body);

    await postArrivals(served.port, [
      { rel: "source_path", sign: "add", row: ["a.rs"] },
    ]);
    await waitUntil(async () => {
      const rows = JSON.parse((await request(served.port, "/idb/host_start", "GET")).body) as {
        rows: readonly (readonly (string | number)[])[];
      };
      return rows.rows.length === 1;
    }, "the struct-typed host answer to land");

    const hostSpan = JSON.parse((await request(served.port, "/idb/host_span", "GET")).body) as {
      rows: readonly (readonly (string | number)[])[];
    };
    const hostStart = JSON.parse((await request(served.port, "/idb/host_start", "GET")).body) as {
      rows: readonly (readonly (string | number)[])[];
    };
    assert.deepEqual(hostSpan.rows, [["a.rs", '{"end":42,"start":17}']]);
    assert.deepEqual(hostStart.rows, [["a.rs", 17]]);

    const outcomes = tickEvents(served.events);
    assert.equal(outcomes.length, 2, `expected source / host ticks, got ${outcomes.length}`);
    const hostTick = JSON.parse(outcomes[1]!.line) as {
      deltas: Record<string, { add: readonly (readonly unknown[])[]; del: readonly (readonly unknown[])[] }>;
    };
    assert.deepEqual(hostTick.deltas["__host_response_scan_span"]?.add, [
      [
        "witness|scan_span|path:text=a.rs",
        0,
        "a.rs",
        { end: 42, start: 17 },
      ],
    ]);
    assert.deepEqual(hostTick.deltas.host_span?.add, [
      ["a.rs", { end: 42, start: 17 }],
    ]);
    assert.deepEqual(hostTick.deltas.host_start?.add, [["a.rs", 17]]);

    const inspection = ScratchStore.open(dbUrl);
    try {
      const dictionaryRows = await firstValueFrom(
        inspection.runner.scalar(
          inspection.db,
          'SELECT count(*) FROM "__dict_span"',
        ),
      );
      assert.equal(dictionaryRows, 1);
    } finally {
      inspection.db.close();
    }
  } finally {
    await served.stop();
    rmSync(directory, { recursive: true, force: true });
  }
});
