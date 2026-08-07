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
import { oracle_log, log_of_ticks, post_arrivals, post_program, request, schedule_from_ticks, start_served, tick_events } from "./serveHelpers.ts";

const HOST_CLOCK_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-host-clock.dl6", import.meta.url));

const STRUCT_HOST_DL6 = `
rel span(end: int, start: int).
rel source_path(path: text).
rel host_span(path: text, at: span).
rel host_start(path: text, start: int).

sh scan_span(path: text) -> (at: span) =
  \`printf '%s\\n' '{"at":{"start":17,"end":42}}'; : "$path"\`.

host_span(Path, At) <-
  source_path(Path),
  scan_span(Path, At).

host_start(Path, Start) <-
  host_span(Path, At),
  decode(At, {start: Start}).
`;

/** Advance an injected virtual scheduler by whole seconds and flush. `maxFrames`
 *  is bounded on purpose: a live `interval` reschedules itself every firing, so
 *  an unbounded flush would spin forever. */
function advance_virtual_seconds(scheduler: VirtualTimeScheduler, seconds: number): void {
  scheduler.maxFrames = scheduler.frame + seconds * 1000;
  scheduler.flush();
}

async function wait_until(predicate: () => boolean | Promise<boolean>, what: string, timeout_ms = 10_000): Promise<void> {
  const deadline = Date.now() + timeout_ms;
  while (!(await predicate())) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
}

