/**
 * tests/6_binds_http.test.ts -- the bind seam's end-to-end receipt, through serveDl
 * exactly the way main.ts composes the real app (boot on a scratch db/port, load a
 * program over http, watch the world-fed rel advance on its own). Mirrors
 * tests/6_http.test.ts's bootTestServer/teardownTestServer shape (duplicated here
 * rather than imported: that file doesn't export them, and this file owns no edits
 * to it).
 *
 * Two tests, split along the SAME real-vs-virtual-time line 4_binds.test.ts draws
 * (that file's header explains the reason in full):
 *
 *   1. "clock_bucket advances ... poll_due re-fires on SSE" -- kept on the REAL
 *      default scheduler. Its assertions depend on `bucketFor` (1_binds.ts) computing
 *      two DIFFERENT bucket values across two firings, and `bucketFor` reads real
 *      `Date.now()` regardless of which scheduler drives the interval's CADENCE
 *      (this seam deliberately does not virtualize wall-clock bucket VALUES). Proves,
 *      against fixtures/clock-swr-demo.dl:
 *        a. clock_bucket advances at least twice within a few declared periods, with
 *           NO code in this test ever POSTing to /edb/clock_bucket -- the row arrives
 *           purely from the bind's own interval timer.
 *        b. poll_due (a plain derived rule reading clock_bucket) re-fires on every
 *           advance, observed over GET /subscribe/poll_due -- SSE delta events, not
 *           just a snapshot read, so "re-fired" means a real tick delta.
 *
 *   2. "reloading ... removes the old interval's scheduled action" -- rewritten onto
 *      an injected rxjs TestScheduler (0_types.ts's ServeDl.scheduler seam,
 *      threaded through the SAME cfg object serveDl already takes -- 6_http.ts had no
 *      other injection point, so this is the "parameterize through the existing
 *      config object" call the dispatch asked for rather than a module-state hack).
 *      Proves reloading the server to a program with NO `clock_period` rel removes
 *      the OLD program's scheduled interval action -- the SAME "assert the timer, not
 *      the row" direction F3 named, because a leaked interval whose firings land the
 *      same real wall-clock second re-commits an IDENTICAL (period, bucket) row under
 *      retention-1, which row-equality alone cannot tell apart from a dead timer.
 *
 * SABOTAGE RECEIPT (F3, mandatory per the dispatch, shared with 4_binds.test.ts):
 * 1_binds.ts's BindRunner constructor was temporarily rewritten to hold an internal
 * Subject-backed subscription that never unsubscribes from the merged bind sources
 * (see 4_binds.test.ts's header for the exact diff). Test 2 below went RED under
 * that sabotage: `waitForScheduledAction(scheduler, count => count === 0)` timed out
 * after its bounded real poll (2s) because the leaked action never left the
 * scheduler's queue, so `scheduler.actions.length` never reached 0 after the
 * reload -- a discriminating failure (a timeout rather than a direct assertion
 * mismatch only because that predicate wait sits before the `assert.equal`, to
 * absorb the same async settling gap 4_binds.test.ts's `waitForScheduledAction`
 * call absorbs). Test 1 was unaffected (it never inspects the scheduler's action
 * queue). The sabotage was reverted before this file was finalized; test 2 was
 * re-run GREEN against the real implementation.
 */
import assert from "node:assert/strict";
import http from "node:http";
import { test } from "node:test";

import type { Subscription } from "rxjs";
import { TestScheduler } from "rxjs/testing";

import { serveDl, type DlServer } from "../src/6_http.ts";
import { advanceVirtualSeconds, waitForScheduledAction } from "./2_helpers_binds.ts";
import { cleanupDbFile, freshDbPath } from "./1_helpers_db.ts";
import { readFixture } from "./0_helpers.ts";
import { waitUntil } from "./2_helpers_hosts.ts";

interface ServerFixture {
  readonly server: DlServer;
  readonly dbPath: string;
  readonly base: string;
  readonly running: Subscription;
}

/** `scheduler` defaults to omitted (serveDl's own default, rxjs's `asyncScheduler`,
 *  real wall-clock) -- test 2 below passes a `TestScheduler` to run a bind's cadence
 *  on virtual time. */
async function bootTestServer(scheduler?: TestScheduler): Promise<ServerFixture> {
  const dbPath = freshDbPath();
  let running: Subscription | undefined;
  const server = await new Promise<DlServer>((resolve, reject) => {
    running = serveDl({ dbPath, port: 0, scheduler }).subscribe({
      next: (event) => {
        if (event.kind === "listening") resolve(event.server);
      },
      error: reject,
    });
  });
  return { server, dbPath, base: `http://127.0.0.1:${server.port}`, running: running! };
}

async function teardownTestServer(fixture: ServerFixture): Promise<void> {
  await fixture.server.close();
  fixture.running.unsubscribe();
  cleanupDbFile(fixture.dbPath);
}

async function loadProgram(base: string, text: string): Promise<void> {
  const response = await fetch(`${base}/edb/program`, { method: "POST", body: text });
  assert.equal(response.status, 200, `program load failed: ${await response.text()}`);
}

async function readRel(base: string, rel: string): Promise<{ period: number; bucket: number }[]> {
  const response = await fetch(`${base}/idb/${rel}`);
  assert.equal(response.status, 200);
  const body = (await response.json()) as { rows: { period: number; bucket: number }[] };
  return body.rows;
}

