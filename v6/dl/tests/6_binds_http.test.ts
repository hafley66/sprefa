/**
 * tests/6_binds_http.test.ts -- the bind seam's end-to-end receipt, through serveDl
 * exactly the way main.ts composes the real app (boot on a scratch db/port, load a
 * program over http, watch the world-fed rel advance on its own). Mirrors
 * tests/6_http.test.ts's bootTestServer/teardownTestServer shape (duplicated here
 * rather than imported: that file doesn't export them, and this file owns no edits
 * to it).
 *
 * Proves, against fixtures/clock-swr-demo.dl:
 *   1. clock_bucket (the clock bind's own EDB rel) advances at least twice within a
 *      few declared periods, with NO code in this test ever POSTing to /edb/clock_bucket
 *      -- the row arrives purely from the bind's own interval timer.
 *   2. poll_due (a plain derived rule reading clock_bucket) re-fires on every advance,
 *      observed over GET /subscribe/poll_due -- SSE delta events, not just a snapshot
 *      read, so "re-fired" means a real tick delta, not merely "the final value differs".
 *   3. Reloading the server to a program with NO `clock_period` rel stops the old
 *      timer: clock_bucket does not advance again after the reload, proving bind
 *      lifetime is scoped to its program the same way host effects are (switchMap's
 *      unsubscribe of the old program's whole branch, 6_http.ts's runProgram$).
 */
import assert from "node:assert/strict";
import http from "node:http";
import { test } from "node:test";

import type { Subscription } from "rxjs";

import { serveDl, type DlServer } from "../src/6_http.ts";
import { cleanupDbFile, freshDbPath } from "./1_helpers_db.ts";
import { readFixture } from "./0_helpers.ts";
import { waitUntil } from "./2_helpers_hosts.ts";

interface ServerFixture {
  readonly server: DlServer;
  readonly dbPath: string;
  readonly base: string;
  readonly running: Subscription;
}

async function bootTestServer(): Promise<ServerFixture> {
  const dbPath = freshDbPath();
  let running: Subscription | undefined;
  const server = await new Promise<DlServer>((resolve, reject) => {
    running = serveDl({ dbPath, port: 0 }).subscribe({
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

test("clock bind: clock_bucket advances on its own, poll_due re-fires on SSE, reload stops the timer", async () => {
  const fixture = await bootTestServer();
  const sseEvents: string[] = [];
  let sseReq: http.ClientRequest | undefined;
  try {
    await loadProgram(fixture.base, readFixture("clock-swr-demo.dl"));

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

    // ---- reload: swap to a program with NO clock_period -> the old timer dies ----
    sseReq!.destroy();
    await loadProgram(fixture.base, NO_PERIOD_PROGRAM);

    const snapshotAfterReload = await readRel(fixture.base, "clock_bucket");
    // Longer than one declared period in the OLD program (2s): if the old interval
    // somehow survived the switchMap teardown, this window would catch its firing.
    await new Promise((resolve) => setTimeout(resolve, 3000));
    const snapshotAfterWaiting = await readRel(fixture.base, "clock_bucket");
    assert.deepEqual(
      snapshotAfterWaiting,
      snapshotAfterReload,
      "clock_bucket must not change after reloading to a program with no clock_period -- the old program's timer must be dead",
    );
  } finally {
    sseReq?.destroy();
    await teardownTestServer(fixture);
  }
});
