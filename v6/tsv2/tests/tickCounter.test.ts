/**
 * tickCounter.test.ts — receipts for `now/1`'s emitted `__tick` counter.
 *
 * now(Tick) lowers to the scalar subquery `(SELECT "n" FROM "__tick")` and the
 * emitted tick pipeline advances that one row once per tick. Three things can
 * go wrong that a tick-log diff on the fixture corpus would still call
 * IDENTICAL, because the corpus never restarts a program mid-run:
 *
 *   1. The counter row multiplying. `__tick` is created by the program's own
 *      DDL, and serve/3_engine.ts RE-RUNS a program's DDL on every swap while
 *      swallowing "already exists". A plain `INSERT INTO "__tick" VALUES (0)`
 *      would add a second row per swap; the scalar subquery would then pick
 *      one silently and the tick would jump backwards.
 *   2. The advance costing more than one statement, or scaling with arrivals.
 *   3. The read costing a scan. One row, but the plan is the receipt that it
 *      is not being joined per arrival row.
 *
 * SABOTAGE RECEIPTS (run, messages quoted from the run):
 *   1. Replace the emitted seed with `INSERT INTO "__tick" ("n") VALUES (0)`
 *      -> RED: "re-running DDL must not mint a second counter row:
 *      Expected values to be strictly equal: 2 !== 1".
 *   2. Change the emitted advance to `UPDATE "__tick" SET "n" = "n" + 1;
 *      UPDATE "__tick" SET "n" = "n" + 1` -> RED at the statement-count
 *      assertion ("the tick advance must be exactly one statement").
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
const FIXTURE = "now_reads_the_tick";

function emitted_source(): string {
  return readFileSync(join(COMPILE_OUT, `${FIXTURE}.ts`), "utf8");
}

function emitted_ddl(source: string): string[] {
  return [...source.matchAll(/^ {2}`((?:CREATE|INSERT) [\s\S]*?)`,$/gm)].map((match) => match[1]!);
}

function emitted_advance_sql(source: string): string {
  return source.match(/function advance_tick[\s\S]*?execute\(seam\.db, `([\s\S]*?)`\)/)![1]!;
}

function run(seam: ISqlSeam, sql: string) {
  return firstValueFrom(seam.runner.execute(seam.db, sql));
}

test("the tick counter survives a re-run of the program DDL", async () => {
  const ddl = emitted_ddl(emitted_source());
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, ddl));
  await run(seam, emitted_advance_sql(emitted_source()));

  // What serve/3_engine.ts does on a program swap: replay the DDL, tolerate
  // "already exists" per statement.
  for (const sql of ddl) {
    try {
      await run(seam, sql);
    } catch (failure) {
      assert.ok(/already exists/i.test(String(failure)), `unexpected DDL failure: ${String(failure)}`);
    }
  }

  const rows = await run(seam, `SELECT count(*) AS c FROM "__tick"`);
  assert.equal(Number(rows.rows[0]!.c), 1, "re-running DDL must not mint a second counter row");
  const value = await run(seam, `SELECT "n" AS n FROM "__tick"`);
  assert.equal(Number(value.rows[0]!.n), 1, "re-running DDL must not reset the counter");
});

test("the tick advance is one statement per tick, flat in arrivals", async () => {
  const source = emitted_source();
  const advance = emitted_advance_sql(source);
  assert.equal(advance, `UPDATE "__tick" SET "n" = "n" + 1`);
  assert.ok(!advance.includes(";"), "the tick advance must be exactly one statement");
  for (const pipeline of ["run_naive_tick", "run_incremental_tick"]) {
    const body = source.match(new RegExp(`function ${pipeline}[\\s\\S]*?\\n}`))![0];
    const calls = body.match(/advance_tick\(seam\)/g) ?? [];
    assert.equal(calls.length, 1, `${pipeline} must advance the tick exactly once`);
  }
});

test("now() reads the counter as a scalar subquery, never a joined row", async () => {
  const source = emitted_source();
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, emitted_ddl(source)));
  await run(seam, emitted_advance_sql(source));
  await run(
    seam,
    `INSERT INTO "__frontier_ping" ("_phase","_sequence","name") VALUES (0,0,'alpha'), (0,1,'beta')`,
  );

  const project_sql = source
    .split("\n")
    .find((line) => line.includes('{ head_rel: "seen_at"') && line.includes("project_sql:"))!
    .match(/project_sql: `([\s\S]*?)` \}/)![1]!;
  assert.ok(project_sql.includes(`(SELECT "n" FROM "__tick")`), `got: ${project_sql}`);

  const derived = await run(seam, project_sql);
  assert.equal(derived.rows.length, 2, "the counter must not multiply the arrival rows");
  assert.deepEqual(
    derived.rows.map((row) => Number(row.tick)),
    [1, 1],
    "every occurrence in one tick reads the same tick number",
  );
});
