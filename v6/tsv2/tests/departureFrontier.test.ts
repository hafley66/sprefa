/**
 * departureFrontier.test.ts — the COUNT/PLAN receipt for the signed departure
 * frontier (TICK PHASE ALIGNMENT target 2), and the honest answer on its
 * durability.
 *
 * engine.pl:tick/7 turns each -delta of a LISTENED rel (one some rule binds
 * with `finalize/1`, `listened_departure_refs/2`) into a `dep(Row)` occurrence
 * at T+1. The emitted runtime had no such stream: `stageEvents` copies only
 * `sign === 1` rows into the frontier tables and their DDL has no sign column.
 *
 * FAIL-FIRST RECEIPT, taken with the compiler half already in place and
 * `IncrementalRuntime.stageDepartures` neutered to stage nothing
 * (`const listening = []`), on both fixtures and BOTH emitter modes:
 *   keyed_replace_departs_the_old_row
 *     tick 3 actual {"tick":3,"deltas":{}}
 *     tick 3 oracle {"tick":3,"deltas":{"replaced_value":{"add":[["cli","v1"]],"del":[]}}}
 *     tick 4 actual <missing tick>       (the drain the carry should have minted)
 *     final  actual has no replaced_value at all
 *   departed_fires_next_tick_on_retraction
 *     tick 3 actual <missing tick>, oracle closed_at add [["alpha",3]]
 * After: tick log AND final state byte-identical, both fixtures, both modes.
 *
 * The correctness half is graded there. What the grading cannot see is the
 * COST, and the whole design argument for a separate table per listened rel
 * (rather than a `_sign` column on the shared frontier) is a cost claim: a
 * program with no `finalize` in it must emit exactly the text it emitted
 * before the feature existed. So:
 *
 *   1  A program with no departure arm has NO departure table in its DDL, no
 *      `departureFrontierTableName` on any relation plan, and no
 *      `stageDepartures` call in either tick pipeline. (The standing receipt
 *      is stronger than this test: re-running the whole sweep across this
 *      change rewrote 83 emitted modules and `git diff` reported zero of them
 *      changed. This is that claim pinned where it can fail later.)
 *   2  stageDepartures executes ZERO statements for such a program, ONE (the
 *      clear) for a listened rel whose tick produced no departure, and TWO
 *      (clear + insert) when it did. Never one per row.
 *   3  The arm SEARCHes/scans only its own departure table -- the plan is the
 *      arrival arm's plan with one table swapped.
 *
 * DURABILITY, C7 (match-frontier lab: "the Ti carry set is not durable in
 * either implementation, crash loses pending firings"). This carry INHERITS
 * C7, it does not close it, and test 4 measures that rather than asserting it:
 * `__departure_frontier_*` is a `CREATE TEMP TABLE` alongside `__frontier_*`
 * and `__next_frontier_*`, so a staged departure and a staged ADDITION are
 * lost together on a new connection to the same file. Closing C7 means making
 * the whole carry set durable, which is a different arc; what this arc owed
 * was not making the hazard worse, and the two are exactly as durable as each
 * other.
 *
 * SABOTAGE RECEIPTS (each edit made, this file run, then reverted; the
 * messages quoted are what the run printed):
 *   a. Make `stageDepartures` skip its `DELETE FROM` clear -> 2 of 5 RED:
 *      "a listened rel with no departure is cleared, nothing else: 0
 *      statements", and the fire-once count dies with "tsv2 drain overflow:
 *      keyed_replace_departs_the_old_row exceeded 100 drain ticks". Worth
 *      writing down: the clear is not bookkeeping, it is what makes a
 *      departure fire ONCE instead of on every drain tick forever.
 *   b. Add `departureFrontierTableName` to all 5 relation plans of the
 *      no-finalize fixture -> 2 of 5 RED, the first being "no relation plan
 *      may carry departureFrontierTableName: 5 !== 0".
 */

import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { firstValueFrom, toArray, type Observable } from "rxjs";
import type { ISqlRunner } from "sprefa-store-engine/src/engine/types.ts";

