/**
 * edgeGuard.test.ts — the COUNT/PLAN receipt for the edge-body guard seam
 * (repo law: "formerly-quadratic paths get COUNT tests ... never end-state
 * equality alone").
 *
 * `not(Atom)`, comparisons and `:=` binds became legal in an EDGE body in the
 * phase-3 edge-body arc. Each one lands inside the arm's projectSql: the
 * negation as `NOT EXISTS (SELECT 1 FROM <rel> n0 WHERE ...)`, the comparison
 * and the bind as WHERE / SELECT expressions. A `NOT EXISTS` correlated on a
 * NON-key column would still answer correctly and would still grade
 * IDENTICAL, while scanning the negated rel once per arrival row. End-state
 * equality cannot see that; the plan can.
 *
 * All SQL here is read out of the REAL emitted modules in
 * v6/prolog/compile/out/, never hand-written in this file.
 *
 * SABOTAGE RECEIPTS. Each probe edits the EMITTED module in
 * v6/prolog/compile/out/, runs this file, and restores it byte-for-byte; the
 * messages quoted are what the run printed, not what it was expected to.
 *   1. Correlate the NOT EXISTS on a non-key column
 *      (`n0."tab_id" = d0."session_id"`) -> plan assertion RED: "negation
 *      must SEARCH the negated rel by key, got: SEARCH d0 USING INDEX
 *      __frontier_open_request_phase (_phase>?) | CORRELATED SCALAR SUBQUERY
 *      1 | SCAN n0 | USE TEMP B-TREE FOR RIGHT PART OF ORDER BY". The
 *      row-count assertion goes red as well on this corpus, since no live_tab
 *      row carries tab_id = 'session_0' -- a mis-correlated NOT EXISTS is
 *      both slower and wrong. The plan probe is the one that would still
 *      speak if the two columns happened to hold the same values.
 *   2. Delete the whole `AND NOT EXISTS (...)` term -> RED at the structural
 *      assertion ("the arm must carry the negation, got: SELECT ... WHERE
 *      d0."_phase" >= 0 ORDER BY ..."), and the derived row count goes 1 -> 3.
 *   3. Delete `(d0."next" < 3)` from repeat_is_a_self_carry_chain's arm ->
 *      RED: "the arm must carry the comparison, got: SELECT (d0."next" + 1)
 *      AS "next" FROM "__frontier_pulse" d0 WHERE d0."_phase" >= 0 ...".
 *
 * The corpus is 5,000 live_tab rows against three arrivals, so a scan and a
 * search are separated by three orders of magnitude of row touches rather
 * than by a coin flip.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { firstValueFrom } from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import type { ISqlSeam } from "../runtime/types.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const COMPILE_OUT = join(HERE, "..", "..", "prolog", "compile", "out");

function emittedSource(fixture: string): string {
  return readFileSync(join(COMPILE_OUT, `${fixture}.ts`), "utf8");
}

function emittedDdl(source: string): string[] {
  return [...source.matchAll(/^ {2}`(CREATE [\s\S]*?)`,$/gm)].map((match) => match[1]!);
}

/**
 * The incremental (delta-join) projectSql of the arm heading `headRel` and
 * triggered off `frontierTable`. One rule can lower to several arms sharing a
 * head (one per candidate trigger atom), so the trigger's frontier table is
 * what names the arm.
 */
function emittedEdgeProjectSql(source: string, headRel: string, frontierTable: string): string {
  const line = source
    .split("\n")
    .find(
      (candidate) =>
        candidate.includes(`{ headRel: "${headRel}"`) &&
        candidate.includes("projectSql:") &&
        candidate.includes(`"${frontierTable}" d0`),
    );
  assert.ok(line, `no incremental edge statement for ${headRel} off ${frontierTable}`);
  return line.match(/projectSql: `([\s\S]*?)` \}/)![1]!;
}

function run(seam: ISqlSeam, sql: string) {
  return firstValueFrom(seam.runner.execute(seam.db, sql));
}

