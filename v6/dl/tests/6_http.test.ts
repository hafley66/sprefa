/**
 * tests/6_http.test.ts — M6 gate: the http front's route surface + the one unit test
 * a shell golden can't reach (SSE teardown, task 6.2). The full sg-rail curl session
 * (program load -> file_changed -> diag -> fix -> retract -> query -> delta dump)
 * lives in tests/golden/curl-session.sh, not here (min tests max coverage: a golden
 * per epic beats N unit tests; unit tests only for what the golden can't isolate).
 *
 * Every test boots its own server on an ephemeral port (0) against a fresh scratch db
 * (freshDbPath/cleanupDbFile, reused from tests/1_helpers_db.ts — never copied).
 */
import assert from "node:assert/strict";
import http from "node:http";
import { test } from "node:test";

import type { Subscription } from "rxjs";

import { serveDl, type DlServer } from "../src/6_http.ts";
import { cleanupDbFile, freshDbPath } from "./1_helpers_db.ts";
import { waitUntil } from "./2_helpers_hosts.ts";

interface ServerFixture {
  readonly server: DlServer;
  readonly dbPath: string;
  readonly base: string;
  readonly running: Subscription;
}

/** serveDl is cold: this subscription is the test's stand-in for main.ts's terminal
 *  one, and the handle arrives on it as the "listening" event. `firstValueFrom` is not
 *  usable here -- it unsubscribes on the first value, which would close the server. */
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

// A minimal, valid, standalone 2-line program (NOT the sg-rail fixture): two plain
// EDB rel decls, no rules, no hosts. Enough to prove the program-load route surface
// without dragging in the sg host or the corpus fixture.
const TWO_LINE_PROGRAM = `rel person(name: text).
rel greeting(name: text, message: text).
`;

test("route surface: program load returns loaded+rels+minted; bad program returns 400 diags", async () => {
  const fixture = await bootTestServer();
  try {
    const okResponse = await fetch(`${fixture.base}/edb/program`, { method: "POST", body: TWO_LINE_PROGRAM });
    assert.equal(okResponse.status, 200);
    const okBody = (await okResponse.json()) as { loaded: boolean; rels: string[]; minted: string[] };
    assert.equal(okBody.loaded, true);
    assert.ok(okBody.rels.includes("person"));
    assert.ok(okBody.rels.includes("greeting"));
    // spine + diag builtins ride along unconditionally (5_diag.ts's builtinDecls).
    assert.ok(okBody.rels.includes("diag"));
    assert.deepEqual(okBody.minted, []); // no hosts, no probes in this program

    const badResponse = await fetch(`${fixture.base}/edb/program`, {
      method: "POST",
      body: "this is not a valid dl program !!! <<<",
    });
    assert.equal(badResponse.status, 400);
    const badBody = (await badResponse.json()) as { diags: { code: string; message: string }[] };
    assert.ok(Array.isArray(badBody.diags));
    assert.ok(badBody.diags.length > 0);
  } finally {
    await teardownTestServer(fixture);
  }
});

test("SSE teardown: activeSubscribeCount returns to baseline when the client socket closes", async () => {
  const fixture = await bootTestServer();
  try {
    const loadResponse = await fetch(`${fixture.base}/edb/program`, { method: "POST", body: TWO_LINE_PROGRAM });
    assert.equal(loadResponse.status, 200);

    assert.equal(fixture.server.activeSubscribeCount(), 0);

    const clientReq = await new Promise<http.ClientRequest>((resolve, reject) => {
      const req = http.get(`${fixture.base}/subscribe/person`, (res) => {
        assert.equal(res.statusCode, 200);
        assert.match(String(res.headers["content-type"]), /text\/event-stream/);
        resolve(req);
      });
      req.on("error", reject);
    });

    await waitUntil(async () => (fixture.server.activeSubscribeCount() === 1 ? true : undefined));

    clientReq.destroy();

    await waitUntil(async () => (fixture.server.activeSubscribeCount() === 0 ? true : undefined));
    assert.equal(fixture.server.activeSubscribeCount(), 0);
  } finally {
    await teardownTestServer(fixture);
  }
});

test("POST /edb/:rel commits and GET /idb/:rel reads back", async () => {
  const fixture = await bootTestServer();
  try {
    await fetch(`${fixture.base}/edb/program`, {
      method: "POST",
      body: "rel thing(name: text, value: int).\n",
    });

    const insertResponse = await fetch(`${fixture.base}/edb/thing`, {
      method: "POST",
      body: JSON.stringify({ rows: [{ name: "a", value: 1 }] }),
    });
    assert.equal(insertResponse.status, 200);
    const insertReport = (await insertResponse.json()) as { tick: number; changed: [string, number][] };
    assert.ok(insertReport.changed.some(([rel, delta]) => rel === "thing" && delta === 1));

    const readResponse = await fetch(`${fixture.base}/idb/thing`);
    assert.equal(readResponse.status, 200);
    const readBody = (await readResponse.json()) as { rows: { name: string; value: number }[] };
    assert.deepEqual(readBody.rows, [{ name: "a", value: 1 }]);

    // idempotent second POST of the same row: zero deltas.
    const secondInsertResponse = await fetch(`${fixture.base}/edb/thing`, {
      method: "POST",
      body: JSON.stringify({ rows: [{ name: "a", value: 1 }] }),
    });
    const secondInsertReport = (await secondInsertResponse.json()) as { changed: [string, number][] };
    assert.deepEqual(secondInsertReport.changed, []);

    // unknown rel -> 400
    const unknownRelResponse = await fetch(`${fixture.base}/edb/no_such_rel`, {
      method: "POST",
      body: JSON.stringify({ rows: [] }),
    });
    assert.equal(unknownRelResponse.status, 400);

    // unknown rel read -> 404
    const unknownReadResponse = await fetch(`${fixture.base}/idb/no_such_rel`);
    assert.equal(unknownReadResponse.status, 404);
  } finally {
    await teardownTestServer(fixture);
  }
});

test("/query binds literals NULL-safe", async () => {
  const fixture = await bootTestServer();
  try {
    await fetch(`${fixture.base}/edb/program`, {
      method: "POST",
      body: "rel item(name: text, note: text).\n",
    });

    // note is null for this row -- a nullable-ish column left unbound.
    await fetch(`${fixture.base}/edb/item`, {
      method: "POST",
      body: JSON.stringify({ rows: [{ name: "a", note: null }] }),
    });

    const queryResponse = await fetch(`${fixture.base}/query`, {
      method: "POST",
      body: "? item(\"a\", _).",
    });
    assert.equal(queryResponse.status, 200);
    const queryBody = (await queryResponse.json()) as { rows: { name: string; note: string | null }[] };
    assert.deepEqual(queryBody.rows, [{ name: "a", note: null }]);

    // no program loaded -> 409
    const freshFixture = await bootTestServer();
    try {
      const noProgramResponse = await fetch(`${freshFixture.base}/query`, { method: "POST", body: "? item(_).\n" });
      assert.equal(noProgramResponse.status, 409);
    } finally {
      await teardownTestServer(freshFixture);
    }
  } finally {
    await teardownTestServer(fixture);
  }
});
