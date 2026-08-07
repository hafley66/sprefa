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

import { post_arrivals, post_program, request, start_served } from "./serveHelpers.ts";
import type { IServeStatsSnapshot } from "../runtime/types.ts";

const DOOR_DL6 = fileURLToPath(new URL("../../dl/fixtures/door-handwritten.dl6", import.meta.url));

test("GET /stats before any program is loaded is a 404, same convention as /idb/:rel", async () => {
  const served = await start_served();
  try {
    const result = await request(served.port, "/stats", "GET");
    assert.equal(result.statusCode, 404);
  } finally {
    await served.stop();
  }
});

test("GET /stats reports process memory always, and dbstat page bytes for requested tables", async () => {
  const source = readFileSync(DOOR_DL6, "utf8");
  const served = await start_served();
  try {
    assert.equal((await post_program(served.port, source)).statusCode, 200);
    await post_arrivals(served.port, [{ rel: "event", sign: "add", row: [1, "boot"] }]);

    const no_tables = await request(served.port, "/stats", "GET");
    assert.equal(no_tables.statusCode, 200);
    const no_tables_body = JSON.parse(no_tables.body) as IServeStatsSnapshot;
    // Memory is unconditional: no program-scoped read is needed for it.
    assert.ok(no_tables_body.memory.rss_bytes > 0);
    assert.ok(no_tables_body.memory.heap_used_bytes > 0);
    // A file-level SQLite db always has at least one page.
    assert.ok(no_tables_body.sqlite.page_count >= 1);
    assert.ok(no_tables_body.sqlite.page_size > 0);
    assert.equal(no_tables_body.sqlite.db_bytes, no_tables_body.sqlite.page_count * no_tables_body.sqlite.page_size);
    // No `tables` query param -> no dbstat round trip is attempted, so the
    // fallback path is never exercised here and `objectBytes` stays empty.
    assert.deepEqual(no_tables_body.sqlite.object_bytes, []);

    const scoped = await request(served.port, "/stats?tables=event,current", "GET");
    assert.equal(scoped.statusCode, 200);
    const scoped_body = JSON.parse(scoped.body) as IServeStatsSnapshot;
    // Verified empirically against @libsql/client 0.17.4 (module header):
    // dbstat is queryable through this driver, so this must be true, not
    // merely well-formed.
    assert.equal(scoped_body.sqlite.dbstat_available, true);
    const by_name = new Map(scoped_body.sqlite.object_bytes.map((entry) => [entry.name, entry.bytes]));
    assert.ok(by_name.has("event"), `expected an "event" entry in ${JSON.stringify(scoped_body.sqlite.object_bytes)}`);
    assert.ok((by_name.get("event") ?? 0) > 0);
    // A table this program never declares is silently absent, not a zero row
    // (dbstat only reports objects that exist).
    assert.ok(!by_name.has("no_such_table"));
  } finally {
    await served.stop();
  }
});
