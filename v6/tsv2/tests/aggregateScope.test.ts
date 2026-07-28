/**
 * aggregateScope.test.ts — the COUNT/PLAN receipt for group-scoped aggregate
 * maintenance (repo law: "formerly-quadratic paths get COUNT tests ... never
 * end-state equality alone").
 *
 * An aggregate head is maintained by delete-then-recompute, which is the
 * shape that WOULD be a whole-table recompute per tick if the scope table
 * were not doing its job. End-state equality cannot tell the two apart: a
 * whole-table recompute produces exactly the same final rows. So this asserts
 * the PLAN and the ROW COUNTS instead, against the REAL emitted SQL of a
 * compiled fixture (read out of v6/prolog/compile/out/, never hand-written
 * here), on a corpus big enough for scan-vs-search to be visible: 5,000
 * groups x 4 rows = 20,000 derivations, one single row retracted.
 *
 * SABOTAGE RECEIPTS (both actually run against the emitted SQL, results as
 * observed, not as predicted):
 *   1. Dropping `(b0."repo") IN (SELECT "repo" FROM "__agg_scope_stat")` from
 *      insertScopedSql -> the PLAN assertion goes red
 *      ("insertScoped must SEARCH by group key, got: SCAN b0"). The ROW-COUNT
 *      assertion does NOT catch it, and that is worth writing down: the
 *      statement is `INSERT OR IGNORE ... RETURNING`, so the 4,999 unchanged
 *      groups re-derive to rows that already exist, get ignored, and never
 *      reach RETURNING. The count stays 1 while the work goes quadratic. The
 *      plan is the only probe that sees this one.
 *   2. Dropping the same term from deleteScopedSql -> BOTH go red
 *      ("deleteScoped must SEARCH by group key, got: SCAN stat" and "only the
 *      affected group may be deleted"), since a wholesale DELETE ... RETURNING
 *      hands back all 5,000 rows.
 * Without these assertions the fixture still grades IDENTICAL either way,
 * because a whole-table recompute is correct -- only quadratic.
 *
 * min/max are the reason the recompute exists at all rather than incremental
 * arithmetic: the match-frontier lab's rx-directness table records
 * "incremental min/max over a retractable set" as IMPOSSIBLE, since removing
 * the current minimum tells you nothing about the next one. The last
 * assertion is that receipt made concrete -- retracting repo_7's minimum (1)
 * moves min to 2, which only a re-read of the group can produce.
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
const FIXTURE = "aggregate_min_recomputes_when_the_minimum_is_retracted";

function emittedSource(): string {
  return readFileSync(join(COMPILE_OUT, `${FIXTURE}.ts`), "utf8");
}

function emittedDdl(source: string): string[] {
  return [...source.matchAll(/^ {2}`(CREATE [\s\S]*?)`,$/gm)].map((match) => match[1]!);
}

function emittedAggregateSql(source: string, field: string): string {
  const block = source.match(/aggregateSql: \{[\s\S]*?\} \},/)![0];
  return block.match(new RegExp(`${field}: \\[?\`([\\s\\S]*?)\``))![1]!;
}

function run(seam: ISqlSeam, sql: string) {
  return firstValueFrom(seam.runner.execute(seam.db, sql));
}

async function loadedSeam(): Promise<{ seam: ISqlSeam; source: string }> {
  const source = emittedSource();
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, emittedDdl(source)));

  const values: string[] = [];
  for (let repo = 0; repo < 5000; repo += 1) {
    for (let star = 1; star <= 4; star += 1) values.push(`('repo_${repo}', ${star})`);
  }
  await run(seam, `INSERT INTO "star_row" ("repo", "stars") VALUES ${values.join(",")}`);
  await run(
    seam,
    `INSERT INTO "stat" ("repo", "col2", "col3", "col4") SELECT b0."repo", count(*), min(b0."stars"), max(b0."stars") FROM "star_row" b0 GROUP BY b0."repo"`,
  );

  // One tick's worth of change: retract repo_7's minimum, stage it the way
  // IncrementalRuntime.applyArrivals would.
  await run(seam, `DELETE FROM "star_row" WHERE "repo" = 'repo_7' AND "stars" = 1`);
  await run(
    seam,
    `INSERT INTO "__delta_star_row" ("_sign","_sequence","repo","stars") VALUES (-1, 0, 'repo_7', 1)`,
  );
  await run(seam, emittedAggregateSql(source, "scopeClearSql"));
  await run(seam, emittedAggregateSql(source, "scopeSeedSql"));
  return { seam, source };
}

async function planLines(seam: ISqlSeam, sql: string): Promise<string[]> {
  const plan = await run(seam, `EXPLAIN QUERY PLAN ${sql}`);
  return plan.rows.map((row) => String(row.detail));
}

test("aggregate scope seed selects only the groups this tick touched", async () => {
  const { seam } = await loadedSeam();
  const groups = await run(seam, `SELECT count(*) AS c FROM "stat"`);
  assert.equal(Number(groups.rows[0]!.c), 5000, "corpus must be big enough for scan-vs-search to matter");
  const derivations = await run(seam, `SELECT count(*) AS c FROM "star_row"`);
  assert.equal(Number(derivations.rows[0]!.c), 19999);
  const scope = await run(seam, `SELECT count(*) AS c FROM "__agg_scope_stat"`);
  assert.equal(Number(scope.rows[0]!.c), 1, "one changed row must scope exactly one group");
});

test("scoped delete and recompute SEARCH by group key, never SCAN", async () => {
  const { seam, source } = await loadedSeam();
  const deletePlan = await planLines(seam, emittedAggregateSql(source, "deleteScopedSql"));
  const insertPlan = await planLines(seam, emittedAggregateSql(source, "insertScopedSql"));
  for (const [label, lines] of [["deleteScoped", deletePlan], ["insertScoped", insertPlan]] as const) {
    assert.ok(
      lines.some((line) => line.includes("SEARCH")),
      `${label} must SEARCH by group key, got: ${lines.join(" | ")}`,
    );
    assert.ok(
      !lines.some((line) => /\bSCAN\b/.test(line)),
      `${label} must not SCAN the whole table, got: ${lines.join(" | ")}`,
    );
    assert.ok(
      lines.some((line) => line.includes("__agg_scope_stat")),
      `${label} must drive off the scope table, got: ${lines.join(" | ")}`,
    );
  }
});

test("scoped delete and recompute touch one group out of 5000, and min moves", async () => {
  const { seam, source } = await loadedSeam();
  const deleted = await run(seam, emittedAggregateSql(source, "deleteScopedSql"));
  assert.equal(deleted.rows.length, 1, "only the affected group may be deleted");
  const inserted = await run(seam, emittedAggregateSql(source, "insertScopedSql"));
  assert.equal(inserted.rows.length, 1, "only the affected group may be re-derived");
  assert.deepEqual(
    { repo: inserted.rows[0]!.repo, count: inserted.rows[0]!.col2, min: inserted.rows[0]!.col3, max: inserted.rows[0]!.col4 },
    { repo: "repo_7", count: 3, min: 2, max: 4 },
    "retracting the group minimum must move min 1 -> 2, which only a re-read of the group can produce",
  );
  const groups = await run(seam, `SELECT count(*) AS c FROM "stat"`);
  assert.equal(Number(groups.rows[0]!.c), 5000, "every untouched group must survive the scoped delete");
});
