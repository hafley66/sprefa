/**
 * serveStats.test.ts -- unit coverage for `GET /stats`
 * (runtime/serveStats.ts, serve/4_http.ts), the storage-stats seam the
 * memory-soak driver reads (scripts/memory-soak.ts). Fast and part of
 * `npm test`; the soak itself is gated behind DL_PERF_LOG like receipt (c)
 * (scripts/memory-soak.sh is the entry point).
 *
 * SABOTAGE RECEIPT (run 2026-07-29, reverted): returning `dbstatAvailable:
 * true` unconditionally from `objectBytes`'s `catchError` branch (deleting
 * the `available: false` fallback) does not fail any assertion here --
 * dbstat really is available against this driver, so the negative branch has
 * no fixture that reaches it. The real discriminator for that branch is
 * scripts/memory-soak.ts's own sabotage receipt (its header), which proves
 * the RETENTION side of the soak, not this availability flag.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { postArrivals, postProgram, request, startServed } from "./serveHelpers.ts";
import type { IServeStatsSnapshot } from "../runtime/types.ts";

const DOOR_DL6 = fileURLToPath(new URL("../../dl/fixtures/door-handwritten.dl6", import.meta.url));

test("GET /stats before any program is loaded is a 404, same convention as /idb/:rel", async () => {
  const served = await startServed();
  try {
    const result = await request(served.port, "/stats", "GET");
    assert.equal(result.statusCode, 404);
  } finally {
    await served.stop();
  }
});

test("GET /stats reports process memory always, and dbstat page bytes for requested tables", async () => {
  const source = readFileSync(DOOR_DL6, "utf8");
  const served = await startServed();
  try {
    assert.equal((await postProgram(served.port, source)).statusCode, 200);
    await postArrivals(served.port, [{ rel: "event", sign: "add", row: [1, "boot"] }]);

    const noTables = await request(served.port, "/stats", "GET");
    assert.equal(noTables.statusCode, 200);
    const noTablesBody = JSON.parse(noTables.body) as IServeStatsSnapshot;
    // Memory is unconditional: no program-scoped read is needed for it.
    assert.ok(noTablesBody.memory.rssBytes > 0);
    assert.ok(noTablesBody.memory.heapUsedBytes > 0);
    // A file-level SQLite db always has at least one page.
    assert.ok(noTablesBody.sqlite.pageCount >= 1);
    assert.ok(noTablesBody.sqlite.pageSize > 0);
    assert.equal(noTablesBody.sqlite.dbBytes, noTablesBody.sqlite.pageCount * noTablesBody.sqlite.pageSize);
    // No `tables` query param -> no dbstat round trip is attempted, so the
    // fallback path is never exercised here and `objectBytes` stays empty.
    assert.deepEqual(noTablesBody.sqlite.objectBytes, []);

    const scoped = await request(served.port, "/stats?tables=event,current", "GET");
    assert.equal(scoped.statusCode, 200);
    const scopedBody = JSON.parse(scoped.body) as IServeStatsSnapshot;
    // Verified empirically against @libsql/client 0.17.4 (module header):
    // dbstat is queryable through this driver, so this must be true, not
    // merely well-formed.
    assert.equal(scopedBody.sqlite.dbstatAvailable, true);
    const byName = new Map(scopedBody.sqlite.objectBytes.map((entry) => [entry.name, entry.bytes]));
    assert.ok(byName.has("event"), `expected an "event" entry in ${JSON.stringify(scopedBody.sqlite.objectBytes)}`);
    assert.ok((byName.get("event") ?? 0) > 0);
    // A table this program never declares is silently absent, not a zero row
    // (dbstat only reports objects that exist).
    assert.ok(!byName.has("no_such_table"));
  } finally {
    await served.stop();
  }
});