// A second program, loaded over the same server, that still declares `clock_bucket`
// (so a stale row could in principle still occupy the table) but declares NO
// `clock_period` rel at all -- clockBind's own tolerance for a missing config rel
// (1_binds.ts: an "unknown rel" read resolves to zero configured periods) means this
// activates the bind with zero intervals, i.e. the clock never fires again.
const NO_PERIOD_PROGRAM = `rel(1) clock_bucket(period: int, bucket: int).
rel poll_due(period: int, bucket: int).
poll_due(period, bucket) <- clock_bucket(period, bucket).
`;

test("clock bind: clock_bucket advances on its own, poll_due re-fires on SSE", async () => {
  const fixture = await bootTestServer();
  const sseEvents: string[] = [];
  let sseReq: http.ClientRequest | undefined;
  try {
    await loadProgram(fixture.base, readFixture("clock-swr-demo.dl6"));

    // SSE on poll_due: every real re-derivation lands here as its own `data:` line,
    // independent of clock_bucket's own retention-1 snapshot.
    await new Promise<void>((resolve, reject) => {
      sseReq = http.get(`${fixture.base}/subscribe/poll_due`, (res) => {
        assert.equal(res.statusCode, 200);
        res.on("data", (chunk: Buffer) => {
          const text = chunk.toString("utf8");
          for (const line of text.split("\n")) {
            if (line.startsWith("data: ")) sseEvents.push(line.slice("data: ".length));
          }
        });
        resolve();
      });
      sseReq.on("error", reject);
    });

    // First advance: no row -> one row. NOT posted by this test -- purely the bind.
    const firstBucket = await waitUntil(
      async () => {
        const rows = await readRel(fixture.base, "clock_bucket");
        return rows.length === 1 ? rows[0]!.bucket : undefined;
      },
      { attempts: 150, intervalMs: 100 }, // up to 15s; demo period is 2s
    );

    // Second advance: a strictly greater bucket for the same period.
    const secondBucket = await waitUntil(
      async () => {
        const rows = await readRel(fixture.base, "clock_bucket");
        return rows.length === 1 && rows[0]!.bucket > firstBucket ? rows[0]!.bucket : undefined;
      },
      { attempts: 150, intervalMs: 100 },
    );
    assert.ok(secondBucket > firstBucket, "clock_bucket must advance at least twice");

    // poll_due mirrors the same current value (the derived rule tracking the bind's
    // own EDB rel).
    const pollDueRows = await readRel(fixture.base, "poll_due");
    assert.deepEqual(pollDueRows, [{ period: 2, bucket: secondBucket }]);

    // The re-fire receipt: at least two distinct poll_due delta events reached this
    // SSE client, one per real advance, proving the derived rule re-ran rather than
    // this test only observing the final snapshot.
    assert.ok(sseEvents.length >= 2, `expected >=2 poll_due SSE events, got ${sseEvents.length}: ${sseEvents.join(" | ")}`);
  } finally {
    sseReq?.destroy();
    await teardownTestServer(fixture);
  }
});

test("clock bind: reloading to a program with no clock_period removes the old interval's scheduled action", async () => {
  const scheduler = new TestScheduler(() => {});
  const fixture = await bootTestServer(scheduler);
  try {
    // A one-second period keeps this fast, same reasoning as 4_binds.test.ts's
    // ONE_SECOND_CLOCK_PROGRAM (the demo fixture's 2s period is exercised by the
    // first test above, on real time).
    await loadProgram(
      fixture.base,
      `rel clock_period(period_secs: int).
clock_period(1).

rel(1) clock_bucket(period: int, bucket: int).
`,
    );

    await waitForScheduledAction(scheduler, (actionCount) => actionCount > 0);
    advanceVirtualSeconds(scheduler, 1);
    await waitUntil(async () => {
      const rows = await readRel(fixture.base, "clock_bucket");
      return rows.length === 1 ? rows : undefined;
    });
    assert.ok(
      scheduler.actions.length > 0,
      "the clock's interval action must still be scheduled while the first program is running",
    );

    // ---- reload: swap to a program with NO clock_period -> the old timer dies ----
    await loadProgram(fixture.base, NO_PERIOD_PROGRAM);

    // switchMap (6_http.ts's runProgram$, `accepted$.pipe(switchMap(...))`) tears
    // down the old program's whole branch -- including the old BindRunner's
    // commits$, hence the old interval's scheduled action -- as soon as the new
    // accepted$ value arrives, which happens before the new program's own async
    // boot completes. `waitForScheduledAction` is still a bounded REAL poll (not a
    // stand-in for the cadence): it is here only because `loadProgram`'s 200
    // response is written from further down the SAME merge than the switchMap
    // teardown, so there is a small async gap between "response received" and
    // "old action definitely gone" to close.
    await waitForScheduledAction(scheduler, (actionCount) => actionCount === 0);
    assert.equal(
      scheduler.actions.length,
      0,
      "reloading to a program with no clock_period must remove the old interval's scheduled action -- no leaked timer",
    );

    // Advance virtual time well past several more periods of the OLD program: a
    // leaked subscription would reschedule a new action here (confirmed: this line
    // goes RED under the held-internal-subscription sabotage described in this
    // file's header). The new program declares no clock_period, so clockBind's own
    // tolerance for a missing config rel (1_binds.ts) keeps this at zero regardless.
    advanceVirtualSeconds(scheduler, 5);
    assert.equal(scheduler.actions.length, 0, "no new scheduled action may appear after the reload");
  } finally {
    await teardownTestServer(fixture);
  }
});
