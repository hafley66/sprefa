import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import {
  incremental_plan,
  program,
} from "../gen_emitted/retention_count_prunes_oldest.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";

async function run_retention_tick(row_count: number) {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  stmt_counter.reset();
  const deltas = await firstValueFrom(
    program.tick(
      seam,
      Array.from({ length: row_count }, (_value, index) => ({
        rel: "event",
        sign: "add" as const,
        row: [`row_${index}`],
      })),
    ),
  );
  const statement_count = stmt_counter.get();
  const final_result = await firstValueFrom(
    seam.runner.execute(seam.db, program.final_select.event!),
  );
  const final_rows = final_result.rows.map((row) => String(row.col1)).sort();
  seam.db.close();
  return {
    statement_count,
    final_rows,
    event_delta: deltas.rels.find((delta) => delta.rel === "event"),
  };
}

test("keep(count) lowers to one set-based retention statement", () => {
  assert.deepEqual(
    incremental_plan.retention,
    [
      {
        rel: "event",
        count: 2,
        delete_sql:
          'DELETE FROM "retention_count_prunes_oldest_event" WHERE rowid NOT IN (SELECT rowid FROM "retention_count_prunes_oldest_event" ORDER BY rowid DESC LIMIT 2) RETURNING "col1"',
      },
    ],
  );
});

test("keep(count) statement count is flat and the oldest rows are pruned", async () => {
  const three_rows = await run_retention_tick(3);
  const hundred_rows = await run_retention_tick(100);
  assert.deepEqual(
    {
      statement_counts: [three_rows.statement_count, hundred_rows.statement_count],
      three_final: three_rows.final_rows,
      hundred_final: hundred_rows.final_rows,
      three_delta: three_rows.event_delta,
      hundred_delta_count: hundred_rows.event_delta?.add.length,
    },
    {
      statement_counts: [14, 14],
      three_final: ["row_1", "row_2"],
      hundred_final: ["row_98", "row_99"],
      three_delta: { rel: "event", add: [["row_1"], ["row_2"]], del: [] },
      hundred_delta_count: 2,
    },
  );
});
