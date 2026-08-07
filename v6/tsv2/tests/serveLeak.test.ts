/**
 * serveLeak.test.ts — RECEIPT (c) of the runtime-bridge arc
 * (plans/2026-07-29-runtime-bridge-header.md scope 4c): 20 program-swap cycles
 * on the served engine, then five assertions about what the process is still
 * holding.
 *
 * Gated on DL_PERF_LOG, like v6/dl's own soak: the run needs the tracing JSONL
 * to assert statements-per-tick, and it is slow enough that `npm test` should
 * not pay for it. `scripts/leak-soak.sh` is the entry point.
 *
 * Each cycle: swap to program A, post an arrival, open an SSE client, read one
 * tick, drop the SSE client, swap to program B (which carries a bind and a host
 * that A does not), post an arrival there too. Every seam the arc added is
 * exercised: the compiler's dynamic import, a fresh SQLite connection, the tick
 * loop, an interval timer, a spawned subprocess, and an SSE inner.
 *
 * SABOTAGE RECEIPTS (run 2026-07-29, all reverted):
 *   - dropping `bumpActive(-1)` from 4_http.ts's SSE `finalize` fails at
 *     "timeout waiting for SSE teardown (iteration 0)".
 *   - merging each bind's timers twice in 2_binds.ts fails at "timeout waiting
 *     for the interval timer to be the only one (iteration 0)", which is the
 *     swap-teardown assertion doing its job.
 *   - MEASURED AND HONEST: removing `state.seam?.db.close()` from
 *     `disposeProgram` does NOT flip anything. A local libsql connection
 *     registers no node handle and no active resource, so neither assertion 1
 *     nor 2 can see it leak. Naming the blind spot rather than claiming a
 *     receipt this test does not earn.
 */

import assert from "node:assert/strict";
import http from "node:http";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { VirtualTimeScheduler } from "rxjs";

import { post_arrivals, post_program, request, start_served } from "./serveHelpers.ts";

const HOST_CLOCK_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-host-clock.dl6", import.meta.url));
const PLAIN_PROGRAM = `rel event(id: int, kind: text) log keep(all).
rel current(id: int, kind: text).
current(Id, Kind) <- event(Id, Kind).
`;

const ITERATIONS = Number(process.env.TSV2_LEAK_ITERATIONS ?? "20");
// 0 = ephemeral. TSV2_LEAK_PORT still pins it for the soak script, which needs
// a known address to curl; the in-process receipt never does.
const PORT = Number(process.env.TSV2_LEAK_PORT ?? "0");
const PERF_LOG_PATH = process.env.DL_PERF_LOG;
const SOAK_ENABLED = PERF_LOG_PATH !== undefined;

interface ResourceCounts {
  readonly handles: Readonly<Record<string, number>>;
  readonly resources: Readonly<Record<string, number>>;
}

function count_names(names: readonly string[]): Readonly<Record<string, number>> {
  const counts: Record<string, number> = {};
  for (const name of names) counts[name] = (counts[name] ?? 0) + 1;
  return counts;
}

function resource_counts(): ResourceCounts {
  const active_handles = (process as NodeJS.Process & { _getActiveHandles(): unknown[] })._getActiveHandles();
  return {
    handles: count_names(
      active_handles.map(
        (handle: unknown) => (handle as { readonly constructor?: { readonly name?: string } }).constructor?.name ?? "unknown",
      ),
    ),
    resources: count_names(process.getActiveResourcesInfo()),
  };
}

function names_with_growth(
  before: Readonly<Record<string, number>>,
  after: Readonly<Record<string, number>>,
): readonly string[] {
  const names = new Set([...Object.keys(before), ...Object.keys(after)]);
  return [...names]
    .filter((name) => (after[name] ?? 0) > (before[name] ?? 0))
    .sort()
    .map((name) => `${name} ${before[name] ?? 0}->${after[name] ?? 0}`);
}

