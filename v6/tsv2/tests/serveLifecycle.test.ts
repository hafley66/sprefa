/**
 * serveLifecycle.test.ts — the served engine's SHUTDOWN contract, which had no
 * test at all before this file and cost the flow rig its results because of it.
 *
 * TWO BUGS, both from the session ledger's bug/3 section, both fixed and pinned
 * here:
 *
 *   serve_lifecycle_idb_read_race
 *     `closeServer$` disposed the program FIRST and closed the http server
 *     SECOND. Disposing closes the sqlite handle, so anything the server was
 *     still serving lost its data source mid-answer: the rig curl'd its final
 *     relations after a fully successful run and wrote empty TSVs. `4_http.ts`
 *     now closes first -- `server.close(callback)` stops accepting and calls
 *     back only once every open connection has ended -- and disposes inside that
 *     callback.
 *
 *   hostdecode_hardcoded_port_collision
 *     every served receipt named a constant port (17521, 17531, ..., 17611) and
 *     collided as EADDRINUSE when two lanes ran the suite in one tree.
 *     `startServed()` now defaults to the ephemeral port 0 and callers read
 *     `served.port` back; `reservePort()` supplies an address for the receipts
 *     that need one NOT to be listening.
 *
 * WHY THE FIRST TEST IS SHAPED THE WAY IT IS. Two simpler shapes were tried and
 * both measure nothing, which is worth recording because they look convincing:
 *
 *   - fire a request and close at the same moment: the client's own "connected"
 *     does not mean the server ACCEPTED the connection, so closing the listening
 *     socket drops it and the client sees ECONNREFUSED. Correct behaviour for a
 *     closed server; silent about the drain.
 *   - hold a keep-alive socket and reuse it after close: node closes IDLE
 *     keep-alive connections as part of `server.close()`, so the reused socket is
 *     reset. Also correct; also silent.
 *
 * What survives `close()` is a request the server is STILL SERVING. An SSE
 * client on `/ticks` is exactly that -- accepted, long-lived, and counted
 * server-side by `activeSubscribeCount()` so the test proves acceptance rather
 * than assuming it. With that connection open, the program's interval bind is
 * advanced on an injected virtual scheduler: the tick it commits has to READ AND
 * WRITE the sqlite handle. Under the old order the handle is already closed when
 * that happens; under the new one it stays alive until the SSE socket goes.
 *
 * SABOTAGE RECEIPTS (both run, both reverted):
 *   (a) restoring `disposeProgram(state)` above `server.close(...)` in
 *       4_http.ts turns the first test red -- the post-close tick never reaches
 *       the SSE client, because the tick loop faults on a closed database.
 *   (b) pinning `startServed`'s default back to a constant turns the third test
 *       red with EADDRINUSE the moment two servers are asked for at once.
 */

import assert from "node:assert/strict";
import http from "node:http";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { VirtualTimeScheduler } from "rxjs";

import { postArrivals, postProgram, request, reservePort, startServed } from "./serveHelpers.ts";

const HOST_CLOCK_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-host-clock.dl6", import.meta.url));

/** Small enough to read in one glance, big enough that `/idb/current` answers
 *  from a real table rather than an empty one. */
const PROGRAM = [
  "rel event(id: int, kind: text) log keep(all).",
  "rel current(id: int, kind: text).",
  "current(Id, Kind) <- event(Id, Kind).",
  "",
].join("\n");

interface SseClient {
  readonly lines: readonly string[];
  readonly drop: () => void;
}

/** An SSE client that stays attached and collects every event it is sent. */
function sseClient(port: number): Promise<SseClient> {
  return new Promise((resolve, reject) => {
    const lines: string[] = [];
    const outgoing = http.request(
      { hostname: "127.0.0.1", port, path: "/ticks", method: "GET", agent: false },
      (response) => {
        response.on("data", (chunk: Buffer) => lines.push(chunk.toString()));
        resolve({ lines, drop: () => outgoing.destroy() });
      },
    );
    outgoing.on("error", (failure) => {
      // Dropping the socket at the end of the receipt is the point; its
      // teardown error is the expected end, not a failure.
      if (!(failure instanceof Error) || !/socket hang up|ECONNRESET/.test(failure.message)) reject(failure);
    });
    outgoing.end();
  });
}

async function waitUntil(predicate: () => boolean, what: string, timeoutMs = 10_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
}

test("the program outlives close() while a request is still being served, and is dropped only after", async () => {
  const scheduler = new VirtualTimeScheduler();
  const served = await startServed(0, scheduler);
  const source = readFileSync(HOST_CLOCK_DL6, "utf8");
  assert.equal((await postProgram(served.port, source)).statusCode, 200);
  await postArrivals(served.port, [{ rel: "seed", sign: "add", row: ["alpha"] }]);
  await waitUntil(() => scheduler.actions.length >= 1, "the interval bind to register");

  const client = await sseClient(served.port);
  await waitUntil(() => served.activeSubscribeCount() === 1, "the SSE client to be accepted server-side");

  // close() begins here and must NOT resolve: one connection is still live.
  let resolved = false;
  const closed = served.stop().then(() => {
    resolved = true;
  });
  await new Promise<void>((resolve) => setTimeout(resolve, 100));
  assert.equal(resolved, false, "stop() resolved while a request was still being served");

  // A tick AFTER close() was asked for. It reads and writes the sqlite handle,
  // so it can only happen if the program was not disposed out from under it.
  const before = client.lines.length;
  scheduler.maxFrames = scheduler.frame + 1000;
  scheduler.flush();
  await waitUntil(() => client.lines.length > before, "a tick to reach the SSE client after close() began");

  client.drop();
  await closed;
  assert.equal(resolved, true);

  // And now it really is gone: nothing is listening.
  await assert.rejects(
    () => request(served.port, "/idb/answered", "GET"),
    (failure: NodeJS.ErrnoException) => failure.code === "ECONNREFUSED",
  );
});

test("stop() resolves only after the port is actually released, so the next server can take it", async () => {
  const first = await startServed();
  const port = first.port;
  assert.equal((await postProgram(port, PROGRAM)).statusCode, 200);
  await first.stop();

  // The old stop() unsubscribed and slept 25ms; nothing guaranteed the listener
  // was gone. Rebinding the SAME port is the discriminating check.
  const second = await startServed(port);
  try {
    assert.equal(second.port, port);
  } finally {
    await second.stop();
  }
});

test("servers started with no port asked for never collide", async () => {
  const servers = await Promise.all([startServed(), startServed(), startServed()]);
  try {
    const ports = servers.map((served) => served.port);
    assert.equal(new Set(ports).size, ports.length, `ephemeral ports repeated: ${ports.join(",")}`);
    for (const port of ports) assert.ok(port > 0, "an ephemeral port must be reported, not 0");
  } finally {
    await Promise.all(servers.map((served) => served.stop()));
  }
});

test("reservePort hands back an address nothing is listening on", async () => {
  const idle = await reservePort();
  assert.ok(idle > 0);
  await assert.rejects(
    () => request(idle, "/idb/current", "GET"),
    (failure: NodeJS.ErrnoException) => failure.code === "ECONNREFUSED",
    "reservePort must yield a closed address, not a live one",
  );
});
