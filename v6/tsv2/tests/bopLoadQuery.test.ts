/**
 * bopLoadQuery.test.ts — `bop load` and `bop q`, both plain HTTP clients
 * against an ALREADY RUNNING server. The server itself is `startServed`
 * (tests/serveHelpers.ts) -- the same in-process `serve_tsv2` every other
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
 * PORT NOTE, corrected: this file used to name 17580-17584 and claim they were
 * "not used by any other test file (grepped)". That check was of the TREE, not
 * of the machine, and the whole class bit as EADDRINUSE the moment two lanes ran
 * the suite at once. Every server here now binds an ephemeral port and the CLI
 * is told `served.port`; the two negative receipts ask `reservePort()` for an
 * address that is definitely not listening instead of hoping a constant is free.
 */

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { reserve_port, start_served } from "./serveHelpers.ts";

const BOP = fileURLToPath(new URL("../cli/bop.ts", import.meta.url));
const DOOR_DL6 = fileURLToPath(new URL("../../dl/fixtures/door-handwritten.dl6", import.meta.url));

interface BopResult {
  readonly status: number | null;
  readonly stdout: string;
  readonly stderr: string;
}

function run_bop(args: readonly string[]): Promise<BopResult> {
  return new Promise((resolve_promise) => {
    const child = spawn("node", ["--experimental-transform-types", BOP, ...args]);
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    child.on("close", (status) => resolve_promise({ status, stdout, stderr }));
  });
}

test("load then q: a program POSTed by `bop load` is readable by `bop q` on the same running server", async () => {
  const served = await start_served();
  const port = String(served.port);
  try {
    const loaded = await run_bop(["load", DOOR_DL6, "--port", port]);
    assert.equal(loaded.status, 0, loaded.stderr);
    const loaded_body = JSON.parse(loaded.stdout) as { readonly loaded: boolean; readonly rels: readonly string[] };
    assert.equal(loaded_body.loaded, true);
    assert.ok(loaded_body.rels.includes("event"), `expected 'event' among rels, got: ${loaded_body.rels.join(",")}`);

    const queried_json = await run_bop(["q", "event", "--port", port, "--json"]);
    assert.equal(queried_json.status, 0, queried_json.stderr);
    const rows = (JSON.parse(queried_json.stdout) as { readonly rows: readonly unknown[] }).rows;
    assert.equal(rows.length, 0, "door-handwritten.dl6 seeds no event rows");

    const queried_table = await run_bop(["q", "event", "--port", port]);
    assert.equal(queried_table.status, 0, queried_table.stderr);
    assert.equal(queried_table.stdout, "", "zero rows render as zero lines in table mode");
  } finally {
    await served.stop();
  }
});

test("q: nothing listening on the port exits 1 with a clear message, never a stack trace", async () => {
  const idle = String(await reserve_port());
  const outcome = await run_bop(["q", "event", "--port", idle]);
  assert.equal(outcome.status, 1);
  assert.match(outcome.stderr, new RegExp(`no server listening on port ${idle}`));
});

test("load: nothing listening on the port exits 1 with a clear message", async () => {
  const idle = String(await reserve_port());
  const outcome = await run_bop(["load", DOOR_DL6, "--port", idle]);
  assert.equal(outcome.status, 1);
  assert.match(outcome.stderr, new RegExp(`no server listening on port ${idle}`));
});

test("q: a running server with no program loaded exits 1 (404 'no program loaded')", async () => {
  const served = await start_served();
  try {
    const outcome = await run_bop(["q", "event", "--port", String(served.port)]);
    assert.equal(outcome.status, 1, outcome.stdout);
    assert.match(outcome.stderr, /no program loaded/);
  } finally {
    await served.stop();
  }
});

test("load: a program that hits a named compiler unsupported construct over http exits 2, not 1", async () => {
  const served = await start_served();
  try {
    const ghcacher_dl6 = fileURLToPath(new URL("../../dl/fixtures/ghcacher.dl6", import.meta.url));
    const outcome = await run_bop(["load", ghcacher_dl6, "--port", String(served.port)]);
    assert.equal(outcome.status, 2, outcome.stderr);
    assert.match(outcome.stderr, /unsupported_construct/);
  } finally {
    await served.stop();
  }
});

test("stats: bop reads the existing GET /stats route after load", async () => {
  const served = await start_served();
  const port = String(served.port);
  try {
    const loaded = await run_bop(["load", DOOR_DL6, "--port", port]);
    assert.equal(loaded.status, 0, loaded.stderr);
    const outcome = await run_bop(["stats", "--port", port]);
    assert.equal(outcome.status, 0, outcome.stderr);
    const body = JSON.parse(outcome.stdout) as { readonly memory: unknown; readonly sqlite: unknown };
    assert.ok(body.memory);
    assert.ok(body.sqlite);
  } finally {
    await served.stop();
  }
});