/** One SSE client: connect, read one event, drop the socket. */
function sse_once(port: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const outgoing = http.request({ hostname: "127.0.0.1", port, path: "/ticks", method: "GET", agent: false }, (response) => {
      response.on("data", (chunk: Buffer) => {
        resolve(chunk.toString());
        outgoing.destroy();
      });
    });
    outgoing.on("error", (failure) => {
      // `destroy()` after the first event is the point of the test, so the
      // socket-teardown error it raises is the expected end, not a failure.
      if (!(failure instanceof Error) || !/socket hang up|ECONNRESET/.test(failure.message)) reject(failure);
    });
    outgoing.end();
  });
}

async function wait_until(predicate: () => boolean | Promise<boolean>, what: string, timeout_ms = 15_000): Promise<void> {
  const deadline = Date.now() + timeout_ms;
  while (!(await predicate())) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
}

test("receipt (c): 20 program-swap cycles leave no handle, timer, or subscription behind", { skip: SOAK_ENABLED ? false : "set DL_PERF_LOG (scripts/leak-soak.sh)" }, async () => {
  const host_clock_source = readFileSync(HOST_CLOCK_DL6, "utf8");
  const scheduler = new VirtualTimeScheduler();
  const served = await start_served(PORT, scheduler);
  let baseline: ResourceCounts | null = null;
  let baseline_rss = 0;

  try {
    for (let iteration = 0; iteration < ITERATIONS; iteration += 1) {
      assert.equal((await post_program(served.port, PLAIN_PROGRAM)).statusCode, 200);
      await post_arrivals(served.port, [{ rel: "event", sign: "add", row: [iteration, "cycle"] }]);

      const sse_promise = sse_once(served.port);
      await wait_until(() => served.active_subscribe_count() === 1, `SSE client to register (iteration ${iteration})`);
      await post_arrivals(served.port, [{ rel: "event", sign: "add", row: [iteration, "sse"] }]);
      await sse_promise;
      await wait_until(() => served.active_subscribe_count() === 0, `SSE teardown (iteration ${iteration})`);

      assert.equal((await post_program(served.port, host_clock_source)).statusCode, 200);
      await post_arrivals(served.port, [{ rel: "seed", sign: "add", row: [`alpha${iteration}`] }]);
      // ASSERTION 4, every iteration: one program, one interval timer. A swap
      // that failed to tear the previous program's timer down would grow this.
      await wait_until(() => scheduler.actions.length === 1, `the interval timer to be the only one (iteration ${iteration})`);

      // Two warmup iterations before the baseline: the first swap pays for
      // pino's file handle, node's module cache, and libsql's own lazy setup.
      if (iteration === 1) {
        baseline = resource_counts();
        baseline_rss = process.memoryUsage().rss;
      }
    }

    assert.ok(baseline, "baseline sampled");
    const after = resource_counts();
    // ASSERTION 1+2: nothing grew, by resource type and by handle type.
    assert.deepEqual(names_with_growth(baseline.resources, after.resources), []);
    assert.deepEqual(names_with_growth(baseline.handles, after.handles), []);
    // ASSERTION 3: every SSE inner is gone.
    assert.equal(served.active_subscribe_count(), 0);
    // ASSERTION 5: RSS bounded past warmup.
    const grown_rss = process.memoryUsage().rss;
    assert.ok(grown_rss < baseline_rss * 1.25, `rss ${baseline_rss} -> ${grown_rss}`);

    // ASSERTION 6 (the count-test law): statements per tick do not grow with
    // the number of swaps. The plain program's ticks are the comparable ones.
    const lines = readFileSync(PERF_LOG_PATH!, "utf8").split("\n").filter((line) => line.trim().length > 0);
    const statement_counts = lines
      .map((line) => JSON.parse(line) as { readonly statements: number; readonly rows: number })
      .filter((entry) => entry.rows > 0)
      .map((entry) => entry.statements);
    assert.ok(statement_counts.length >= ITERATIONS, `perf log has ${statement_counts.length} data ticks`);
    const first = statement_counts.slice(0, 5);
    const last = statement_counts.slice(-5);
    assert.ok(
      Math.max(...last) <= Math.max(...first),
      `statements per tick grew across swaps: first ${JSON.stringify(first)} last ${JSON.stringify(last)}`,
    );
  } finally {
    await served.stop();
  }
});