import { IncrementalRuntime } from "../runtime/1_incremental.ts";
import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, IRelDelta, ISqlSeam, SqlStatement } from "../runtime/types.ts";
import {
  incremental_plan as departure_plan,
  program as departure_program,
} from "../gen_emitted/keyed_replace_departs_the_old_row.ts";
import { incremental_plan as plain_plan } from "../gen_emitted/switch_as_keyed_replace.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const COMPILE_OUT = join(HERE, "..", "..", "prolog", "compile", "out");

function emitted_source(fixture: string): string {
  return readFileSync(join(COMPILE_OUT, `${fixture}.ts`), "utf8");
}

function counting_seam(seam: ISqlSeam): { seam: ISqlSeam; statements: string[] } {
  const statements: string[] = [];
  const record = (statement: string | SqlStatement): void => {
    statements.push(typeof statement === "string" ? statement : statement.sql);
  };
  const runner: ISqlRunner = {
    ...seam.runner,
    execute(db, statement) {
      record(statement);
      return seam.runner.execute(db, statement);
    },
    batch(db, batched) {
      for (const statement of batched) record(statement);
      return seam.runner.batch(db, batched);
    },
    executeMultiple(db, sql) {
      for (const statement of sql.split(";\n")) record(statement);
      return seam.runner.executeMultiple(db, sql);
    },
  };
  return { seam: { db: seam.db, runner }, statements };
}

function booted_seam(url = ":memory:"): Promise<ISqlSeam> {
  const seam = ScratchStore.open(url);
  return firstValueFrom(ScratchStore.boot(seam, departure_program.ddl)).then(() => seam);
}

const DEPARTED: readonly IRelDelta[] = [{ rel: "latest", add: [], del: [["cli", "v1"]] }];
const NOTHING_DEPARTED: readonly IRelDelta[] = [{ rel: "latest", add: [["cli", "v2"]], del: [] }];

test("count: a program with no finalize emits no departure table anywhere", () => {
  const source = emitted_source("switch_as_keyed_replace");
  const departure_mentions = [...source.matchAll(/__departure_frontier_/g)].length;
  assert.equal(
    departure_mentions,
    0,
    `a program with no finalize must not emit a departure table, got ${departure_mentions}`,
  );
  assert.equal(
    plain_plan.relations.filter((relation) => relation.departure_frontier_table_name !== undefined).length,
    0,
    "no relation plan may carry departureFrontierTableName",
  );
  assert.ok(
    !source.includes("stageDepartures"),
    "neither tick pipeline may call stageDepartures",
  );
  // ... and the listening program does carry all three, so the assertions
  // above are discriminating rather than vacuously true of every fixture.
  const listening = emitted_source("keyed_replace_departs_the_old_row");
  assert.ok(listening.includes(`departure_frontier_table_name: "__departure_frontier_latest"`));
  assert.ok(listening.includes("IncrementalRuntime.stage_departures"));
});

test("count: staging is one clear, plus one insert only when something departed", async () => {
  const base = await booted_seam();

  const empty = counting_seam(base);
  await firstValueFrom(
    IncrementalRuntime.stage_departures(empty.seam, plain_plan.relations, DEPARTED),
  );
  assert.equal(
    empty.statements.length,
    0,
    `a program with no listened rel stages nothing: ${empty.statements.length} statements`,
  );

  const quiet = counting_seam(base);
  await firstValueFrom(
    IncrementalRuntime.stage_departures(quiet.seam, departure_plan.relations, NOTHING_DEPARTED),
  );
  assert.equal(
    quiet.statements.length,
    1,
    `a listened rel with no departure is cleared, nothing else: ${quiet.statements.length} statements`,
  );
  assert.match(quiet.statements[0]!, /^DELETE FROM "__departure_frontier_latest"$/);

  const staged = counting_seam(base);
  await firstValueFrom(
    IncrementalRuntime.stage_departures(staged.seam, departure_plan.relations, DEPARTED),
  );
  assert.equal(
    staged.statements.length,
    2,
    `clear + one set-based insert, never one statement per row: ${staged.statements.length} statements`,
  );
  const rows = await firstValueFrom(
    base.runner.execute(base.db, `SELECT "key", "value" FROM "__departure_frontier_latest"`),
  );
  assert.deepEqual(rows.rows.map((row) => [row.key, row.value]), [["cli", "v1"]]);
});