async function planLines(seam: ISqlSeam, sql: string): Promise<string[]> {
  const plan = await run(seam, `EXPLAIN QUERY PLAN ${sql}`);
  return plan.rows.map((row) => String(row.detail));
}

async function exhaustPolicySeam(liveTabRows: number): Promise<{ seam: ISqlSeam; source: string }> {
  const source = emittedSource("exhaust_policy");
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, emittedDdl(source)));

  const values: string[] = [];
  for (let session = 0; session < liveTabRows; session += 1) {
    values.push(`('session_${session}', 'tab_${session}', 1)`);
  }
  if (values.length > 0) {
    await run(
      seam,
      `INSERT INTO "live_tab" ("session_id", "tab_id", "__support_count") VALUES ${values.join(",")}`,
    );
  }
  // One tick's worth of arrival, staged the way IncrementalRuntime.applyArrivals
  // would: session_0 is already live (the guard must reject it), session_new is
  // not (the guard must admit it).
  await run(
    seam,
    `INSERT INTO "__frontier_open_request" ("_phase","_sequence","session_id","tab_id") VALUES ` +
      `(0, 0, 'session_0', 'tab_again'), (0, 1, 'session_new', 'tab_fresh'), (0, 2, 'session_1', 'tab_again')`,
  );
  return { seam, source };
}

test("edge-body negation SEARCHes the negated rel by key, never SCANs it", async () => {
  const { seam, source } = await exhaustPolicySeam(5000);
  const projectSql = emittedEdgeProjectSql(source, "open_tab", "__frontier_open_request");
  assert.ok(projectSql.includes("NOT EXISTS"), `the arm must carry the negation, got: ${projectSql}`);

  // `n0` is compile_negative_uses/4's alias for the negated rel; sqlite prints
  // the ALIAS in the plan detail, never the table name, so the alias is what
  // these assertions read.
  const lines = await planLines(seam, projectSql);
  assert.ok(
    lines.some((line) => /SEARCH n0 USING PRIMARY KEY/.test(line)),
    `negation must SEARCH the negated rel by key, got: ${lines.join(" | ")}`,
  );
  assert.ok(
    !lines.some((line) => /\bSCAN n0\b/.test(line)),
    `negation must not SCAN the negated rel, got: ${lines.join(" | ")}`,
  );
});

test("edge-body negation admits exactly the arrivals its guard lets through", async () => {
  const { seam, source } = await exhaustPolicySeam(5000);
  const derived = await run(seam, emittedEdgeProjectSql(source, "open_tab", "__frontier_open_request"));
  assert.equal(derived.rows.length, 1, "only the arrival with no live_tab row may derive");
  assert.equal(String(derived.rows[0]!.session_id), "session_new");
});

test("edge-body negation plan does not change shape as the negated rel grows", async () => {
  const small = await exhaustPolicySeam(10);
  const large = await exhaustPolicySeam(5000);
  const projectSql = emittedEdgeProjectSql(small.source, "open_tab", "__frontier_open_request");
  const smallPlan = (await planLines(small.seam, projectSql)).join(" | ");
  const largePlan = (await planLines(large.seam, projectSql)).join(" | ");
  assert.equal(largePlan, smallPlan, "the arm's plan must be flat in the negated rel's size");
});

test("edge-body comparison and bind filter and compute inside the arm", async () => {
  const source = emittedSource("repeat_is_a_self_carry_chain");
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, emittedDdl(source)));
  await run(
    seam,
    `INSERT INTO "__frontier_pulse" ("_phase","_sequence","next") VALUES (0,0,1), (0,1,2), (0,2,3), (0,3,4)`,
  );

  const projectSql = emittedEdgeProjectSql(source, "pulse", "__frontier_pulse");
  assert.ok(/\(.*<\s*3\)/.test(projectSql), `the arm must carry the comparison, got: ${projectSql}`);

  const derived = await run(seam, projectSql);
  assert.equal(derived.rows.length, 2, "only next < 3 may derive");
  assert.deepEqual(
    derived.rows.map((row) => Number(row.next)),
    [2, 3],
    "the head arithmetic must compute next + 1 in the projected row",
  );
});