test("receipt (b): live interval bind + live sh host, served, matches the oracle fed the same answers", async () => {
  const source = readFileSync(HOST_CLOCK_DL6, "utf8");
  const scheduler = new VirtualTimeScheduler();
  const served = await start_served(0, scheduler);
  try {
    const loaded = await post_program(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);
    const plans = JSON.parse(loaded.body) as {
      readonly hosts: readonly string[];
      readonly binds: readonly { readonly name: string; readonly literals: readonly (string | number)[] }[];
      readonly arrival_targets: readonly string[];
    };
    // The program's own rules said the cadence; nothing out here chose it.
    assert.deepEqual(plans.hosts, ["answer"]);
    assert.deepEqual(plans.binds, [{ name: "interval", literals: [1] }]);

    await post_arrivals(served.port, [{ rel: "seed", sign: "add", row: ["alpha"] }]);

    // The interval subscribes only after the program's boot SQL resolves, on
    // the real event loop; flushing before it registers advances nothing.
    await wait_until(() => scheduler.actions.length >= 1, "the interval to register on the injected scheduler");
    advance_virtual_seconds(scheduler, 1);

    // The bind's tick is synchronous with the flush; the host's subprocess and
    // its response commit are not.
    await wait_until(async () => {
      const answered = JSON.parse((await request(served.port, "/idb/answered", "GET")).body) as { rows: unknown[] };
      return answered.rows.length === 1;
    }, "the sh host's response to land in answered");

    const outcomes = tick_events(served.events);
    assert.equal(outcomes.length, 3, `expected seed / bind / host ticks, got ${outcomes.length}`);

    const replayed = schedule_from_ticks(outcomes, plans.arrival_targets);
    assert.deepEqual(
      replayed.map((batch) => batch.map((arrival) => arrival.rel)),
      [["seed"], ["interval"], ["__host_response_answer"]],
    );
    assert.equal(log_of_ticks(outcomes), oracle_log(source, replayed));

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
  const served = await start_served(0, scheduler);
  try {
    assert.equal((await post_program(served.port, source)).statusCode, 200);
    await wait_until(() => scheduler.actions.length === 1, "the first program's interval to register");

    // A program with no bind at all: the switchMap must leave zero timers.
    const plain = "rel event(id: int, kind: text) log keep(all).\nrel current(id: int, kind: text).\ncurrent(Id, Kind) <- event(Id, Kind).\n";
    assert.equal((await post_program(served.port, plain)).statusCode, 200);
    await wait_until(() => scheduler.actions.length === 0, "the swapped-out program's interval to be torn down");
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
 *   3. Written against the removed surface (`type span(...)` decl, `? probe`
 *      RHS rider) this program stopped at the parser: POST /program returned
 *      400 dl_parse_error(statement, "type span(end: int, start: int)...").
 *      The referenced relation is now an ORDINARY rel decl and the registered
 *      host is an ORDINARY RHS atom; the contract, not a marker, selects the
 *      demand-response lowering.
 *
 * The file-backed injected seam lets this one receipt count the target
 * relation's rows straight out of sqlite after the live host tick.
 *
 * SABOTAGE RECEIPT (run 2026-07-30, reverted): swapping the expected target
 * delta to the other column order (`span` add `[[17, 42]]`) turns this test red
 * (2 pass, 1 fail), so the public-target assertions read the real tick log and
 * pin the declared column order rather than merely observing a non-empty key.
 */
test("declared-struct live host output interns once and tick logs render the canonical value", async () => {
  const directory = mkdtempSync(join(tmpdir(), "tsv2-struct-host-"));
  const db_url = `file:${join(directory, "served.sqlite")}`;
  const served = await start_served(0, undefined, db_url);
  try {
    const loaded = await post_program(served.port, STRUCT_HOST_DL6);
    assert.equal(loaded.statusCode, 200, loaded.body);

    await post_arrivals(served.port, [
      { rel: "source_path", sign: "add", row: ["a.rs"] },
    ]);
    await wait_until(async () => {
      const rows = JSON.parse((await request(served.port, "/idb/host_start", "GET")).body) as {
        rows: readonly (readonly (string | number)[])[];
      };
      return rows.rows.length === 1;
    }, "the struct-typed host answer to land");

    const host_span = JSON.parse((await request(served.port, "/idb/host_span", "GET")).body) as {
      rows: readonly (readonly (string | number)[])[];
    };
    const host_start = JSON.parse((await request(served.port, "/idb/host_start", "GET")).body) as {
      rows: readonly (readonly (string | number)[])[];
    };
    assert.deepEqual(host_span.rows, [["a.rs", '{"end":42,"start":17}']]);
    assert.deepEqual(host_start.rows, [["a.rs", 17]]);

    const outcomes = tick_events(served.events);
    assert.equal(outcomes.length, 2, `expected source / host ticks, got ${outcomes.length}`);
    const host_tick = JSON.parse(outcomes[1]!.line) as {
      deltas: Record<string, { add: readonly (readonly unknown[])[]; del: readonly (readonly unknown[])[] }>;
    };
    assert.deepEqual(host_tick.deltas["__host_response_scan_span"]?.add, [
      [
        "witness|scan_span|path:text=a.rs",
        0,
        "a.rs",
        { end: 42, start: 17 },
      ],
    ]);
    assert.deepEqual(host_tick.deltas.host_span?.add, [
      ["a.rs", { end: 42, start: 17 }],
    ]);
    assert.deepEqual(host_tick.deltas.host_start?.add, [["a.rs", 17]]);
    // The referenced relation is ordinary and PUBLIC: the target row enters the
    // same tick as the parent that references it, in its own declared column
    // order, and is queryable by name. Under the dictionary model this row was
    // storage-plane only and appeared in neither place.
    assert.deepEqual(host_tick.deltas.span?.add, [[42, 17]]);
    const span_rows = JSON.parse((await request(served.port, "/idb/span", "GET")).body) as {
      rows: readonly (readonly (string | number)[])[];
    };
    assert.deepEqual(span_rows.rows, [[42, 17]]);

    const inspection = ScratchStore.open(db_url);
    try {
      const target_rows = await firstValueFrom(
        inspection.runner.scalar(
          inspection.db,
          'SELECT count(*) FROM "span"',
        ),
      );
      assert.equal(target_rows, 1);
    } finally {
      inspection.db.close();
    }
  } finally {
    await served.stop();
    rmSync(directory, { recursive: true, force: true });
  }
});
