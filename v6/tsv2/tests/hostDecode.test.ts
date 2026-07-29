/**
 * hostDecode.test.ts — the sh host's JSON-object decode is a NAMED PROJECTION
 * (golden plan phase 2, the seam that lets `sprefa-extract`'s heterogeneous
 * JSONL feed a rel without a `decode/2` lowering).
 *
 * Graded through a REAL host: the program's template is a hermetic `printf`
 * that reproduces the extractor's own interleaved record shapes, so what is
 * measured is the shipped decode path and not a unit-test copy of it. The tick
 * log is then diffed byte-for-byte against the oracle fed the very rows the
 * server pushed (the runtime bridge's total-replay grading).
 *
 * SABOTAGE RECEIPT (run 2026-07-29, reverted): deleting the
 * `carriesEveryColumn` filter in serve/1_hosts.ts (`decodeObjectItems`) makes
 * the two `record=node` lines land as rows with an empty `callee`, and this
 * test fails with 4 picked rows instead of 2 -- plus the oracle diff, since
 * those rows are in the tick log too. Restoring the OLD positional fallback
 * (`Object.values(item)[index]`) is worse and also caught here: the node lines
 * then land carrying their `kind` value ("function"/"lambda") in the callee
 * column, which is failure class 36's cross-contamination in a new costume.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { logOfTicks, oracleLog, postArrivals, postProgram, request, scheduleFromTicks, startServed, tickEvents } from "./serveHelpers.ts";

const JSON_PROJECTION_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-json-projection.dl6", import.meta.url));

async function waitUntil(predicate: () => boolean | Promise<boolean>, what: string, timeoutMs = 10_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!(await predicate())) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
}

test("sh host: a JSON object stream projects by NAME, and a record missing a declared column is no row", async () => {
  const source = readFileSync(JSON_PROJECTION_DL6, "utf8");
  const served = await startServed(17611);
  try {
    const loaded = await postProgram(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);
    const plans = JSON.parse(loaded.body) as { readonly arrivalTargets: readonly string[] };

    await postArrivals(served.port, [{ rel: "seed", sign: "add", row: ["alpha"] }]);
    await waitUntil(async () => {
      const reply = await request(served.port, "/idb/picked", "GET");
      return (JSON.parse(reply.body) as { rows: unknown[] }).rows.length > 0;
    }, "the host's projected rows to land in picked");

    const picked = JSON.parse((await request(served.port, "/idb/picked", "GET")).body) as {
      rows: readonly (readonly (string | number)[])[];
    };
    // FOUR JSON lines out, TWO rows in: the two `record=node` lines carry no
    // `callee` at all, so they are not rows of this rel.
    assert.equal(picked.rows.length, 2, `expected the two site records only, got ${JSON.stringify(picked.rows)}`);
    assert.deepEqual(
      picked.rows.map((row) => row[1]).sort(),
      ["alpha_one", "alpha_two"],
      "each row carries the site record's own callee, never a neighbouring field",
    );

    const outcomes = tickEvents(served.events);
    const replayed = scheduleFromTicks(outcomes, plans.arrivalTargets);
    assert.equal(logOfTicks(outcomes), oracleLog(source, replayed));
  } finally {
    await served.stop();
  }
});
