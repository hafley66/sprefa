/**
 * ordered aggregate cost receipts.
 *
 * The aggregate plan must execute a fixed statement family for one touched
 * group or 1000 touched groups. The scoped grouped INSERT must seek the
 * source relation by its group-key index.
 *
 * SABOTAGE RECEIPTS:
 *
 * - Replacing the scoped INSERT with a whole-table INSERT changes the query
 *   plan from SEARCH to SCAN and fails the EXPLAIN assertion.
 * - Expanding one aggregate tick into one INSERT per group makes the 10 and
 *   1000 group statement counts differ.
 */

import assert from "node:assert/strict";
import { firstValueFrom, toArray } from "rxjs";
import { test } from "node:test";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, ISqlSeam } from "../runtime/types.ts";
import { incremental_plan, program } from "../gen_emitted/ordered_aggregate_retraction_rebuild.ts";

function counting_seam(seam: ISqlSeam): { readonly seam: ISqlSeam; readonly statements: string[] } {
  const statements: string[] = [];
  const record = (statement: { readonly sql: string } | string): void => {
    statements.push(typeof statement === "string" ? statement : statement.sql);
  };
  const runner = {
    ...seam.runner,
    execute(db: typeof seam.db, statement: Parameters<typeof seam.runner.execute>[1]) {
      record(statement);
      return seam.runner.execute(db, statement);
    },
    batch(db: typeof seam.db, batch: Parameters<typeof seam.runner.batch>[1]) {
      for (const statement of batch) record(statement);
      return seam.runner.batch(db, batch);
    },
    executeMultiple(db: typeof seam.db, sql: string) {
      for (const statement of sql.split(";\n")) record(statement);
      return seam.runner.executeMultiple(db, sql);
    },
  };
  return { seam: { db: seam.db, runner }, statements };
}

function group_batch(group_count: number): IArrivalBatch {
  return Array.from({ length: group_count }, (_unused, index) => ({
    rel: "item",
    sign: "add" as const,
    row: [`group_${index}`, 1, `value_${index}`],
  }));
}

async function aggregate_tick_statement_count(group_count: number): Promise<number> {
  const base = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(base, program.ddl));
  await firstValueFrom(BootRunner.run(base, program.boot));
  const counted = counting_seam(base);
  await firstValueFrom(TickFold.run(program, counted.seam, [group_batch(group_count)]).pipe(toArray()));
  return counted.statements.length;
}

test("ordered aggregate maintenance statement count is flat in group count", async () => {
  const ten = await aggregate_tick_statement_count(10);
  const thousand = await aggregate_tick_statement_count(1000);
  assert.equal(thousand, ten, `1000 groups cost ${thousand} statements versus ${ten} for 10 groups`);
});

test("ordered aggregate scoped INSERT uses SEARCH on the source group key", async () => {
  const aggregate = incremental_plan.levels.find((level) => level.head_rel === "ordered_values")?.aggregate_sql;
  assert.ok(aggregate);
  const scoped_insert = aggregate.insert_scoped_sql[0];
  assert.ok(scoped_insert);

  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  const explain = await firstValueFrom(
    seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${scoped_insert}`),
  );
  const details = explain.rows.map((row) => Object.values(row).join(" ")).join("\n");
  assert.match(details, /SEARCH b0/i, details);
  assert.doesNotMatch(details, /SCAN b0/i, details);
});