test("plan: the departure arm reads only its own departure table", async () => {
  const seam = await booted_seam();
  const arm = departure_plan.edges.find((edge) => edge.project_sql.includes("__departure_frontier_"))!;
  assert.ok(arm !== undefined, "the fixture must have a departure arm");
  const explained = await firstValueFrom(
    seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${arm.project_sql}`),
  );
  const plan = explained.rows.map((row) => String(row.detail)).join(" | ");
  assert.ok(
    !/\b(SCAN|SEARCH) (?!d0\b)/.test(plan),
    `the departure arm must touch only its own delta table, got: ${plan}`,
  );
  assert.ok(
    plan.includes("d0"),
    `the departure arm must drive off the departure table, got: ${plan}`,
  );
});

/** C7, measured. Both carry tables are TEMP; a new connection to the same file
 *  sees neither. The claim under test is the SAMENESS, not that either
 *  survives. */
test("endurance: a staged departure is exactly as durable as a staged addition", async () => {
  const directory = mkdtempSync(join(tmpdir(), "tsv2-departure-"));
  const url = `file:${join(directory, "db.sqlite")}`;
  try {
    const seam = await booted_seam(url);
    await firstValueFrom(
      IncrementalRuntime.stage_departures(seam, departure_plan.relations, DEPARTED),
    );
    const before = await firstValueFrom(
      seam.runner.execute(seam.db, `SELECT count(*) AS n FROM "__departure_frontier_latest"`),
    );
    assert.equal(Number(before.rows[0]!.n), 1, "the departure must be staged before the reopen");

    const carry_tables_sql =
      `SELECT name FROM temp.sqlite_master WHERE type = 'table' AND name LIKE '%frontier%' ORDER BY name`;
    const live = await firstValueFrom(seam.runner.execute(seam.db, carry_tables_sql));
    const live_names = live.rows.map((row) => String(row.name));
    assert.ok(
      live_names.includes("__frontier_latest") && live_names.includes("__departure_frontier_latest"),
      `the probe must see both carry tables while the connection is open, got: ${live_names.join(", ")}`,
    );

    // A fresh connection is what a restart looks like from SQLite's side.
    const reopened = ScratchStore.open(url);
    const after_restart = await firstValueFrom(reopened.runner.execute(reopened.db, carry_tables_sql));
    assert.deepEqual(
      after_restart.rows.map((row) => String(row.name)),
      [],
      "C7 inherited, not closed: the arrival frontier and the departure frontier are both TEMP, and a new connection sees neither",
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

/** The end-to-end shape the fixtures grade, restated as a count: the departure
 *  fires ONCE, on the tick AFTER the replace (update-arm verdict), and the
 *  carry it minted is a real drain tick rather than a silent stall. */
test("count: a departure fires exactly one tick after the row left", async () => {
  const seam = await booted_seam();
  await firstValueFrom(BootRunner.run(seam, departure_program.boot));
  const schedule = JSON.parse(
    readFileSync(join(COMPILE_OUT, "keyed_replace_departs_the_old_row.schedule.json"), "utf8"),
  ) as readonly IArrivalBatch[];
  const lines = await firstValueFrom(
    (TickFold.run(departure_program, seam, schedule).pipe(toArray()) as Observable<string[]>),
  );
  assert.equal(schedule.length, 2, "fixture shape assumed below");
  assert.equal(lines.length, 4, `2 scheduled ticks + the departure drain + its own carry drain: ${lines.length}`);
  assert.ok(!lines[1]!.includes("replaced_value"), "the replace tick itself derives nothing");
  assert.match(lines[2]!, /"replaced_value":\{"add":\[\["cli","v1"\]\]/);
  assert.equal(lines[3], `{"tick":4,"deltas":{}}`);
});
