/**
 * bopLoadQuery.test.ts — `bop load` and `bop q`, both plain HTTP clients
 * against an ALREADY RUNNING server. The server itself is `startServed`
 * (tests/serveHelpers.ts) -- the same in-process `serveTsv2` every other
 * served-engine receipt in this directory boots -- rather than a spawned
 * `bop serve` subprocess: `load`/`q` only ever speak the existing
 * `/program`/`/idb/:rel` http routes, so grading them against the routes
 * directly is the same contract with one fewer process in the way.
 *
 * `runBop` uses ASYNC `spawn`, never `spawnSync`, and this is load-bearing,
 * not a style choice: the server lives in THIS test process's own event
 * loop (that is what "in-process" means), and `spawnSync` blocks that exact
 * event loop until the child exits -- which deadlocks the moment the child
 * is a `bop load`/`bop q` waiting on an HTTP response from the very server
 * the blocked loop can no longer service. Measured, not assumed: the first
 * draft used `spawnSync` and every test in this file hung forever (server
 * process alive, socket accepted, zero bytes ever written back); switching
 * to `spawn` + a Promise around its `close` event fixed it outright. This is
 * the one file in this arc where the CLI rim's own "async becomes rxjs; sync
 * stays sync" exemption does NOT cover a synchronous child-process call.
 *
 * PORT NOTE: 17580-17582 are not used by any other test file (grepped).
 */

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { startServed } from "./serveHelpers.ts";

const BOP = fileURLToPath(new URL("../cli/bop.ts", import.meta.url));
const DOOR_DL6 = fileURLToPath(new URL("../../dl/fixtures/door-handwritten.dl6", import.meta.url));

interface BopResult {
  readonly status: number | null;
  readonly stdout: string;
  readonly stderr: string;
}

function runBop(args: readonly string[]): Promise<BopResult> {
  return new Promise((resolvePromise) => {
    const child = spawn("node", ["--experimental-transform-types", BOP, ...args]);
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    child.on("close", (status) => resolvePromise({ status, stdout, stderr }));
  });
}

test("load then q: a program POSTed by `bop load` is readable by `bop q` on the same running server", async () => {
  const served = await startServed(17580);
  try {
    const loaded = await runBop(["load", DOOR_DL6, "--port", "17580"]);
    assert.equal(loaded.status, 0, loaded.stderr);
    const loadedBody = JSON.parse(loaded.stdout) as { readonly loaded: boolean; readonly rels: readonly string[] };
    assert.equal(loadedBody.loaded, true);
    assert.ok(loadedBody.rels.includes("event"), `expected 'event' among rels, got: ${loadedBody.rels.join(",")}`);

    const queriedJson = await runBop(["q", "event", "--port", "17580", "--json"]);
    assert.equal(queriedJson.status, 0, queriedJson.stderr);
    const rows = (JSON.parse(queriedJson.stdout) as { readonly rows: readonly unknown[] }).rows;
    assert.equal(rows.length, 0, "door-handwritten.dl6 seeds no event rows");

    const queriedTable = await runBop(["q", "event", "--port", "17580"]);
    assert.equal(queriedTable.status, 0, queriedTable.stderr);
    assert.equal(queriedTable.stdout, "", "zero rows render as zero lines in table mode");
  } finally {
    await served.stop();
  }
});

test("q: nothing listening on the port exits 1 with a clear message, never a stack trace", async () => {
  const outcome = await runBop(["q", "event", "--port", "17581"]);
  assert.equal(outcome.status, 1);
  assert.match(outcome.stderr, /no server listening on port 17581/);
});

test("load: nothing listening on the port exits 1 with a clear message", async () => {
  const outcome = await runBop(["load", DOOR_DL6, "--port", "17581"]);
  assert.equal(outcome.status, 1);
  assert.match(outcome.stderr, /no server listening on port 17581/);
});

test("q: a running server with no program loaded exits 1 (404 'no program loaded')", async () => {
  const served = await startServed(17582);
  try {
    const outcome = await runBop(["q", "event", "--port", "17582"]);
    assert.equal(outcome.status, 1, outcome.stdout);
    assert.match(outcome.stderr, /no program loaded/);
  } finally {
    await served.stop();
  }
});

test("load: a program that hits a named compiler refusal over http exits 2, not 1", async () => {
  const served = await startServed(17583);
  try {
    const ghcacherDl6 = fileURLToPath(new URL("../../dl/fixtures/ghcacher.dl6", import.meta.url));
    const outcome = await runBop(["load", ghcacherDl6, "--port", "17583"]);
    assert.equal(outcome.status, 2, outcome.stderr);
    assert.match(outcome.stderr, /unsupported_construct/);
  } finally {
    await served.stop();
  }
});
