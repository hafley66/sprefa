/**
 * edgeGuard.test.ts — the COUNT/PLAN receipt for the edge-body guard seam
 * (repo law: "formerly-quadratic paths get COUNT tests ... never end-state
 * equality alone").
 *
 * `not(Atom)`, comparisons and `:=` binds became legal in an EDGE body in the
 * phase-3 edge-body arc. Each one lands inside the arm's project_sql: the
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

function emitted_source(fixture: string): string {
  return readFileSync(join(COMPILE_OUT, `${fixture}.ts`), "utf8");
}

function emitted_ddl(source: string): string[] {
  return [...source.matchAll(/^ {2}`(CREATE [\s\S]*?)`,$/gm)].map((match) => match[1]!);
}

/**
 * The incremental (delta-join) project_sql of the arm heading `head_rel` and
 * triggered off `frontierTable`. One rule can lower to several arms sharing a
 * head (one per candidate trigger atom), so the trigger's frontier table is
 * what names the arm.
 */
function emitted_edge_project_sql(source: string, head_rel: string, frontier_table: string): string {
  const line = source
    .split("\n")
    .find(
      (candidate) =>
        candidate.includes(`{ head_rel: "${head_rel}"`) &&
        candidate.includes("project_sql:") &&
        candidate.includes(`"${frontier_table}" d0`),
    );
  assert.ok(line, `no incremental edge statement for ${head_rel} off ${frontier_table}`);
  return line.match(/project_sql: `([\s\S]*?)` \}/)![1]!;
}

function run(seam: ISqlSeam, sql: string) {
  return firstValueFrom(seam.runner.execute(seam.db, sql));
}

async function plan_lines(seam: ISqlSeam, sql: string): Promise<string[]> {
  const plan = await run(seam, `EXPLAIN QUERY PLAN ${sql}`);
  return plan.rows.map((row) => String(row.detail));
}

async function exhaust_policy_seam(live_tab_rows: number): Promise<{ seam: ISqlSeam; source: string }> {
  const source = emitted_source("exhaust_policy");
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, emitted_ddl(source)));

  const values: string[] = [];
  for (let session = 0; session < live_tab_rows; session += 1) {
    values.push(`('session_${session}', 'tab_${session}', 1)`);
  }
  if (values.length > 0) {
    await run(
      seam,
      `INSERT INTO "exhaust_policy_live_tab" ("session_id", "tab_id", "__refcount") VALUES ${values.join(",")}`,
    );
  }
  // One tick's worth of arrival, staged the way IncrementalRuntime.applyArrivals
  // would: session_0 is already live (the guard must reject it), session_new is
  // not (the guard must admit it).
  await run(
    seam,
    `INSERT INTO "__frontier_exhaust_policy_open_request_4790c54b2927" ("_phase","_sequence","session_id","tab_id") VALUES ` +
      `(0, 0, 'session_0', 'tab_again'), (0, 1, 'session_new', 'tab_fresh'), (0, 2, 'session_1', 'tab_again')`,
  );
  return { seam, source };
}

test("edge-body negation SEARCHes the negated rel by key, never SCANs it", async () => {
  const { seam, source } = await exhaust_policy_seam(5000);
  const project_sql = emitted_edge_project_sql(source, "open_tab", "__frontier_exhaust_policy_open_request_4790c54b2927");
  assert.ok(project_sql.includes("NOT EXISTS"), `the arm must carry the negation, got: ${project_sql}`);

  // `n0` is compile_negative_uses/4's alias for the negated rel; sqlite prints
  // the ALIAS in the plan detail, never the table name, so the alias is what
  // these assertions read. A set rel's key is its `UNIQUE (<cols>)` autoindex,
  // not its `__id` PRIMARY KEY, so the reading that matters is that the plan
  // BINDS the key column, whichever index carries it.
  const lines = await plan_lines(seam, project_sql);
  assert.ok(
    lines.some((line) => /\bSEARCH n0 USING\b.*\(session_id=\?\)/.test(line)),
    `negation must SEARCH the negated rel on its key column, got: ${lines.join(" | ")}`,
  );
  assert.ok(
    !lines.some((line) => /\bSCAN n0\b/.test(line)),
    `negation must not SCAN the negated rel, got: ${lines.join(" | ")}`,
  );
});

test("edge-body negation admits exactly the arrivals its guard lets through", async () => {
  const { seam, source } = await exhaust_policy_seam(5000);
  const derived = await run(seam, emitted_edge_project_sql(source, "open_tab", "__frontier_exhaust_policy_open_request_4790c54b2927"));
  assert.equal(derived.rows.length, 1, "only the arrival with no live_tab row may derive");
  assert.equal(String(derived.rows[0]!.session_id), "session_new");
});

test("edge-body negation plan does not change shape as the negated rel grows", async () => {
  const small = await exhaust_policy_seam(10);
  const large = await exhaust_policy_seam(5000);
  const project_sql = emitted_edge_project_sql(small.source, "open_tab", "__frontier_exhaust_policy_open_request_4790c54b2927");
  const small_plan = (await plan_lines(small.seam, project_sql)).join(" | ");
  const large_plan = (await plan_lines(large.seam, project_sql)).join(" | ");
  assert.equal(large_plan, small_plan, "the arm's plan must be flat in the negated rel's size");
});

test("edge-body comparison and bind filter and compute inside the arm", async () => {
  const source = emitted_source("repeat_is_a_self_carry_chain");
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, emitted_ddl(source)));
  await run(
    seam,
    `INSERT INTO "__frontier_repeat_is_a_self_carry_chain_pulse" ("_phase","_sequence","next") VALUES (0,0,1), (0,1,2), (0,2,3), (0,3,4)`,
  );

  const project_sql = emitted_edge_project_sql(source, "pulse", "__frontier_repeat_is_a_self_carry_chain_pulse");
  assert.ok(/\(.*<\s*3\)/.test(project_sql), `the arm must carry the comparison, got: ${project_sql}`);

  const derived = await run(seam, project_sql);
  assert.equal(derived.rows.length, 2, "only next < 3 may derive");
  assert.deepEqual(
    derived.rows.map((row) => Number(row.next)),
    [2, 3],
    "the head arithmetic must compute next + 1 in the projected row",
  );
});
